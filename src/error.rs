use hyprland::error::HyprError;
use procfs::ProcError;
use std::env::VarError;
use thiserror::Error;

pub type HyprCwdResult<T> = Result<T, HyprCwdError>;

#[derive(Error, Debug)]
pub enum HyprCwdError {
    #[error("error(hyprland): {0}")]
    HyprlandError(#[from] HyprError),
    #[error("error(procfs): {0}")]
    ProcfsError(#[from] ProcError),
    #[error("error(env): {0}")]
    EnvVarError(#[from] VarError),
    #[error("error(active_window): no active window found, default not specified")]
    NoActiveWindow,
}
