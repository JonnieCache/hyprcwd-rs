mod error;
mod hyprland;
mod kitty;

use clap::Parser;
use error::{HyprCwdError as Error, HyprCwdResult as Result};
use hyprland::active_window_pid;
use procfs::process::{Process, Stat};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::path::{Path, PathBuf};
use std::process::exit;

#[derive(Parser)]
struct Args {
    /// Directory to be printed if no active window is found
    #[arg(short, long, value_name = "DIR")]
    default_dir: Option<PathBuf>,

    /// Kitty UNIX socket path/address; supports {kitty_pid}
    #[arg(long, value_name = "SOCKET")]
    kitty_socket: Option<String>,
}

fn main() {
    let args = Args::parse();

    match (
        active_window_cwd(args.kitty_socket.as_deref()),
        args.default_dir,
    ) {
        (Ok(working_dir), _) => {
            println!("{}", working_dir);
        }

        (Err(Error::NoActiveWindow), Some(default_dir)) => {
            println!("{}", default_dir.to_string_lossy());
        }

        (Err(err), _) => {
            eprintln!("{}", err);
            exit(1);
        }
    };
}

fn active_window_cwd(kitty_socket: Option<&str>) -> Result<String> {
    let window_pid = active_window_pid()?;
    if let Some(cwd) = kitty::focused_window_cwd(window_pid, kitty_socket)? {
        return if Path::new(&cwd).is_dir() {
            Ok(cwd)
        } else {
            home_dir()
        };
    }

    let candidate_pids = cwd_candidate_pids(window_pid)?;

    for pid in candidate_pids {
        match process_cwd(pid) {
            Ok(cwd) => {
                return Ok(cwd);
            }
            Err(_err) => {}
        }
    }

    home_dir()
}

fn cwd_candidate_pids(window_pid: i32) -> Result<Vec<i32>> {
    let all_processes = procfs::process::all_processes()?;
    let mut child_processes = HashMap::new();
    let mut window_process = None;

    for process in all_processes.flatten() {
        let Ok(stat) = process.stat() else {
            continue;
        };

        if stat.pid == window_pid {
            window_process = Some(stat.clone());
        }

        child_processes
            .entry(stat.ppid)
            .or_insert_with(Vec::new)
            .push(stat);
    }

    let Some(window_process) = window_process else {
        return Ok(vec![window_pid]);
    };

    let process_tree = process_tree(window_process, &child_processes);

    let mut candidates = Vec::new();
    let mut seen = HashSet::new();

    let mut foreground_processes = process_tree
        .iter()
        .filter(|process| process.stat.tty_nr != 0)
        .filter(|process| process.stat.tpgid > 0)
        .filter(|process| process.stat.pgrp == process.stat.tpgid)
        .collect::<Vec<_>>();

    foreground_processes.sort_by_key(|process| {
        (
            process.stat.pid != process.stat.tpgid,
            Reverse(process.depth),
            Reverse(process.stat.starttime),
        )
    });

    for process in foreground_processes {
        push_candidate(&mut candidates, &mut seen, process.stat.pid);
    }

    let mut tty_processes = process_tree
        .iter()
        .filter(|process| process.stat.tty_nr != 0)
        .collect::<Vec<_>>();

    tty_processes.sort_by_key(|process| (Reverse(process.depth), Reverse(process.stat.starttime)));

    for process in tty_processes {
        push_candidate(&mut candidates, &mut seen, process.stat.pid);
    }

    push_candidate(&mut candidates, &mut seen, window_pid);

    Ok(candidates)
}

#[derive(Debug)]
struct ProcessTreeEntry {
    stat: Stat,
    depth: usize,
}

fn process_tree(root: Stat, child_processes: &HashMap<i32, Vec<Stat>>) -> Vec<ProcessTreeEntry> {
    let mut entries = Vec::new();
    let mut queue = VecDeque::from([(root, 0)]);

    while let Some((stat, depth)) = queue.pop_front() {
        if let Some(children) = child_processes.get(&stat.pid) {
            for child in children {
                queue.push_back((child.clone(), depth + 1));
            }
        }

        entries.push(ProcessTreeEntry { stat, depth });
    }

    entries
}

fn push_candidate(candidates: &mut Vec<i32>, seen: &mut HashSet<i32>, pid: i32) {
    if seen.insert(pid) {
        candidates.push(pid);
    }
}

fn process_cwd(pid: i32) -> Result<String> {
    let process = Process::new(pid)?;
    let cwd = process.cwd()?;

    if cwd.exists() && cwd.is_dir() {
        Ok(cwd.to_string_lossy().to_string())
    } else {
        home_dir()
    }
}

fn home_dir() -> Result<String> {
    let home = env::var("HOME").map_err(|source| Error::EnvVarError {
        name: "HOME",
        source,
    })?;

    Ok(home)
}
