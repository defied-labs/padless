use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use padless_core::Config;

const CONFIG_FILE_NAME: &str = "config.toml";

pub struct ConfigSource {
    path: PathBuf,
    explicit: bool,
}

pub struct Settings {
    pub config: Config,
    pub from_file: bool,
}

impl ConfigSource {
    pub fn new(explicit: Option<PathBuf>) -> Result<Self> {
        match explicit {
            Some(path) => Ok(Self {
                path,
                explicit: true,
            }),
            None => Ok(Self {
                path: default_path()?,
                explicit: false,
            }),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Settings> {
        match fs::read_to_string(&self.path) {
            Ok(text) => {
                let config = Config::from_toml(&text)
                    .with_context(|| format!("invalid config file {}", self.path.display()))?;
                Ok(Settings {
                    config,
                    from_file: true,
                })
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound && !self.explicit => {
                Ok(Settings {
                    config: Config::default(),
                    from_file: false,
                })
            }
            Err(error) => {
                Err(error).with_context(|| format!("cannot read {}", self.path.display()))
            }
        }
    }
}

fn default_path() -> Result<PathBuf> {
    ProjectDirs::from("", "", "padless")
        .map(|dirs| dirs.config_dir().join(CONFIG_FILE_NAME))
        .context("cannot determine the user configuration directory")
}
