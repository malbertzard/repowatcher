//! VCS operations. Everything runs in-process: no `git` binary is invoked.

use crate::plugins;
use git2::{Cred, CredentialType, DiffFormat, DiffOptions, RemoteCallbacks, Repository, Status, StatusOptions};
use rw_model::{DiffResult, GitConfig, RcsConfig, RemoteRole, Source, UnitState, UnitStatus};
use std::io::Write as _;
use std::path::{Path, PathBuf};

pub fn checkout_paths(source: &Source, root: &Path) -> Vec<(String, PathBuf)> {
    match &source.vcs {
        rw_model::VcsConfig::Git(git) => git
            .checkouts
            .iter()
            .map(|c| (format!("checkout:{}", c.name), root.join(&c.path)))
            .collect(),
        rw_model::VcsConfig::Rcs(rcs) => {
            vec![("file".into(), rcs.file.clone())]
        }
        rw_model::VcsConfig::Plugin(config) => plugins::resolve_paths(root, &config.paths)
            .into_iter()
            .map(|path| (format!("plugin:{}", config.plugin), path))
            .collect(),
    }
}

pub fn status(source: &Source, root: &Path) -> Result<Vec<UnitStatus>, String> {
    match &source.vcs {
        rw_model::VcsConfig::Git(git) => git_status(root, git),
        rw_model::VcsConfig::Rcs(rcs) => Ok(vec![rcs_status(rcs)]),
        rw_model::VcsConfig::Plugin(config) => plugins::registry()
            .vcs(&config.plugin)
            .ok_or_else(|| format!("VCS plugin not registered: {}", config.plugin))?
            .status(&config.options, &plugins::resolve_paths(root, &config.paths)),
    }
}

pub fn sync(source: &Source, root: &Path) -> Result<(), String> {
    match &source.vcs {
        rw_model::VcsConfig::Git(git) => git_sync(root, git),
        rw_model::VcsConfig::Rcs(_) => Ok(()),
        rw_model::VcsConfig::Plugin(config) => plugins::registry()
            .vcs(&config.plugin)
            .ok_or_else(|| format!("VCS plugin not registered: {}", config.plugin))?
            .sync(&config.options, &plugins::resolve_paths(root, &config.paths)),
    }
}

pub fn diff(source: &Source, root: &Path, unit: Option<&str>) -> Result<DiffResult, String> {
    let name = unit
        .map(str::to_owned)
        .or_else(|| status(source, root).ok().and_then(|units| units.first().map(|u| u.name.clone())))
        .ok_or("source has no units to diff")?;

    match &source.vcs {
        rw_model::VcsConfig::Git(git) => git_diff(root, git, &name),
        rw_model::VcsConfig::Rcs(_) => Err("diff is not available for rcs sources yet".into()),
        rw_model::VcsConfig::Plugin(config) => plugins::registry()
            .vcs(&config.plugin)
            .ok_or_else(|| format!("VCS plugin not registered: {}", config.plugin))?
            .diff(&config.options, &plugins::resolve_paths(root, &config.paths), Some(&name)),
    }
}

/// Index of the first checkout that exists and is a readable repository.
/// Status and fork diffs must agree on this checkout.
fn first_open_checkout(root: &Path, git: &GitConfig) -> Option<usize> {
    git.checkouts
        .iter()
        .enumerate()
        .find(|(_, checkout)| Repository::open(root.join(&checkout.path)).is_ok())
        .map(|(index, _)| index)
}

fn git_status(root: &Path, git: &GitConfig) -> Result<Vec<UnitStatus>, String> {
    let mut units = Vec::new();
    let fork_checkout = first_open_checkout(root, git);

    for (index, checkout) in git.checkouts.iter().enumerate() {
        let path = root.join(&checkout.path);
        let name = format!("checkout:{}", checkout.name);
        if !path.exists() {
            units.push(UnitStatus { name, state: UnitState::Missing, detail: Some(format!("{} does not exist", path.display())) });
            continue;
        }
        let repo = match Repository::open(&path) {
            Ok(repo) => repo,
            Err(error) => {
                units.push(UnitStatus {
                    name,
                    state: UnitState::Missing,
                    detail: Some(format!("{}: {error}", path.display())),
                });
                continue;
            }
        };
        let head = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let branch = repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().ok().map(str::to_owned))
            .unwrap_or_else(|| checkout.branch.clone());

        let mut opts = StatusOptions::new();
        opts.include_untracked(true).recurse_untracked_dirs(true).include_ignored(false);
        let dirty = repo
            .statuses(Some(&mut opts))
            .map(|s| s.iter().any(|entry| entry.status() != Status::CURRENT))
            .unwrap_or(false);

        let tracking = format!("refs/remotes/{}/{}", checkout.remote, branch);
        let comparison = repo
            .find_reference(&tracking)
            .ok()
            .and_then(|r| r.peel_to_commit().ok())
            .zip(head.as_ref());

        let (ahead, behind, detail) = match comparison {
            Some((remote_commit, local_commit)) if remote_commit.id() != local_commit.id() => {
                let (ahead, behind) = repo
                    .graph_ahead_behind(local_commit.id(), remote_commit.id())
                    .map_err(|e| e.to_string())?;
                (ahead as u64, behind as u64, None)
            }
            Some(_) => (0, 0, None),
            None => (0, 0, Some(format!("no local ref for {tracking}; run sync"))),
        };

        let state = if dirty {
            UnitState::Dirty
        } else {
            state_from_counts(ahead, behind)
        };
        units.push(UnitStatus { name, state, detail });

        // Fork comparisons use the first checkout's branch and its fetched refs.
        // shortcut: multi-branch forks need a per-remote branch choice before this can be accurate.
        if fork_checkout == Some(index) {
            if let Some(upstream) = git.upstream() {
                let upstream_ref = format!("refs/remotes/{}/{}", upstream.name, branch);
                let upstream_commit = repo.find_reference(&upstream_ref).ok().and_then(|r| r.peel_to_commit().ok());
                for remote in git.remotes.iter().filter(|r| r.role == RemoteRole::Fork) {
                    let name = format!("remote:{}", remote.name);
                    let fork_ref = format!("refs/remotes/{}/{}", remote.name, branch);
                    let fork_commit = repo.find_reference(&fork_ref).ok().and_then(|r| r.peel_to_commit().ok());
                    let (state, detail) = match (fork_commit, upstream_commit.as_ref()) {
                        (Some(fork), Some(upstream_commit)) if fork.id() != upstream_commit.id() => {
                            let (ahead, behind) = repo
                                .graph_ahead_behind(fork.id(), upstream_commit.id())
                                .map_err(|e| e.to_string())?;
                            (state_from_counts(ahead as u64, behind as u64), None)
                        }
                        (Some(_), Some(_)) => (UnitState::Clean, None),
                        _ => (UnitState::Clean, Some(format!("no fetched refs for {fork_ref}"))),
                    };
                    units.push(UnitStatus { name, state, detail });
                }
            }
        }
    }

    Ok(units)
}

fn git_sync(root: &Path, git: &GitConfig) -> Result<(), String> {
    let mut errors = Vec::new();
    for checkout in &git.checkouts {
        let path = root.join(&checkout.path);
        let repo = match Repository::open(&path) {
            Ok(repo) => repo,
            Err(_) => match clone(&path, &git.remotes, &checkout.remote) {
                Ok(repo) => repo,
                Err(e) => {
                    errors.push(format!("{}: {e}", checkout.name));
                    continue;
                }
            },
        };

        for remote in &git.remotes {
            let result = if repo.find_remote(&remote.name).is_ok() {
                repo.remote_set_url(&remote.name, &remote.url)
            } else {
                repo.remote(&remote.name, &remote.url).map(|_| ())
            };
            if let Err(_) = result {
                errors.push(format!("{}: cannot configure remote {}", checkout.name, remote.name));
                continue;
            }
            let mut remote = repo
                .find_remote(&remote.name)
                .map_err(|e| format!("{}: {e}", checkout.name))?;
            let mut callbacks = RemoteCallbacks::new();
            callbacks.credentials(credentials);
            let mut fetch_options = git2::FetchOptions::new();
            fetch_options.remote_callbacks(callbacks);
            if let Err(e) = remote.fetch(&[] as &[&str], Some(&mut fetch_options), None) {
                let remote_name = remote.name().ok().flatten().unwrap_or("?").to_owned();
                errors.push(format!("{}: fetch {remote_name}: {e}", checkout.name));
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn clone(path: &Path, remotes: &[rw_model::GitRemote], origin: &str) -> Result<Repository, git2::Error> {
    let url = &remotes
        .iter()
        .find(|r| r.name == origin)
        .ok_or_else(|| git2::Error::from_str("origin remote not found"))?
        .url;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| git2::Error::from_str(&e.to_string()))?;
    }
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(credentials);
    let mut options = git2::FetchOptions::new();
    options.remote_callbacks(callbacks);
    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(options);
    builder.clone(url, path)
}

fn host_key(url: &str) -> Option<String> {
    let host = if let Some(rest) = url.split("://").nth(1) {
        rest.split('/')
            .next()
            .and_then(|part| part.split('@').last())
            .and_then(|part| part.split(':').next())
    } else {
        url.strip_prefix("git@")
            .and_then(|rest| rest.split(':').next())
    };
    host.filter(|host| !host.is_empty())
        .map(|host| host.to_uppercase().replace(|c: char| !c.is_ascii_alphanumeric(), "_"))
}

pub(crate) fn credentials(
    url: &str,
    username: Option<&str>,
    allowed: CredentialType,
) -> Result<Cred, git2::Error> {
    if allowed.contains(CredentialType::USERNAME) {
        return Cred::username(username.unwrap_or("git"));
    }
    if allowed.contains(CredentialType::SSH_KEY) {
        if let Some(user) = username {
            if let Ok(cred) = Cred::ssh_key_from_agent(user) {
                return Ok(cred);
            }
        }
    }
    if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) {
        if let Some(host) = host_key(url) {
            let user_var = format!("RW_GIT_USER_{host}");
            let token_var = format!("RW_GIT_TOKEN_{host}");
            if let (Ok(user), Ok(token)) = (std::env::var(&user_var), std::env::var(&token_var)) {
                return Cred::userpass_plaintext(&user, &token);
            }
            let known = matches!(host.as_str(), "GITHUB_COM" | "GITLAB_COM" | "CODEBERG_ORG");
            if known {
                if let (Ok(user), Ok(token)) = (std::env::var("RW_GIT_USER"), std::env::var("RW_GIT_TOKEN")) {
                    return Cred::userpass_plaintext(&user, &token);
                }
            }
        }
    }
    Err(git2::Error::from_str(&format!(
        "no credentials for {url}; set RW_GIT_USER_<HOST>/RW_GIT_TOKEN_<HOST> for HTTPS"
    )))
}

fn git_diff(root: &Path, git: &GitConfig, unit: &str) -> Result<DiffResult, String> {
    let (checkout, remote_name) = if let Some(name) = unit.strip_prefix("checkout:") {
        (git.checkouts.iter().find(|c| c.name == name), None)
    } else if let Some(name) = unit.strip_prefix("remote:") {
        let belongs_to_remote = git.remotes.iter().any(|r| r.name == name);
        (
            if belongs_to_remote {
                first_open_checkout(root, git).and_then(|index| git.checkouts.get(index))
            } else {
                None
            },
            Some(name.to_owned()),
        )
    } else {
        return Err(format!("unknown unit: {unit}"));
    };
    let checkout = checkout.ok_or_else(|| format!("unknown unit: {unit}"))?;
    let repo = Repository::open(root.join(&checkout.path)).map_err(|e| e.to_string())?;

    let mut options = DiffOptions::new();
    let diff = if let Some(remote_name) = remote_name {
        let branch = repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().ok().map(str::to_owned))
            .unwrap_or_else(|| checkout.branch.clone());
        let local = repo.head().and_then(|h| h.peel_to_commit()).map_err(|e| e.to_string())?;
        let remote_ref = format!("refs/remotes/{remote_name}/{branch}");
        let remote = repo
            .find_reference(&remote_ref)
            .and_then(|r| r.peel_to_commit())
            .map_err(|e| format!("{remote_ref}: {e}"))?;
        repo.diff_tree_to_tree(
            Some(&local.tree().map_err(|e| e.to_string())?),
            Some(&remote.tree().map_err(|e| e.to_string())?),
            Some(&mut options),
        )
        .map_err(|e| e.to_string())?
    } else {
        // HEAD -> worktree includes both staged and unstaged changes.
        match repo.head().ok().and_then(|h| h.peel_to_commit().ok()) {
            Some(head) => repo.diff_tree_to_workdir(
                Some(&head.tree().map_err(|e| e.to_string())?),
                Some(&mut options),
            ),
            None => repo.diff_index_to_workdir(None, Some(&mut options)),
        }
        .map_err(|e| e.to_string())?
    };

    let mut patch = Vec::new();
    diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        if let Some(text) = std::str::from_utf8(line.content()).ok() {
            let prefix = match line.origin() {
                '+' | '-' | ' ' => line.origin(),
                _ => ' ',
            };
            let _ = write!(patch, "{prefix}{text}");
        }
        true
    })
    .map_err(|e| e.to_string())?;
    let patch = String::from_utf8_lossy(&patch).into_owned();
    let patch = if patch.chars().count() > 20_000 {
        patch.chars().take(20_000).collect::<String>() + "\n... truncated"
    } else {
        patch
    };
    Ok(DiffResult { source: String::new(), unit: unit.into(), patch })
}

/// The `,v` archive location: explicit rcsDir, then the conventional RCS/
/// subfolder, then next to the file. No external `co`/`rlog` process is used.
fn rcs_archive(rcs: &RcsConfig) -> PathBuf {
    let name = rcs
        .file
        .file_name()
        .map(|name| format!("{},v", name.to_string_lossy()))
        .unwrap_or_else(|| ",v".into());
    if let Some(dir) = &rcs.rcs_dir {
        return dir.join(&name);
    }
    let parent = rcs.file.parent().unwrap_or_else(|| Path::new("."));
    let conventional = parent.join("RCS").join(&name);
    if conventional.exists() {
        conventional
    } else {
        parent.join(&name)
    }
}

fn rcs_status(rcs: &RcsConfig) -> UnitStatus {
    let name = format!("file:{}", rcs.file.file_name().unwrap_or_default().to_string_lossy());
    if !rcs.file.exists() {
        return UnitStatus {
            name,
            state: UnitState::Missing,
            detail: Some(format!("{} does not exist", rcs.file.display())),
        };
    }
    let archive = rcs_archive(rcs);
    let Ok(text) = std::fs::read_to_string(&archive) else {
        return UnitStatus {
            name,
            state: UnitState::Clean,
            detail: Some(format!("no RCS archive at {}", archive.display())),
        };
    };
    let head = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("head ").and_then(|rest| rest.strip_suffix(';')));
    let locked = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("locked ").and_then(|rest| rest.strip_suffix(';')));
    let detail = match (head, locked) {
        (Some(head), Some(locked)) => format!("head {head}, locked by {locked}"),
        (Some(head), None) => format!("head {head}"),
        (None, _) => "archive has no head".into(),
    };
    UnitStatus { name, state: UnitState::Clean, detail: Some(detail) }
}

fn state_from_counts(ahead: u64, behind: u64) -> UnitState {
    match (ahead, behind) {
        (0, 0) => UnitState::Clean,
        (a, 0) => UnitState::Ahead { count: a },
        (0, b) => UnitState::Behind { count: b },
        (a, b) => UnitState::Diverged { ahead: a, behind: b },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rw_model::{GitCheckout, GitRemote, RcsConfig, VcsConfig};
    use std::process::Command;

    struct Fixture {
        dir: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let bare = dir.path().join("bare.git");
            let work = dir.path().join("work");
            run(dir.path(), &["init", "--bare", "-q", bare.to_str().unwrap()]);
            run(dir.path(), &["clone", "-q", bare.to_str().unwrap(), work.to_str().unwrap()]);
            run(&work, &["config", "user.email", "test@example.com"]);
            run(&work, &["config", "user.name", "Test"]);
            std::fs::write(work.join("file.txt"), "one\n").unwrap();
            run(&work, &["add", "file.txt"]);
            run(&work, &["commit", "-qm", "first"]);
            run(&work, &["push", "-q", "origin", "HEAD"]);
            Self { dir }
        }

        fn source(&self) -> Source {
            Source {
                id: "test".into(),
                label: None,
                interval_secs: 60,
                imports: vec![],
            notes: String::new(),
            tags: vec![],
                vcs: VcsConfig::Git(GitConfig {
                    remotes: vec![GitRemote {
                        name: "origin".into(),
                        role: RemoteRole::Upstream,
                        url: self.dir.path().join("bare.git").to_string_lossy().into_owned(),
                        provider: None,
                    }],
                    checkouts: vec![GitCheckout {
                        name: "main".into(),
                        path: PathBuf::from("work"),
                        remote: "origin".into(),
                        branch: "master".into(),
                    }],
                }),
            }
        }
    }

    fn run(dir: &Path, args: &[&str]) {
        let out = Command::new("git").args(args).current_dir(dir).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn host_key_handles_https_and_scp_urls() {
        assert_eq!(host_key("https://github.com/a/b.git").as_deref(), Some("GITHUB_COM"));
        assert_eq!(host_key("git@git.example.com:team/repo.git").as_deref(), Some("GIT_EXAMPLE_COM"));
        assert_eq!(host_key("ssh://git@host:2222/a.git").as_deref(), Some("HOST"));
        assert_eq!(host_key("not a url"), None);
    }

    #[test]
    fn git_status_detects_dirty_and_behind_without_git_cli() {
        let fixture = Fixture::new();
        let source = fixture.source();
        let root = fixture.dir.path();

        let units = status(&source, root).unwrap();
        assert!(matches!(units[0].state, UnitState::Clean), "{units:?}");

        let bare = root.join("bare.git");
        let scratch = root.join("scratch");
        run(root, &["clone", "-q", bare.to_str().unwrap(), scratch.to_str().unwrap()]);
        run(&scratch, &["config", "user.email", "test@example.com"]);
        run(&scratch, &["config", "user.name", "Test"]);
        std::fs::write(scratch.join("file.txt"), "one\ntwo\n").unwrap();
        run(&scratch, &["add", "file.txt"]);
        run(&scratch, &["commit", "-qm", "second"]);
        run(&scratch, &["push", "-q", "origin", "HEAD"]);

        sync(&source, root).unwrap();
        let units = status(&source, root).unwrap();
        assert!(matches!(units[0].state, UnitState::Behind { count: 1 }), "{units:?}");

        std::fs::write(root.join("work").join("file.txt"), "local edit\n").unwrap();
        let units = status(&source, root).unwrap();
        assert!(matches!(units[0].state, UnitState::Dirty), "{units:?}");
    }

    #[test]
    fn diff_includes_staged_changes() {
        let fixture = Fixture::new();
        let root = fixture.dir.path();
        std::fs::write(root.join("work/file.txt"), "one\nstaged\n").unwrap();
        run(&root.join("work"), &["add", "file.txt"]);

        let patch = diff(&fixture.source(), root, Some("checkout:main")).unwrap();
        assert!(patch.patch.contains("+staged"), "{:?}", patch.patch);
    }

    #[test]
    fn plugin_source_uses_the_registered_driver() {
        plugins::register_builtins();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("exists.txt"), "x").unwrap();
        let source = Source {
            id: "files".into(),
            label: None,
            interval_secs: 60,
            imports: vec![],
            notes: String::new(),
            tags: vec![],
            vcs: VcsConfig::Plugin(rw_model::PluginConfig {
                plugin: "plain-file".into(),
                options: serde_json::Value::Null,
                paths: vec![PathBuf::from("exists.txt"), PathBuf::from("missing.txt")],
            }),
        };
        let paths = checkout_paths(&source, dir.path());
        assert_eq!(paths[0].1, dir.path().join("exists.txt"));
        let units = status(&source, dir.path()).unwrap();
        assert_eq!(units.len(), 2);
        assert!(matches!(units[0].state, UnitState::Clean));
        assert!(matches!(units[1].state, UnitState::Missing));
        sync(&source, dir.path()).unwrap();

        let unknown = Source {
            imports: vec![],
            notes: String::new(),
            tags: vec![],
            vcs: VcsConfig::Plugin(rw_model::PluginConfig {
                plugin: "nope".into(),
                ..match &source.vcs { VcsConfig::Plugin(config) => config.clone(), _ => unreachable!() }
            }),
            ..source
        };
        assert!(status(&unknown, dir.path()).is_err());
    }

    #[test]
    fn rcs_status_reads_head_and_lock_from_archive() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("hosts");
        let rcs_dir = dir.path().join("RCS");
        std::fs::create_dir_all(&rcs_dir).unwrap();
        std::fs::write(&file, "127.0.0.1 localhost\n").unwrap();
        std::fs::write(
            rcs_dir.join("hosts,v"),
            "head 1.2;\nlocked bob:1.1;\ndesc\n@text@\n",
        )
        .unwrap();
        let source = Source {
            id: "hosts".into(),
            label: None,
            interval_secs: 60,
            imports: vec![],
            notes: String::new(),
            tags: vec![],
            vcs: VcsConfig::Rcs(RcsConfig { file, rcs_dir: Some(rcs_dir) }),
        };
        let units = status(&source, dir.path()).unwrap();
        assert_eq!(units[0].name, "file:hosts");
        assert!(units[0].detail.as_deref().is_some_and(|d| d.contains("head 1.2") && d.contains("bob")));
    }

    #[test]
    fn fork_diff_uses_the_fork_ref_not_the_upstream_ref() {
        let fixture = Fixture::new();
        let root = fixture.dir.path();
        let bare = root.join("bare.git");
        let fork = root.join("fork.git");
        let fork_work = root.join("fork-work");
        run(root, &["clone", "-q", "--bare", bare.to_str().unwrap(), fork.to_str().unwrap()]);
        run(root, &["clone", "-q", fork.to_str().unwrap(), fork_work.to_str().unwrap()]);
        run(&fork_work, &["config", "user.email", "test@example.com"]);
        run(&fork_work, &["config", "user.name", "Test"]);
        std::fs::write(fork_work.join("fork-only.txt"), "from fork\n").unwrap();
        run(&fork_work, &["add", "fork-only.txt"]);
        run(&fork_work, &["commit", "-qm", "fork commit"]);
        run(&fork_work, &["push", "-q", "origin", "HEAD"]);

        let mut source = fixture.source();
        if let VcsConfig::Git(git) = &mut source.vcs {
            git.remotes.push(GitRemote {
                name: "fork".into(),
                role: RemoteRole::Fork,
                url: fork.to_string_lossy().into_owned(),
                provider: None,
            });
        }
        sync(&source, root).unwrap();
        let patch = diff(&source, root, Some("remote:fork")).unwrap();
        assert!(patch.patch.contains("fork-only.txt"), "{:?}", patch.patch);
    }
}
