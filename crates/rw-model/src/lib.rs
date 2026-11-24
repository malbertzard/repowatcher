//! Types shared by the daemon, the CLI and the Leptos frontend.
//! Keep this crate free of OS-specific dependencies: it is compiled to WASM.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const DEFAULT_INTERVAL_SECS: u64 = 120;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub root_folder: PathBuf,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub sources: Vec<Source>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundMode {
    /// Poll only while a UI is connected; the daemon stops watching when the
    /// window is closed.
    WhileWindowOpen,
    /// Keep polling even with no UI attached (tray/headless mode).
    Always,
}

impl Default for BackgroundMode {
    fn default() -> Self {
        Self::Always
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub background: BackgroundMode,
    #[serde(default)]
    pub tray: bool,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub start_minimized: bool,
    /// Where quick imports are cloned, relative to rootFolder.
    #[serde(default = "default_import_root")]
    pub import_root: PathBuf,
    /// Folder template: {owner}/{repo} or {repo}.
    #[serde(default = "default_folder_template")]
    pub folder_template: String,
    #[serde(default = "default_interval")]
    pub default_interval_secs: u64,
    #[serde(default)]
    pub hosts: std::collections::BTreeMap<String, HostConfig>,
    /// How the Open button launches a source's path.
    #[serde(default)]
    pub open: OpenSettings,
    /// Defaults keyed by "type:git" or "tag:work".
    #[serde(default)]
    pub source_defaults: std::collections::BTreeMap<String, SourceDefault>,
    /// Global configuration owned by plugins, keyed by plugin name.
    #[serde(default)]
    pub plugins: std::collections::BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDefault {
    #[serde(default)]
    pub interval_secs: Option<u64>,
    #[serde(default)]
    pub auto_fetch: Option<bool>,
    #[serde(default)]
    pub token_env: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSettings {
    /// Program to run, e.g. "code", "nautilus", "xdg-open".
    #[serde(default = "default_open_command")]
    pub command: String,
    /// Arguments; `{path}` is replaced by the source path.
    #[serde(default = "default_open_args")]
    pub args: Vec<String>,
}

fn default_open_command() -> String {
    String::from("code")
}

fn default_open_args() -> Vec<String> {
    vec![String::from("{path}")]
}

impl Default for OpenSettings {
    fn default() -> Self {
        Self { command: default_open_command(), args: default_open_args() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostConfig {
    pub provider: ProviderKind,
    #[serde(default)]
    pub token_env: Option<String>,
}

fn default_import_root() -> PathBuf {
    PathBuf::from("imports")
}

fn default_folder_template() -> String {
    String::from("{owner}/{repo}")
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            background: BackgroundMode::Always,
            tray: false,
            autostart: false,
            start_minimized: false,
            import_root: default_import_root(),
            folder_template: default_folder_template(),
            default_interval_secs: DEFAULT_INTERVAL_SECS,
            hosts: std::collections::BTreeMap::new(),
            open: OpenSettings::default(),
            source_defaults: std::collections::BTreeMap::new(),
            plugins: std::collections::BTreeMap::new(),
        }
    }
}

impl Config {
    /// One place for all config rules, used by the daemon before it saves.
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = std::collections::HashSet::new();
        for source in &self.sources {
            if source.id.trim().is_empty() {
                return Err("source id must not be empty".into());
            }
            if !ids.insert(&source.id) {
                return Err(format!("duplicate source id: {}", source.id));
            }
            if source.interval_secs == 0 {
                return Err(format!("source {}: interval must be > 0", source.id));
            }
            for import in &source.imports {
                if import.url.trim().is_empty() {
                    return Err(format!("source {}: import needs a URL", source.id));
                }
            }
            match &source.vcs {
                VcsConfig::Git(git) => git.validate(&source.id)?,
                VcsConfig::Rcs(rcs) => rcs.validate(&source.id)?,
                VcsConfig::Plugin(plugin) => plugin.validate(&source.id)?,
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    /// Remotes to bring into this source, handled by a registered ingest system.
    #[serde(default)]
    pub imports: Vec<ImportSpec>,
    /// Free-form notes and todos for this project.
    #[serde(default)]
    pub notes: String,
    /// Tags used to filter projects on the dashboard.
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(flatten)]
    pub vcs: VcsConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSpec {
    /// Remote location to fetch from. The only field a user must provide.
    pub url: String,
    /// Inferred from the URL when omitted.
    #[serde(default)]
    pub ingest: Option<String>,
    /// Inferred from settings when omitted.
    #[serde(default)]
    pub path: Option<PathBuf>,
    #[serde(default)]
    pub options: serde_json::Value,
}

fn default_interval() -> u64 {
    DEFAULT_INTERVAL_SECS
}

impl Source {
    /// Case-insensitive search over id, label, tags and notes.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }
        self.id.to_lowercase().contains(&query)
            || self.label.as_deref().is_some_and(|label| label.to_lowercase().contains(&query))
            || self.notes.to_lowercase().contains(&query)
            || self.tags.iter().any(|tag| tag.to_lowercase().contains(&query))
    }

    /// Display name of the VCS driving this source.
    pub fn vcs_name(&self) -> &str {
        match &self.vcs {
            VcsConfig::Git(_) => "git",
            VcsConfig::Rcs(_) => "rcs",
            VcsConfig::Plugin(config) => match config.plugin.as_str() {
                "plain-file" => "plain file",
                other => other,
            },
        }
    }

    /// Token env from the most specific type/tag default, if set.
    pub fn default_token_env(&self, settings: &Settings) -> Option<String> {
        let vcs = match &self.vcs {
            VcsConfig::Git(_) => "git",
            VcsConfig::Rcs(_) => "rcs",
            VcsConfig::Plugin(config) => config.plugin.as_str(),
        };
        let mut result = None;
        for key in std::iter::once(format!("type:{vcs}"))
            .chain(self.tags.iter().map(|tag| format!("tag:{}", tag.to_lowercase())))
        {
            if let Some(value) = settings.source_defaults.get(&key).and_then(|item| item.token_env.clone()) {
                result = Some(value);
            }
        }
        result
    }

    /// Only sources backed by a remote can be moved to another device: the
    /// bundle re-clones them. Local-only sources (RCS, plain files) cannot.
    pub fn can_export(&self) -> bool {
        match &self.vcs {
            VcsConfig::Git(git) => !git.remotes.is_empty(),
            VcsConfig::Rcs(_) => false,
            VcsConfig::Plugin(_) => !self.imports.is_empty(),
        }
    }
}

/// One variant per VCS. Flattened into `Source` so YAML looks like:
/// `vcs: git` followed by that VCS's own fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "vcs", rename_all = "snake_case")]
pub enum VcsConfig {
    Git(GitConfig),
    Rcs(RcsConfig),
    /// A driver registered at runtime by name; options are passed through untouched.
    Plugin(PluginConfig),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginConfig {
    pub plugin: String,
    #[serde(default)]
    pub options: serde_json::Value,
    /// Local paths the plugin should inspect, relative to rootFolder.
    #[serde(default)]
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitConfig {
    pub remotes: Vec<GitRemote>,
    pub checkouts: Vec<GitCheckout>,
}

impl GitConfig {
    fn validate(&self, source: &str) -> Result<(), String> {
        let mut names = std::collections::HashSet::new();
        for remote in &self.remotes {
            if remote.name.trim().is_empty() {
                return Err(format!("source {source}: remote name must not be empty"));
            }
            if !names.insert(&remote.name) {
                return Err(format!("source {source}: duplicate remote: {}", remote.name));
            }
        }
        for checkout in &self.checkouts {
            if checkout.name.trim().is_empty() || checkout.path.as_os_str().is_empty() {
                return Err(format!("source {source}: checkout name and path are required"));
            }
            if !names.contains(&checkout.remote) {
                return Err(format!("source {source}: checkout {} uses unknown remote {}", checkout.name, checkout.remote));
            }
        }
        Ok(())
    }

    /// The upstream remote, if the user marked one.
    pub fn upstream(&self) -> Option<&GitRemote> {
        self.remotes.iter().find(|r| r.role == RemoteRole::Upstream)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRemote {
    pub name: String,
    pub role: RemoteRole,
    pub url: String,
    /// Optional forge API for pull requests and pipelines.
    #[serde(default)]
    pub provider: Option<ProviderRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Github,
    Gitlab,
    Codeberg,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRef {
    pub kind: ProviderKind,
    /// Repository id on the forge, e.g. "owner/name".
    pub repo: String,
    /// Env var holding this remote's token; falls back to the forge default.
    #[serde(default)]
    pub token_env: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteRole {
    Upstream,
    Fork,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCheckout {
    pub name: String,
    pub path: PathBuf,
    pub remote: String,
    pub branch: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RcsConfig {
    pub file: PathBuf,
    #[serde(default)]
    pub rcs_dir: Option<PathBuf>,
}

impl RcsConfig {
    fn validate(&self, source: &str) -> Result<(), String> {
        if self.file.as_os_str().is_empty() {
            return Err(format!("source {source}: rcs file is required"));
        }
        Ok(())
    }
}

impl PluginConfig {
    fn validate(&self, source: &str) -> Result<(), String> {
        if self.plugin.trim().is_empty() {
            return Err(format!("source {source}: plugin name is required"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    pub id: String,
    pub units: Vec<UnitStatus>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitStatus {
    pub name: String,
    pub state: UnitState,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UnitState {
    Clean,
    Dirty,
    Behind { count: u64 },
    Ahead { count: u64 },
    Diverged { ahead: u64, behind: u64 },
    Missing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub config: Config,
    pub statuses: Vec<SourceStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineRun {
    pub id: u64,
    pub status: String,
    #[serde(default)]
    pub conclusion: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceAction {
    pub name: String,
    pub label: String,
    pub description: String,
    /// VCS names this action applies to; "*" means every source.
    pub vcs: Vec<String>,
}

/// A way to add a source, supplied by a built-in or a registered plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceKind {
    pub name: String,
    pub label: String,
    pub description: String,
    /// What the user is asked for in step two of the add wizard.
    pub input_label: String,
    /// `import` clones a remote; `direct` registers a local path/URL.
    pub mode: SourceKindMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKindMode {
    Import,
    Direct,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationItem {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    /// Action the plugin offers for this item, e.g. "checkout-pr".
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub action_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationSnapshot {
    pub plugin: String,
    pub title: String,
    #[serde(default)]
    pub items: Vec<IntegrationItem>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub name: String,
    pub owner: Option<String>,
    pub repo: Option<String>,
    pub folder: String,
    pub ingest: String,
    pub vcs: String,
    pub provider: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportBundle {
    pub version: u32,
    pub exported_at: u64,
    pub settings: Settings,
    pub sources: Vec<Source>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginNames {
    pub vcs: Vec<String>,
    pub ingest: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSnapshot {
    pub source: String,
    pub remote: String,
    pub kind: ProviderKind,
    #[serde(default)]
    pub pull_requests: Vec<PullRequest>,
    #[serde(default)]
    pub pipelines: Vec<PipelineRun>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResult {
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub diff: Option<DiffResult>,
    #[serde(default)]
    pub preview: Option<ImportPreview>,
    #[serde(default)]
    pub export: Option<ExportBundle>,
    #[serde(default)]
    pub integrations: Vec<IntegrationSnapshot>,
    #[serde(default)]
    pub message: Option<String>,
}

impl ApiResult {
    pub fn ok() -> Self {
        Self { ok: true, error: None, diff: None, preview: None, export: None, integrations: Vec::new(), message: None }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self { ok: false, error: Some(message.into()), ..Self::ok() }
    }

    pub fn diff(result: DiffResult) -> Self {
        Self { diff: Some(result), ..Self::ok() }
    }

    pub fn preview(value: ImportPreview) -> Self {
        Self { preview: Some(value), ..Self::ok() }
    }

    pub fn message(value: impl Into<String>) -> Self {
        Self { message: Some(value.into()), ..Self::ok() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffResult {
    pub source: String,
    pub unit: String,
    pub patch: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Command {
    SaveConfig { config: Config },
    Sync { source: String },
    Diff { source: String, unit: Option<String> },
    Open { source: String },
    /// Run all imports configured on one source.
    Import { source: String },
    /// Infer everything from a URL without cloning; used for the one-field UI.
    ImportPreview { url: String },
    /// Paste a URL; the daemon infers the rest, clones and registers a source.
    QuickImport { url: String, folder: Option<String>, ingest: Option<String> },
    /// Export the portable, remote-backed part of the configuration.
    ExportBundle,
    /// Re-clone exported sources on this device.
    ImportBundle { bundle: ExportBundle },
    /// Update only a project's notes and tags, without replacing the whole config.
    UpdateProject { id: String, notes: String, tags: Vec<String> },
    /// Run a registered action against one source.
    RunAction { source: String, action: String },
    /// Fetch integration cards for one source from registered plugins.
    Integrations { source: String },
    /// Run an integration action, e.g. check out a pull request.
    IntegrationAction { source: String, plugin: String, action: String, value: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Event {
    ConfigSaved { at: u64 },
    Changed { source: String, message: String, at: u64 },
    Failed { source: String, message: String, at: u64 },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(id: &str) -> Source {
        Source {
            id: id.into(),
            label: None,
            interval_secs: DEFAULT_INTERVAL_SECS,
            imports: vec![],
            notes: String::new(),
            tags: vec![],
            vcs: VcsConfig::Git(GitConfig { remotes: vec![], checkouts: vec![] }),
        }
    }

    #[test]
    fn config_round_trips_through_yaml() {
        let mut config = Config {
            root_folder: PathBuf::from("/tmp/repos"),
            settings: Settings::default(),
            sources: vec![source("a")],
        };
        if let VcsConfig::Git(git) = &mut config.sources[0].vcs {
            git.remotes.push(GitRemote {
                name: "upstream".into(),
                role: RemoteRole::Upstream,
                url: "https://example.com/a.git".into(),
                provider: None,
            });
            git.checkouts.push(GitCheckout {
                name: "main".into(),
                path: PathBuf::from("a"),
                remote: "upstream".into(),
                branch: "main".into(),
            });
        }
        let yaml = yaml_serde::to_string(&config).unwrap();
        let back: Config = yaml_serde::from_str(&yaml).unwrap();
        assert_eq!(config, back);
        assert!(yaml.contains("vcs: git"), "{yaml}");
    }

    #[test]
    fn search_matches_id_label_tags_and_notes() {
        let mut item = source("alpha");
        item.label = Some("Frontend".into());
        item.tags = vec!["work".into(), "urgent".into()];
        item.notes = "refactor the router".into();
        assert!(item.matches(""));
        assert!(item.matches("ALPHA"));
        assert!(item.matches("front"));
        assert!(item.matches("URGENT"));
        assert!(item.matches("router"));
        assert!(!item.matches("backend"));
    }

    #[test]
    fn rejects_duplicate_ids_and_unknown_remotes() {
        let mut config = Config {
            root_folder: PathBuf::from("/tmp"),
            settings: Settings::default(),
            sources: vec![source("a"), source("a")],
        };
        assert!(config.validate().is_err());

        config.sources.pop();
        if let VcsConfig::Git(git) = &mut config.sources[0].vcs {
            git.checkouts.push(GitCheckout {
                name: "c".into(),
                path: PathBuf::from("c"),
                remote: "missing".into(),
                branch: "main".into(),
            });
        }
        assert!(config.validate().is_err());
    }
}
