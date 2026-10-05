use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlatformError {
    #[cfg(target_os = "windows")]
    #[error("failed to install the low-level keyboard hook")]
    Hook(#[source] ::windows::core::Error),

    #[cfg(target_os = "windows")]
    #[error("the message loop failed")]
    MessageLoop(#[source] ::windows::core::Error),
}
