use crate::error::{HyprCwdError as Error, HyprCwdResult as Result};
use crate::hyprland::runtime_dir;
use serde_json::Value;
use std::io::{ErrorKind, Read, Write};
use std::net::Shutdown;
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixStream};
use std::path::Path;
use std::time::Duration;

const PREFIX: &[u8] = b"\x1bP@kitty-cmd";
const SUFFIX: &[u8] = b"\x1b\\";
const TIMEOUT: Duration = Duration::from_millis(250);
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

pub fn focused_window_cwd(
    window_pid: i32,
    configured_socket: Option<&str>,
) -> Result<Option<String>> {
    let (address, name) = socket_address(window_pid, configured_socket)?;
    let mut stream = match UnixStream::connect_addr(&address) {
        Ok(stream) => stream,
        // Ordinary applications and Kitty without a remote-control socket keep
        // the generic process-tree lookup. Ignore stale sockets as well.
        Err(err)
            if matches!(
                err.kind(),
                ErrorKind::NotFound | ErrorKind::ConnectionRefused
            ) =>
        {
            return Ok(None);
        }
        Err(err) => return Err(Error::KittyError(format!("{name}: {err}"))),
    };

    // A fixed socket may belong to a different instance from the active
    // application. Verify its owner before sending any remote-control command.
    let server_pid = rustix::net::sockopt::socket_peercred(&stream)
        .map_err(std::io::Error::from)?
        .pid
        .as_raw_pid();
    if server_pid != window_pid {
        return Ok(None);
    }

    request_focused_cwd(&mut stream)
        .map(Some)
        .map_err(|err| Error::KittyError(format!("{name}: {err}")))
}

fn socket_address(
    window_pid: i32,
    configured_socket: Option<&str>,
) -> Result<(SocketAddr, String)> {
    if let Some(template) = configured_socket {
        let name = template.replace("{kitty_pid}", &window_pid.to_string());
        let path = name.strip_prefix("unix:").unwrap_or(&name);
        let address = if let Some(abstract_name) = path.strip_prefix('@') {
            SocketAddr::from_abstract_name(abstract_name)?
        } else {
            SocketAddr::from_pathname(path)?
        };
        Ok((address, name))
    } else {
        let path = runtime_dir()?.join(format!("kitty-{window_pid}"));
        Ok((
            SocketAddr::from_pathname(&path)?,
            path.display().to_string(),
        ))
    }
}

fn request_focused_cwd(stream: &mut UnixStream) -> Result<String> {
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    // Filter inside Kitty to avoid transferring every pane's environment.
    stream.write_all(
        b"\x1bP@kitty-cmd{\"cmd\":\"ls\",\"version\":[0,26,0],\"payload\":{\"match\":\"state:focused\"}}\x1b\\",
    )?;
    stream.shutdown(Shutdown::Write)?;

    let mut response = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            return Err(Error::KittyError(
                "incomplete remote-control response".into(),
            ));
        }
        response.extend_from_slice(&chunk[..count]);
        if response.len() > MAX_RESPONSE_BYTES {
            return Err(Error::KittyError(
                "remote-control response is too large".into(),
            ));
        }
        if response.ends_with(SUFFIX) {
            break;
        }
    }
    let json = response
        .strip_prefix(PREFIX)
        .and_then(|response| response.strip_suffix(SUFFIX))
        .ok_or_else(|| Error::KittyError("invalid remote-control response".into()))?;
    let response: Value = serde_json::from_slice(json)?;
    if response["ok"] != true {
        return Err(Error::KittyError(
            response["error"]
                .as_str()
                .unwrap_or("remote control failed")
                .into(),
        ));
    }
    let data = response["data"]
        .as_str()
        .ok_or_else(|| Error::KittyError("window list missing from response".into()))?;
    focused_cwd(&serde_json::from_str(data)?)
}

fn focused_cwd(os_windows: &Value) -> Result<String> {
    let selected = os_windows
        .as_array()
        .into_iter()
        .flatten()
        .filter(|window| window["is_focused"] == true)
        .filter_map(|window| window["tabs"].as_array())
        .flatten()
        .filter(|tab| tab["is_active"] == true)
        .filter_map(|tab| tab["windows"].as_array())
        .flatten()
        .find(|window| window["is_active"] == true);

    let selected = selected.ok_or_else(|| {
        Error::KittyError("no focused Kitty pane found (focus may have changed)".into())
    })?;

    // Kitty already resolves the pane's foreground process group. Match its
    // own cwd selection policy: prefer the newest foreground process, then
    // the pane's original child. No second /proc scan is needed.
    let foreground_cwd = selected["foreground_processes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|process| {
            let pid = i32::try_from(process["pid"].as_i64()?).ok()?;
            (pid > 0).then_some((pid, cwd_field(process)?))
        })
        .max_by_key(|(pid, _)| *pid)
        .map(|(_, cwd)| cwd);

    foreground_cwd
        .or_else(|| cwd_field(selected))
        .map(str::to_owned)
        .ok_or_else(|| Error::KittyError("focused pane did not report a working directory".into()))
}

fn cwd_field(process: &Value) -> Option<&str> {
    process["cwd"]
        .as_str()
        .filter(|cwd| Path::new(cwd).is_absolute())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::thread;

    #[test]
    fn fixed_socket_for_another_process_receives_no_command() {
        let name = format!("hyprcwd-peer-test-{}", std::process::id());
        let address = SocketAddr::from_abstract_name(&name).unwrap();
        let listener = std::os::unix::net::UnixListener::bind_addr(&address).unwrap();
        let other_pid = i32::try_from(std::process::id()).unwrap() + 1;
        assert!(
            focused_window_cwd(other_pid, Some(&format!("unix:@{name}")))
                .unwrap()
                .is_none()
        );
        let (mut peer, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        peer.read_to_end(&mut request).unwrap();
        assert!(request.is_empty());
    }

    fn windows() -> Value {
        json!([
            {"is_focused": false, "tabs": [{"is_active": true, "windows": [
                {"is_active": true, "pid": 10, "cwd": "/one", "title": "same title"}
            ]}]},
            {"is_focused": true, "tabs": [
                {"is_active": false, "windows": [
                    {"is_active": true, "is_focused": true, "pid": 20, "cwd": "/hidden"}
                ]},
                {"is_active": true, "windows": [
                    {"is_active": false, "pid": 30, "cwd": "/inactive"},
                    {"is_active": true, "pid": 40, "cwd": "/two", "title": "same title"}
                ]}
            ]}
        ])
    }

    #[test]
    fn selects_focused_os_window_active_tab_and_pane() {
        assert_eq!(focused_cwd(&windows()).unwrap(), "/two");
    }

    #[test]
    fn does_not_use_last_active_window_when_kitty_is_unfocused() {
        let mut windows = windows();
        windows[1]["is_focused"] = json!(false);
        assert!(focused_cwd(&windows).is_err());
    }

    #[test]
    fn rejects_missing_invalid_and_relative_directories() {
        for cwd in [json!(null), json!(""), json!("relative"), json!(0)] {
            let mut windows = windows();
            windows[1]["tabs"][1]["windows"][1]["cwd"] = cwd;
            assert!(focused_cwd(&windows).is_err());
        }
    }

    #[test]
    fn prefers_newest_readable_foreground_process_directory() {
        let mut windows = windows();
        windows[1]["tabs"][1]["windows"][1]["foreground_processes"] = json!([
            {"pid": 60, "cwd": "/foreground"},
            {"pid": 50, "cwd": "/older"},
            {"pid": 70, "cwd": null},
            {"pid": -1, "cwd": "/invalid"}
        ]);
        assert_eq!(focused_cwd(&windows).unwrap(), "/foreground");
        windows[1]["tabs"][1]["windows"][1]["foreground_processes"] = json!([]);
        assert_eq!(focused_cwd(&windows).unwrap(), "/two");
    }

    fn mock_request(response: Value) -> Result<String> {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let responder = thread::spawn(move || {
            server
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut request = Vec::new();
            server.read_to_end(&mut request).unwrap();
            let request: Value = serde_json::from_slice(
                request
                    .strip_prefix(PREFIX)
                    .unwrap()
                    .strip_suffix(SUFFIX)
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(request["cmd"], "ls");
            assert_eq!(request["payload"]["match"], "state:focused");
            let mut frame = PREFIX.to_vec();
            frame.extend(serde_json::to_vec(&response).unwrap());
            frame.extend(SUFFIX);
            // Exercise reads with a frame larger than the receive buffer, and
            // allow the terminator to arrive separately from the JSON.
            server.write_all(&frame[..frame.len() - 1]).unwrap();
            server.write_all(&frame[frame.len() - 1..]).unwrap();
        });
        let result = request_focused_cwd(&mut client);
        responder.join().unwrap();
        result
    }

    #[test]
    fn reads_framed_remote_control_response() {
        let mut windows = windows();
        windows[1]["tabs"][1]["windows"][1]["title"] = json!("a".repeat(8192));
        assert_eq!(
            mock_request(json!({"ok": true, "data": windows.to_string()})).unwrap(),
            "/two"
        );
    }

    #[test]
    fn reports_remote_control_disabled() {
        let result = mock_request(json!({"ok": false, "error": "Remote control is disabled"}));
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Remote control is disabled")
        );
    }

    #[test]
    fn rejects_truncated_response() {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let responder = thread::spawn(move || {
            let mut request = Vec::new();
            server.read_to_end(&mut request).unwrap();
            server.write_all(b"\x1bP@kitty-cmd{\"ok\":true}").unwrap();
        });
        assert!(
            request_focused_cwd(&mut client)
                .unwrap_err()
                .to_string()
                .contains("incomplete")
        );
        responder.join().unwrap();
    }

    #[test]
    fn times_out_when_socket_does_not_reply() {
        let (mut client, _server) = UnixStream::pair().unwrap();
        assert!(
            matches!(request_focused_cwd(&mut client), Err(Error::IpcError(err))
            if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut))
        );
    }
}
