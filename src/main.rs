mod error;

use clap::Parser;
use error::{HyprCwdError as Error, HyprCwdResult as Result};
use procfs::process::{Process, Stat};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet, VecDeque};
use std::env::{self, VarError};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::exit;

macro_rules! debug_log {
    ($($arg:tt)*) => {{
        #[cfg(debug_assertions)]
        eprintln!("[hyprcwd debug] {}", format_args!($($arg)*));
    }};
}

#[derive(Parser)]
struct Args {
    /// Directory to be printed if no active window is found
    #[arg(short, long, value_name = "DIR")]
    default_dir: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();

    match (active_window_cwd(), args.default_dir) {
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

fn active_window_cwd() -> Result<String> {
    debug_log!("starting active window cwd lookup");

    let window_pid = active_window_pid()?;
    debug_log!("active window pid: {window_pid}");

    let candidate_pids = cwd_candidate_pids(window_pid)?;
    debug_log!("cwd candidate pid order: {candidate_pids:?}");

    for pid in candidate_pids {
        match process_cwd(pid) {
            Ok(cwd) => {
                debug_log!("cwd lookup returned from pid {pid}: {cwd}");
                return Ok(cwd);
            }
            Err(_err) => {
                debug_log!("cwd lookup for pid {pid} failed: {_err}");
            }
        }
    }

    debug_log!("all cwd candidates failed; falling back to HOME");
    home_dir()
}

fn active_window_pid() -> Result<i32> {
    let response = hyprland_request("j/activewindow")?;
    debug_log!("activewindow response: {response}");

    let active_window: serde_json::Value = serde_json::from_str(&response)?;

    let Some(pid) = active_window.get("pid") else {
        debug_log!("activewindow response has no pid field");
        return Err(Error::NoActiveWindow);
    };

    debug_log!("activewindow pid field: {pid}");

    let parsed_pid = pid
        .as_i64()
        .and_then(|pid| i32::try_from(pid).ok())
        .ok_or(Error::InvalidActiveWindowPid)?;

    debug_log!("parsed active window pid: {parsed_pid}");

    Ok(parsed_pid)
}

fn hyprland_request(command: &str) -> Result<String> {
    let socket_path = hyprland_socket_path()?;
    debug_log!(
        "connecting to Hyprland socket {} with command {command}",
        socket_path.display()
    );

    let mut stream = UnixStream::connect(socket_path)?;
    stream.write_all(command.as_bytes())?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;

    debug_log!("Hyprland IPC response length: {} bytes", response.len());

    Ok(String::from_utf8(response)?)
}

fn hyprland_socket_path() -> Result<PathBuf> {
    let instance_signature = env::var("HYPRLAND_INSTANCE_SIGNATURE").map_err(|err| match err {
        VarError::NotPresent => Error::NoHyprlandInstance,
        err @ VarError::NotUnicode(_) => Error::EnvVarError {
            name: "HYPRLAND_INSTANCE_SIGNATURE",
            source: err,
        },
    })?;

    let mut path = if let Some(runtime_dir) = env::var_os("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir)
    } else if let Some(uid) = env::var_os("UID") {
        PathBuf::from("/run/user").join(uid)
    } else {
        return Err(Error::NoRuntimeDir);
    };

    path.push("hypr");
    path.push(instance_signature);
    path.push(".socket.sock");

    debug_log!("resolved Hyprland socket path: {}", path.display());

    Ok(path)
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
        debug_log!("window process {window_pid} disappeared; trying it directly");
        return Ok(vec![window_pid]);
    };

    let process_tree = process_tree(window_process, &child_processes);
    debug_log!(
        "found {} process(es) in active window process tree",
        process_tree.len()
    );

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
        debug_log!(
            "foreground tty candidate: pid={} comm={} depth={} pgrp={} tpgid={} tty={} starttime={}",
            process.stat.pid,
            process.stat.comm,
            process.depth,
            process.stat.pgrp,
            process.stat.tpgid,
            process.stat.tty_nr,
            process.stat.starttime
        );
        push_candidate(&mut candidates, &mut seen, process.stat.pid);
    }

    let mut tty_processes = process_tree
        .iter()
        .filter(|process| process.stat.tty_nr != 0)
        .collect::<Vec<_>>();

    tty_processes.sort_by_key(|process| (Reverse(process.depth), Reverse(process.stat.starttime)));

    for process in tty_processes {
        debug_log!(
            "tty fallback candidate: pid={} comm={} depth={} pgrp={} tpgid={} tty={} starttime={}",
            process.stat.pid,
            process.stat.comm,
            process.depth,
            process.stat.pgrp,
            process.stat.tpgid,
            process.stat.tty_nr,
            process.stat.starttime
        );
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
        debug_log!(
            "process tree entry: pid={} comm={} ppid={} depth={} pgrp={} tpgid={} tty={} starttime={}",
            stat.pid,
            stat.comm,
            stat.ppid,
            depth,
            stat.pgrp,
            stat.tpgid,
            stat.tty_nr,
            stat.starttime
        );

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

    debug_log!("pid {pid} cwd candidate: {}", cwd.display());

    if cwd.exists() && cwd.is_dir() {
        Ok(cwd.to_string_lossy().to_string())
    } else {
        debug_log!(
            "pid {pid} cwd candidate is not an existing directory: {}",
            cwd.display()
        );
        home_dir()
    }
}

fn home_dir() -> Result<String> {
    let home = env::var("HOME").map_err(|source| Error::EnvVarError {
        name: "HOME",
        source,
    })?;

    debug_log!("using HOME fallback: {home}");

    Ok(home)
}
