use std::sync::Arc;

use padless_core::Engine;

mod error;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as native;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as native;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as native;

pub use error::PlatformError;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
compile_error!("padless supports Linux, macOS and Windows");

pub use native::{open, supports_trigger};

pub trait KeyboardBackend {
    fn name(&self) -> &'static str;

    fn shutdown_handle(&self) -> ShutdownHandle;

    fn run(&mut self, engine: Engine) -> Result<(), PlatformError>;
}

#[derive(Clone)]
pub struct ShutdownHandle(Arc<dyn Fn() + Send + Sync>);

impl ShutdownHandle {
    pub fn new(request: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Arc::new(request))
    }

    pub fn request(&self) {
        (self.0)();
    }
}
