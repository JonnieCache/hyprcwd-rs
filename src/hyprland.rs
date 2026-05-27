use crate::error::{HyprCwdError as Error, HyprCwdResult as Result};
use std::env::{self, VarError};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

pub fn active_window_pid() -> Result<i32> {
    let response = request("j/activewindow")?;

    let active_window: serde_json::Value = serde_json::from_str(&response)?;

    let Some(pid) = active_window.get("pid") else {
        return Err(Error::NoActiveWindow);
    };

    let parsed_pid = pid
        .as_i64()
        .and_then(|pid| i32::try_from(pid).ok())
        .ok_or(Error::InvalidActiveWindowPid)?;

    Ok(parsed_pid)
}

fn request(command: &str) -> Result<String> {
    let socket_path = socket_path()?;
    let mut stream = UnixStream::connect(socket_path)?;
    stream.write_all(command.as_bytes())?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;

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

    Ok(path)
}
