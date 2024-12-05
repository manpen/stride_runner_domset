use std::{
    path::{Path, PathBuf},
    sync::{Mutex, Once},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{directory::StrideDirectory, server_connection::DEFAULT_SERVER_URL};

static mut GLOBAL_SETTINGS: Option<Mutex<Settings>> = None;
static GLOBAL_SETTINGS_INIT: Once = Once::new();

pub fn global_settings<'a>() -> &'a Mutex<Settings> {
    // default construct on first call
    GLOBAL_SETTINGS_INIT.call_once(|| unsafe {
        GLOBAL_SETTINGS = Some(Mutex::new(Default::default()));
    });

    #[allow(static_mut_refs)]
    unsafe {
        GLOBAL_SETTINGS.as_ref().unwrap()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    pub server_url: String,
    pub solver_bin: String,
    pub run_log_dir: String,
    pub solver_uuid: Option<Uuid>,
    pub timeout: u64,
    pub grace: u64,
    pub parallel_jobs: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            server_url: DEFAULT_SERVER_URL.into(),
            run_log_dir: "stride-logs".into(),

            solver_uuid: None,
            solver_bin: String::new(),

            timeout: 300,
            grace: 5,
            parallel_jobs: num_cpus::get(),
        }
    }
}

impl Settings {
    pub fn load_from_default_path() -> anyhow::Result<Settings> {
        let path = Self::default_path()?;
        Self::load_from_path(path.as_path())
    }

    pub fn load_from_path(path: &Path) -> anyhow::Result<Self> {
        let file = std::fs::File::open(path)?;
        Ok(serde_json::from_reader(file)?)
    }

    pub fn store_to_path(&self, path: &Path) -> anyhow::Result<()> {
        let file = std::fs::File::create(path)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }

    pub fn store_to_global_variable(&self) -> anyhow::Result<()> {
        match global_settings().lock() {
            Ok(mut guard) => *guard = self.clone(),
            Err(_) => anyhow::bail!("Cannot lock global settings variable"),
        }
        Ok(())
    }

    fn default_path() -> anyhow::Result<PathBuf> {
        Ok(StrideDirectory::try_default()?.config_file())
    }
}

#[cfg(test)]
mod test {
    use std::ops::Deref;

    use tempdir::TempDir;

    use super::*;

    #[test]
    fn load_store_test() {
        let tmp_dir = TempDir::new("settings").unwrap();
        let path = tmp_dir.path().join("settings.json");

        // produce non-default value
        let mut settings = Settings::default();
        settings.grace += 123;

        // store and read it back
        settings.store_to_path(path.as_path()).unwrap();
        let read_back = Settings::load_from_path(path.as_path()).unwrap();

        assert_eq!(settings, read_back);
    }

    #[test]
    fn global_var() {
        let init = {
            let guard = global_settings().lock().unwrap();
            guard.deref().clone()
        };

        assert_eq!(Settings::default(), init);

        // produce non-default value
        let mut settings = Settings::default();
        settings.grace += 123;

        // store and read it back
        settings.store_to_global_variable().unwrap();
        let read_back = {
            let guard = global_settings().lock().unwrap();
            guard.deref().clone()
        };

        assert_eq!(settings, read_back);
    }
}
