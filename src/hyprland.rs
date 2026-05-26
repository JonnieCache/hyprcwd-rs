use crate::error::{HyprCwdError as Error, HyprCwdResult as Result};
use std::env::{self, VarError};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

pub fn active_window_pid() -> Result<i32> {
    let response = request("j/activewindow")?;
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

fn request(command: &str) -> Result<String> {
    let socket_path = socket_path()?;
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

fn socket_path() -> Result<PathBuf> {
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
