use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlatformError {
    #[cfg(target_os = "linux")]
    #[error(
        "permission denied reading keyboards in /dev/input. Add your user to the input group \
         with `sudo usermod -aG input $USER`, then log out and back in"
    )]
    InputPermission,

    #[cfg(target_os = "linux")]
    #[error("no keyboard found in /dev/input")]
    NoKeyboards,

    #[cfg(target_os = "linux")]
    #[error(
        "permission denied opening /dev/uinput. Install the udev rule from the README so the \
         input group can write to it, then reboot or reload udev"
    )]
    UinputPermission,

    #[cfg(target_os = "linux")]
    #[error("/dev/uinput does not exist. Load the module with `sudo modprobe uinput`")]
    UinputMissing,

    #[cfg(target_os = "linux")]
    #[error("failed to grab {device}; another program may hold an exclusive grab on it")]
    Grab {
        device: String,
        #[source]
        source: std::io::Error,
    },

    #[cfg(target_os = "linux")]
    #[error("failed to {action}")]
    Io {
        action: &'static str,
        #[source]
        source: std::io::Error,
    },
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
