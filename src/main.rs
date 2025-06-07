use anyhow::Result;
use hyprland::data::Client;
use hyprland::shared::HyprDataActiveOptional;
use procfs::process::Process;
use std::env;

fn main() -> Result<()> {
    let working_dir = active_window_cwd()?;
    println!("{}", working_dir);
    Ok(())
}

fn active_window_cwd() -> Result<String> {
    let active_window =
        Client::get_active()?.ok_or_else(|| anyhow::anyhow!("No active window found"))?;
    let window_pid = active_window.pid;
    let child_pid = newest_child_process(window_pid)?;

    process_cwd(child_pid).or_else(|_| home_dir())
}

fn newest_child_process(parent_pid: i32) -> Result<i32> {
    let mut newest_pid = parent_pid;
    let mut newest_start_time = 0u64;
    let all_processes = procfs::process::all_processes()?;

    for process in all_processes.flatten() {
        if let Ok(stat) = process.stat() {
            if stat.ppid == parent_pid {
                if stat.starttime > newest_start_time {
                    newest_start_time = stat.starttime;
                    newest_pid = process.pid;
                }
            }
        }
    }

    Ok(newest_pid)
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
    env::var("HOME").map_err(|e| anyhow::anyhow!("Could not get HOME directory: {}", e))
}
