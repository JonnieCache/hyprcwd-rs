use procfs::ProcError;
use std::env::VarError;
use std::io;
use std::string::FromUtf8Error;
use thiserror::Error;

pub type HyprCwdResult<T> = Result<T, HyprCwdError>;

#[derive(Error, Debug)]
pub enum HyprCwdError {
    #[error("error(hyprland): HYPRLAND_INSTANCE_SIGNATURE is not set, is Hyprland running?")]
    NoHyprlandInstance,
    #[error("error(hyprland): could not find XDG_RUNTIME_DIR or UID")]
    NoRuntimeDir,
    #[error("error(hyprland): active window response did not include a valid pid")]
    InvalidActiveWindowPid,
    #[error("error(ipc): {0}")]
    IpcError(#[from] io::Error),
    #[error("error(ipc): {0}")]
    IpcUtf8Error(#[from] FromUtf8Error),
    #[error("error(json): {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("error(procfs): {0}")]
    ProcfsError(#[from] ProcError),
    #[error("error(env): {name}: {source}")]
    EnvVarError {
        name: &'static str,
        #[source]
        source: VarError,
    },
    #[error("error(active_window): no active window found, default not specified")]
    NoActiveWindow,
}
