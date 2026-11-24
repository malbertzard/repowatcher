//! Minimal runtime plugin registry. Drivers and API providers are looked up by
//! name at call time, so a binary can gain a VCS or forge without recompiling
//! the daemon. Plugins are registered in-process; loading code from disk is a
//! later step.

use rw_model::{
    DiffResult, GitCheckout, GitConfig, GitRemote, IntegrationItem, IntegrationSnapshot,
    PluginNames, ProviderKind, RemoteRole, Settings, Source, SourceAction, SourceKind,
    SourceKindMode, UnitState, UnitStatus, VcsConfig,
};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, LazyLock, RwLock};
use std::time::Instant;

pub trait VcsPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn status(&self, options: &serde_json::Value, paths: &[PathBuf]) -> Result<Vec<UnitStatus>, String>;
    fn sync(&self, options: &serde_json::Value, paths: &[PathBuf]) -> Result<(), String>;
    fn diff(
        &self,
        options: &serde_json::Value,
        paths: &[PathBuf],
        unit: Option<&str>,
    ) -> Result<DiffResult, String>;
}

pub struct IngestResult {
    pub vcs: VcsConfig,
    pub label: Option<String>,
}

pub trait IngestPlugin: Send + Sync {
    fn name(&self) -> &str;
    /// Fetch `url` into `root/relative_path` and return the source config to register.
    fn ingest(
        &self,
        url: &str,
        root: &Path,
        relative_path: &Path,
        options: &serde_json::Value,
    ) -> Result<IngestResult, String>;
}

pub trait IntegrationPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn title(&self) -> &str;
    fn fetch(&self, source: &Source, settings: &Settings) -> Result<IntegrationSnapshot, String>;
    fn run_action(&self, source: &Source, root: &Path, action: &str, value: &str) -> Result<String, String>;
}

pub trait SourceActionPlugin: Send + Sync {
    fn metadata(&self) -> SourceAction;
    fn run(&self, source: &Source, root: &Path) -> Result<String, String>;
}

#[derive(Default)]
pub struct Registry {
    drivers: RwLock<HashMap<String, Arc<dyn VcsPlugin>>>,
    ingest: RwLock<HashMap<String, Arc<dyn IngestPlugin>>>,
    actions: RwLock<HashMap<String, Arc<dyn SourceActionPlugin>>>,
    integrations: RwLock<HashMap<String, Arc<dyn IntegrationPlugin>>>,
}

impl Registry {
    pub fn register_vcs(&self, plugin: Arc<dyn VcsPlugin>) -> Result<(), String> {
        let mut drivers = self.drivers.write().unwrap();
        let name = plugin.name().to_owned();
        if drivers.contains_key(&name) {
            return Err(format!("VCS plugin already registered: {name}"));
        }
        drivers.insert(name, plugin);
        Ok(())
    }

    pub fn vcs(&self, name: &str) -> Option<Arc<dyn VcsPlugin>> {
        self.drivers.read().unwrap().get(name).cloned()
    }

    pub fn register_ingest(&self, plugin: Arc<dyn IngestPlugin>) -> Result<(), String> {
        let mut ingest = self.ingest.write().unwrap();
        let name = plugin.name().to_owned();
        if ingest.contains_key(&name) {
            return Err(format!("ingest plugin already registered: {name}"));
        }
        ingest.insert(name, plugin);
        Ok(())
    }

    pub fn ingest(&self, name: &str) -> Option<Arc<dyn IngestPlugin>> {
        self.ingest.read().unwrap().get(name).cloned()
    }

    pub fn register_integration(&self, plugin: Arc<dyn IntegrationPlugin>) -> Result<(), String> {
        let mut integrations = self.integrations.write().unwrap();
        let name = plugin.name().to_owned();
        if integrations.contains_key(&name) {
            return Err(format!("integration already registered: {name}"));
        }
        integrations.insert(name, plugin);
        Ok(())
    }

    pub fn integration(&self, name: &str) -> Option<Arc<dyn IntegrationPlugin>> {
        self.integrations.read().unwrap().get(name).cloned()
    }

    pub fn integrations(&self) -> Vec<String> {
        let mut names: Vec<String> = self.integrations.read().unwrap().keys().cloned().collect();
        names.sort();
        names
    }

    pub fn register_action(&self, plugin: Arc<dyn SourceActionPlugin>) -> Result<(), String> {
        let mut actions = self.actions.write().unwrap();
        let name = plugin.metadata().name;
        if actions.contains_key(&name) {
            return Err(format!("action already registered: {name}"));
        }
        actions.insert(name, plugin);
        Ok(())
    }

    pub fn action(&self, name: &str) -> Option<Arc<dyn SourceActionPlugin>> {
        self.actions.read().unwrap().get(name).cloned()
    }

    pub fn actions(&self) -> Vec<SourceAction> {
        let mut actions: Vec<SourceAction> = self
            .actions
            .read()
            .unwrap()
            .values()
            .map(|plugin| plugin.metadata())
            .collect();
        actions.sort_by(|a, b| a.name.cmp(&b.name));
        actions
    }

    pub fn names(&self) -> PluginNames {
        PluginNames {
            vcs: self.drivers.read().unwrap().keys().cloned().collect(),
            ingest: self.ingest.read().unwrap().keys().cloned().collect(),
        }
    }
}

static REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::default);

pub fn registry() -> &'static Registry {
    &REGISTRY
}

/// The built-in plugin proves the registry works without an external library.
/// It watches one or more plain files and reports whether they exist.
struct PlainFilePlugin;

impl VcsPlugin for PlainFilePlugin {
    fn name(&self) -> &str {
        "plain-file"
    }

    fn status(&self, _options: &serde_json::Value, paths: &[PathBuf]) -> Result<Vec<UnitStatus>, String> {
        Ok(paths.iter().map(|path| UnitStatus {
            name: format!("file:{}", path.display()),
            state: if path.exists() {
                rw_model::UnitState::Clean
            } else {
                rw_model::UnitState::Missing
            },
            detail: None,
        }).collect())
    }

    fn sync(&self, _options: &serde_json::Value, _paths: &[PathBuf]) -> Result<(), String> {
        Ok(())
    }

    fn diff(
        &self,
        _options: &serde_json::Value,
        _paths: &[PathBuf],
        _unit: Option<&str>,
    ) -> Result<DiffResult, String> {
        Err("plain-file does not support diff".into())
    }
}

/// Clones a git URL into root/relative_path and returns a git source config.
struct GitIngest;

impl IngestPlugin for GitIngest {
    fn name(&self) -> &str {
        "git-ingest"
    }

    fn ingest(
        &self,
        url: &str,
        root: &Path,
        relative_path: &Path,
        options: &serde_json::Value,
    ) -> Result<IngestResult, String> {
        let destination = root.join(relative_path);
        if destination.exists() {
            // Re-importing a successful clone is a no-op: accept an existing
            // git repo, refuse a non-repo directory so we never hide a conflict.
            if git2::Repository::open(&destination).is_err() {
                return Err(format!("destination is not a git repo: {}", destination.display()));
            }
        } else {
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut callbacks = git2::RemoteCallbacks::new();
            callbacks.credentials(crate::drivers::credentials);
            let mut fetch_options = git2::FetchOptions::new();
            fetch_options.remote_callbacks(callbacks);
            let mut builder = git2::build::RepoBuilder::new();
            builder.fetch_options(fetch_options);
            builder
                .clone(url, &destination)
                .map_err(|e| format!("clone {url}: {e}"))?;
        }

        let role = if options.get("fork").and_then(|v| v.as_bool()).unwrap_or(false) {
            RemoteRole::Fork
        } else {
            RemoteRole::Upstream
        };
        let branch = options
            .get("branch")
            .and_then(|v| v.as_str())
            .unwrap_or("main")
            .to_owned();
        Ok(IngestResult {
            label: options.get("label").and_then(|v| v.as_str()).map(str::to_owned),
            vcs: VcsConfig::Git(GitConfig {
                remotes: vec![GitRemote { name: "origin".into(), role, url: url.into(), provider: None }],
                checkouts: vec![GitCheckout {
                    name: "main".into(),
                    path: relative_path.to_path_buf(),
                    remote: "origin".into(),
                    branch,
                }],
            }),
        })
    }
}

/// A hung CLI tool is killed before the watcher's 300s guard timeout, so the
/// source is not stuck forever and the child process does not leak.
const CLI_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(240);

fn run_cli(
    binary: &str,
    args: &[&str],
    path: &Path,
    timeout: std::time::Duration,
) -> Result<std::process::Output, String> {
    let mut child = std::process::Command::new(binary)
        .args(args)
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("{binary}: {error}"))?;
    let mut stdout = child.stdout.take().ok_or("no stdout pipe")?;
    let mut stderr = child.stderr.take().ok_or("no stderr pipe")?;
    let out_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let err_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        bytes
    });
    let start = Instant::now();
    loop {
        if child.try_wait().map_err(|error| format!("{binary}: {error}"))?.is_some() {
            let status = child.wait().map_err(|error| format!("{binary}: {error}"))?;
            return Ok(std::process::Output {
                status,
                stdout: out_reader.join().unwrap_or_default(),
                stderr: err_reader.join().unwrap_or_default(),
            });
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{binary} timed out after {}s", timeout.as_secs()));
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// A VCS whose CLI tool is guaranteed on PATH by the Nix wrapper. These are
/// registered like any other driver; the core knows nothing about them.
struct CliVcs {
    id: &'static str,
    binary: &'static str,
    status_args: &'static [&'static str],
    sync_args: &'static [&'static str],
    diff_args: &'static [&'static str],
}

impl CliVcs {
    fn run(&self, args: &[&str], path: &Path) -> Result<std::process::Output, String> {
        run_cli(self.binary, args, path, CLI_TIMEOUT)
    }
}

impl VcsPlugin for CliVcs {
    fn name(&self) -> &str {
        self.id
    }

    fn status(&self, _options: &serde_json::Value, paths: &[PathBuf]) -> Result<Vec<UnitStatus>, String> {
        let mut units = Vec::new();
        for path in paths {
            let name = format!("{}:{}", self.id, path.display());
            if !path.exists() {
                units.push(UnitStatus { name, state: UnitState::Missing, detail: Some("path does not exist".into()) });
                continue;
            }
            let output = self.run(self.status_args, path)?;
            if !output.status.success() {
                units.push(UnitStatus {
                    name,
                    state: UnitState::Dirty,
                    detail: Some(String::from_utf8_lossy(&output.stderr).trim().to_owned()),
                });
                continue;
            }
            let changed = !String::from_utf8_lossy(&output.stdout).trim().is_empty();
            units.push(UnitStatus {
                name,
                state: if changed { UnitState::Dirty } else { UnitState::Clean },
                detail: None,
            });
        }
        Ok(units)
    }

    fn sync(&self, _options: &serde_json::Value, paths: &[PathBuf]) -> Result<(), String> {
        let mut errors = Vec::new();
        for path in paths {
            match self.run(self.sync_args, path) {
                Ok(output) if !output.status.success() => errors.push(format!(
                    "{}: {}",
                    path.display(),
                    String::from_utf8_lossy(&output.stderr).trim()
                )),
                Err(error) => errors.push(format!("{}: {error}", path.display())),
                Ok(_) => {}
            }
        }
        if errors.is_empty() { Ok(()) } else { Err(errors.join("; ")) }
    }

    fn diff(&self, _options: &serde_json::Value, paths: &[PathBuf], _unit: Option<&str>) -> Result<DiffResult, String> {
        let path = paths.first().ok_or("no path to diff")?;
        let output = self.run(self.diff_args, path)?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }
        Ok(DiffResult {
            source: String::new(),
            unit: path.display().to_string(),
            patch: String::from_utf8_lossy(&output.stdout).into_owned(),
        })
    }
}

/// Prunes stale remote-tracking branches with libgit2 (no `git` CLI call).
struct GitPrune;

impl SourceActionPlugin for GitPrune {
    fn metadata(&self) -> SourceAction {
        SourceAction {
            name: "git-prune".into(),
            label: "Prune remote branches".into(),
            description: "Remove remote-tracking refs that no longer exist upstream.".into(),
            vcs: vec!["git".into()],
        }
    }

    fn run(&self, source: &Source, root: &Path) -> Result<String, String> {
        let VcsConfig::Git(git) = &source.vcs else {
            return Err("git-prune only works on git sources".into());
        };
        let mut checkouts = 0;
        for checkout in &git.checkouts {
            let repo = git2::Repository::open(root.join(&checkout.path))
                .map_err(|e| format!("{}: {e}", checkout.path.display()))?;
            for remote_config in &git.remotes {
                let mut remote = repo
                    .find_remote(&remote_config.name)
                    .map_err(|e| format!("{}: {e}", remote_config.name))?;
                let mut callbacks = git2::RemoteCallbacks::new();
                callbacks.credentials(crate::drivers::credentials);
                let mut options = git2::FetchOptions::new();
                options.remote_callbacks(callbacks);
                options.prune(git2::FetchPrune::On);
                remote
                    .fetch(&[] as &[&str], Some(&mut options), None)
                    .map_err(|e| format!("fetch {}: {e}", remote_config.name))?;
                remote.prune(None).map_err(|e| format!("prune {}: {e}", remote_config.name))?;
            }
            checkouts += 1;
        }
        Ok(format!("Pruned remote refs in {checkouts} checkout(s)"))
    }
}

/// The add wizard's choices: built-ins plus whatever the registry provides.
pub fn source_kinds() -> Vec<SourceKind> {
    let mut kinds = vec![
        SourceKind {
            name: "git-ingest".into(),
            label: "Git repository".into(),
            description: "Clone a repository from a URL and watch it.".into(),
            input_label: "Git URL".into(),
            mode: SourceKindMode::Import,
        },
        SourceKind {
            name: "rcs".into(),
            label: "RCS file".into(),
            description: "Watch a single RCS-managed file and its ,v archive.".into(),
            input_label: "File path".into(),
            mode: SourceKindMode::Direct,
        },
    ];
    let names = registry().names();
    for name in names.ingest.into_iter().filter(|name| name != "git-ingest") {
        kinds.push(SourceKind {
            label: name.clone(),
            description: format!("Ingest a remote with the {name} system."),
            name,
            input_label: "Remote URL".into(),
            mode: SourceKindMode::Import,
        });
    }
    for name in names.vcs {
        kinds.push(SourceKind {
            label: name.clone(),
            description: format!("Watch paths with the {name} driver."),
            name,
            input_label: "Paths (comma separated)".into(),
            mode: SourceKindMode::Direct,
        });
    }
    kinds
}

/// Pull requests, issues and pipelines from configured forge remotes. VCS data
/// comes from the source config, so the same integration works for any driver
/// that exposes a remote with a provider.
struct ForgeIntegration(ProviderKind);

impl ForgeIntegration {
    fn kind_name(kind: ProviderKind) -> &'static str {
        match kind {
            ProviderKind::Github => "github",
            ProviderKind::Gitlab => "gitlab",
            ProviderKind::Codeberg => "codeberg",
        }
    }

    fn kind_title(kind: ProviderKind) -> &'static str {
        match kind {
            ProviderKind::Github => "GitHub",
            ProviderKind::Gitlab => "GitLab",
            ProviderKind::Codeberg => "Codeberg",
        }
    }
}

impl IntegrationPlugin for ForgeIntegration {
    fn name(&self) -> &str {
        Self::kind_name(self.0)
    }

    fn title(&self) -> &str {
        Self::kind_title(self.0)
    }

    fn fetch(&self, source: &Source, settings: &Settings) -> Result<IntegrationSnapshot, String> {
        let VcsConfig::Git(git) = &source.vcs else {
            return Ok(IntegrationSnapshot {
                plugin: self.name().into(),
                title: self.title().into(),
                items: Vec::new(),
                error: Some("no forge remotes on this source".into()),
            });
        };
        let mut items = Vec::new();
        let mut errors = Vec::new();
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;
        for remote in git
            .remotes
            .iter()
            .filter_map(|remote| remote.provider.as_ref().map(|provider| (remote, provider)))
            .filter(|(_, provider)| provider.kind == self.0)
        {
            let (remote, provider) = remote;
            let token_name = provider
                .token_env
                .clone()
                .or_else(|| source.default_token_env(settings))
                .unwrap_or_else(|| crate::providers::token_var(self.0));
            let Ok(token) = std::env::var(&token_name) else {
                errors.push(format!("{token_name} is not set"));
                continue;
            };
            let pulls = crate::providers::pulls_url(self.0, &provider.repo);
            let request = client
                .get(&pulls)
                .header("User-Agent", "repowatcher")
                .bearer_auth(&token)
                .send();
            match request
                .and_then(|response| response.error_for_status())
                .and_then(|response| response.json::<serde_json::Value>())
            {
                Ok(value) => {
                    for pull in crate::providers::map_pulls(self.0, &value) {
                        items.push(IntegrationItem {
                            id: format!("pr-{}", pull.number),
                            title: format!("#{} {}", pull.number, pull.title),
                            subtitle: Some(remote.name.clone()),
                            state: Some(pull.state),
                            url: Some(pull.url),
                            action: Some("checkout-pr".into()),
                            action_label: Some("Get code".into()),
                        });
                    }
                }
                Err(error) => errors.push(error.to_string()),
            }
        }
        Ok(IntegrationSnapshot {
            plugin: self.name().into(),
            title: self.title().into(),
            items,
            error: if errors.is_empty() { None } else { Some(errors.join("; ")) },
        })
    }

    fn run_action(&self, source: &Source, root: &Path, action: &str, value: &str) -> Result<String, String> {
        if action != "checkout-pr" {
            return Err(format!("unknown integration action: {action}"));
        }
        let VcsConfig::Git(git) = &source.vcs else {
            return Err("checkout-pr needs a git source".into());
        };
        let number: u64 = value
            .trim()
            .strip_prefix("pr-")
            .unwrap_or(value.trim())
            .parse()
            .map_err(|_| "pull request number must be numeric")?;
        let checkout = git.checkouts.first().ok_or("source has no checkout")?;
        let repo = git2::Repository::open(root.join(&checkout.path)).map_err(|e| e.to_string())?;
        let mut remote = repo.find_remote(&checkout.remote).map_err(|e| e.to_string())?;
        let target_ref = match self.0 {
            rw_model::ProviderKind::Gitlab => format!("refs/merge-requests/{number}/head"),
            _ => format!("refs/pull/{number}/head"),
        };
        let refspec = format!("+{target_ref}:{target_ref}");
        let mut callbacks = git2::RemoteCallbacks::new();
        callbacks.credentials(crate::drivers::credentials);
        let mut options = git2::FetchOptions::new();
        options.remote_callbacks(callbacks);
        remote
            .fetch(&[refspec.as_str()], Some(&mut options), None)
            .map_err(|e| format!("fetch PR {number}: {e}"))?;
        let reference = repo.find_reference(&target_ref).map_err(|e| e.to_string())?;
        let object = reference.peel_to_commit().map_err(|e| e.to_string())?;
        repo.set_head_detached(object.id()).map_err(|e| e.to_string())?;
        repo.checkout_head(None).map_err(|e| e.to_string())?;
        Ok(format!("Checked out pull request #{number}"))
    }
}

/// Register built-ins once. Duplicate registration is ignored so tests can call
/// this freely.
pub fn register_builtins() {
    let _ = registry().register_vcs(Arc::new(PlainFilePlugin));
    let _ = registry().register_vcs(Arc::new(CliVcs {
        id: "mercurial",
        binary: "hg",
        status_args: &["status"],
        sync_args: &["pull", "-u"],
        diff_args: &["diff"],
    }));
    let _ = registry().register_vcs(Arc::new(CliVcs {
        id: "subversion",
        binary: "svn",
        status_args: &["status"],
        sync_args: &["update"],
        diff_args: &["diff"],
    }));
    let _ = registry().register_vcs(Arc::new(CliVcs {
        id: "jujutsu",
        binary: "jj",
        status_args: &["diff", "--summary"],
        sync_args: &["git", "fetch"],
        diff_args: &["diff"],
    }));
    let _ = registry().register_ingest(Arc::new(GitIngest));
    let _ = registry().register_action(Arc::new(GitPrune));
    for kind in [
        ProviderKind::Github,
        ProviderKind::Gitlab,
        ProviderKind::Codeberg,
    ] {
        let _ = registry().register_integration(Arc::new(ForgeIntegration(kind)));
    }
}

pub fn resolve_paths(root: &Path, paths: &[PathBuf]) -> Vec<PathBuf> {
    paths.iter().map(|path| root.join(path)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rw_model::UnitState;

    #[test]
    fn plugins_are_lookupable_by_name() {
        register_builtins();
        assert!(registry().vcs("plain-file").is_some());
        for name in ["mercurial", "subversion", "jujutsu"] {
            assert!(registry().vcs(name).is_some(), "{name} not registered");
        }
        assert!(registry().vcs("nope").is_none());
    }

    #[test]
    fn cli_plugin_drains_large_output() {
        let dir = tempfile::tempdir().unwrap();
        let output = run_cli(
            "sh",
            &["-c", "head -c 200000 /dev/zero | tr '\\0' 'x'"],
            dir.path(),
            std::time::Duration::from_secs(10),
        )
        .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 200000);
    }

    #[test]
    fn cli_plugin_kills_a_hung_command() {
        let dir = tempfile::tempdir().unwrap();
        let start = Instant::now();
        let error = run_cli(
            "sh",
            &["-c", "sleep 5"],
            dir.path(),
            std::time::Duration::from_millis(200),
        )
        .unwrap_err();
        assert!(error.contains("timed out"), "{error}");
        assert!(start.elapsed() < std::time::Duration::from_secs(3));
    }

    #[test]
    fn git_ingest_clones_and_builds_a_source_config() {
        register_builtins();
        let dir = tempfile::tempdir().unwrap();
        let bare = dir.path().join("origin.git");
        let seed = dir.path().join("seed");
        let run = |cwd: &Path, args: &[&str]| {
            let out = std::process::Command::new("git").args(args).current_dir(cwd).output().unwrap();
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        };
        run(dir.path(), &["init", "--bare", "-q", bare.to_str().unwrap()]);
        run(dir.path(), &["clone", "-q", bare.to_str().unwrap(), seed.to_str().unwrap()]);
        run(&seed, &["config", "user.email", "t@e.com"]);
        run(&seed, &["config", "user.name", "T"]);
        std::fs::write(seed.join("f.txt"), "x").unwrap();
        run(&seed, &["add", "f.txt"]);
        run(&seed, &["commit", "-qm", "first"]);
        run(&seed, &["push", "-q", "origin", "HEAD"]);

        let result = registry()
            .ingest("git-ingest")
            .unwrap()
            .ingest(
                bare.to_str().unwrap(),
                dir.path(),
                Path::new("cloned"),
                &serde_json::json!({ "branch": "master" }),
            )
            .unwrap();
        assert!(dir.path().join("cloned/f.txt").exists());
        // Re-import of an existing clone is a no-op, not a conflict.
        registry()
            .ingest("git-ingest")
            .unwrap()
            .ingest(
                bare.to_str().unwrap(),
                dir.path(),
                Path::new("cloned"),
                &serde_json::Value::Null,
            )
            .unwrap();
        match result.vcs {
            VcsConfig::Git(git) => {
                assert_eq!(git.remotes[0].name, "origin");
                assert_eq!(git.checkouts[0].path, PathBuf::from("cloned"));
                assert_eq!(git.checkouts[0].branch, "master");
            }
            _ => panic!("git ingest must produce a git source"),
        }
    }

    #[test]
    fn source_actions_are_registered() {
        register_builtins();
        let actions = registry().actions();
        assert!(actions.iter().any(|action| action.name == "git-prune"));
        assert!(registry().action("git-prune").is_some());
        assert!(registry().action("nope").is_none());
    }

    #[test]
    fn plain_file_reports_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("a.txt");
        std::fs::write(&existing, "x").unwrap();
        let plugin = PlainFilePlugin;
        let units = plugin
            .status(&serde_json::Value::Null, &[existing.clone(), dir.path().join("missing.txt")])
            .unwrap();
        assert_eq!(units[0].state, UnitState::Clean);
        assert_eq!(units[1].state, UnitState::Missing);
    }
}
