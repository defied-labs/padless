use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlatformError {
    #[cfg(target_os = "macos")]
    #[error(
        "padless needs Accessibility access to read and inject keystrokes. Open System Settings > \
         Privacy & Security > Accessibility, enable the program that starts padless (your \
         terminal, or the padless binary itself), then start padless again"
    )]
    AccessibilityPermission,

    #[cfg(target_os = "macos")]
    #[error(
        "macOS refused to create the keyboard event tap; check that Accessibility access is \
         granted to the program that starts padless"
    )]
    EventTap,

    #[cfg(target_os = "windows")]
    #[error("failed to install the low-level keyboard hook")]
    Hook(#[source] ::windows::core::Error),

    #[cfg(target_os = "windows")]
    #[error("the message loop failed")]
    MessageLoop(#[source] ::windows::core::Error),
}
