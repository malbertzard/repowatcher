use fs2::FileExt;
use rw_model::{Config, Settings};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Owns the three files the daemon touches: config, API token, and lock.
pub struct Store {
    config_path: PathBuf,
    token_path: PathBuf,
    lock_path: PathBuf,
}

impl Store {
    pub fn new(config_path: PathBuf) -> Self {
        let dir = config_path.parent().unwrap_or(Path::new("."));
        Self {
            lock_path: dir.join("daemon.lock"),
            token_path: dir.join("token"),
            config_path,
        }
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn load(&self) -> Result<Config, String> {
        if !self.config_path.exists() {
            let config = Config {
                root_folder: home().join("repos"),
                settings: Settings::default(),
                sources: Vec::new(),
            };
            self.save(&config)?;
            return Ok(config);
        }
        let text = fs::read_to_string(&self.config_path)
            .map_err(|e| format!("read {}: {e}", self.config_path.display()))?;
        yaml_serde::from_str(&text).map_err(|e| format!("parse {}: {e}", self.config_path.display()))
    }

    /// Only the daemon writes config: validate, then temp file + rename.
    pub fn save(&self, config: &Config) -> Result<(), String> {
        config.validate()?;
        let dir = self.config_path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        let temp = self.config_path.with_extension("yaml.tmp");
        let text = yaml_serde::to_string(config).map_err(|e| format!("serialize config: {e}"))?;
        fs::write(&temp, text).map_err(|e| format!("write {}: {e}", temp.display()))?;
        fs::rename(&temp, &self.config_path)
            .map_err(|e| format!("rename {} -> {}: {e}", temp.display(), self.config_path.display()))?;
        Ok(())
    }

    /// Stable token so the CLI and the desktop shell can connect after a restart.
    pub fn token(&self) -> Result<String, String> {
        if let Ok(token) = fs::read_to_string(&self.token_path) {
            let token = token.trim().to_owned();
            if !token.is_empty() {
                tighten_token_permissions(&self.token_path);
                return Ok(token);
            }
        }
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| format!("random token: {e}"))?;
        let token = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();

        // An existing but empty token (crash during first write) is not a valid
        // token; overwrite it. The daemon lock guarantees a single writer.
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&self.token_path)
            .map_err(|e| format!("create {}: {e}", self.token_path.display()))?;
        file.write_all(token.as_bytes())
            .map_err(|e| format!("write {}: {e}", self.token_path.display()))?;
        tighten_token_permissions(&self.token_path);
        Ok(token)
    }

    /// The OS releases this lock when the daemon exits, so no stale-lock cleanup.
    pub fn lock(&self) -> Result<File, String> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.lock_path)
            .map_err(|e| format!("open {}: {e}", self.lock_path.display()))?;
        file.try_lock_exclusive()
            .map_err(|_| String::from("another rw daemon is already running"))?;
        Ok(file)
    }
}

fn tighten_token_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    let _ = path;
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rw_model::Source;

    #[test]
    fn creates_default_config_and_keeps_token_stable() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("config.yaml"));
        let config = store.load().unwrap();
        assert!(config.sources.is_empty());
        assert!(store.config_path().exists());

        let token = store.token().unwrap();
        assert_eq!(token.len(), 64);
        assert_eq!(store.token().unwrap(), token);
        assert!(!config.root_folder.as_os_str().is_empty());

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let mode = fs::metadata(dir.path().join("token")).unwrap().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn save_is_atomic_and_rejects_invalid_config() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("config.yaml"));
        let mut config = store.load().unwrap();
        config.sources.push(Source {
            id: "a".into(),
            label: None,
            interval_secs: 10,
            imports: vec![],
            notes: String::new(),
            tags: vec![],
            vcs: rw_model::VcsConfig::Rcs(rw_model::RcsConfig {
                file: PathBuf::from("f"),
                rcs_dir: None,
            }),
        });
        store.save(&config).unwrap();
        assert_eq!(store.load().unwrap(), config);
        assert!(!dir.path().join("config.yaml.tmp").exists());

        config.sources.push(config.sources[0].clone());
        assert!(store.save(&config).is_err());
        assert_eq!(store.load().unwrap().sources.len(), 1);
    }

    #[test]
    fn empty_token_file_is_repaired() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("config.yaml"));
        let path = dir.path().join("token");
        fs::write(&path, "").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        }
        let token = store.token().unwrap();
        assert_eq!(token.len(), 64);
        assert_eq!(store.token().unwrap(), token);
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let mode = fs::metadata(&path).unwrap().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn only_one_daemon_can_hold_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("config.yaml"));
        let _first = store.lock().unwrap();
        assert!(store.lock().is_err());
    }
}
