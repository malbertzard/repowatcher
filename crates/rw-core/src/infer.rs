//! Everything that can be derived from a remote URL, so the user only types
//! the URL. No network access here: this is pure, testable inference.

use rw_model::{ProviderKind, Settings};
use std::path::PathBuf;
use url::Url;

#[derive(Debug, Clone, PartialEq)]
pub struct Inference {
    pub name: String,
    pub owner: Option<String>,
    pub repo: String,
    pub folder: PathBuf,
    pub ingest: String,
    pub vcs: String,
    pub provider: Option<ProviderKind>,
    pub token_env: Option<String>,
    pub url: String,
}

/// Turn a git URL (HTTPS, ssh:// or git@host:owner/repo.git) into a plan.
pub fn infer(raw: &str, settings: &Settings) -> Result<Inference, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("URL is empty".into());
    }
    let normalized = normalize_scp_syntax(raw);
    let parsed = Url::parse(&normalized).map_err(|e| format!("invalid URL: {e}"))?;
    if !matches!(parsed.scheme(), "http" | "https" | "ssh" | "git" | "git+ssh" | "file") {
        return Err(format!("unsupported URL scheme: {}", parsed.scheme()));
    }
    let is_file = parsed.scheme() == "file";
    let host = parsed.host_str().map(str::to_owned);
    let segments: Vec<String> = parsed
        .path_segments()
        .ok_or("URL has no path")?
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect();
    let repo = segments
        .last()
        .map(|part| part.trim_end_matches(".git").to_owned())
        .filter(|name| !name.is_empty() && name != "." && name != "..")
        .ok_or("URL has no repository name")?;
    let owner = if is_file || segments.len() < 2 {
        None
    } else {
        segments
            .get(segments.len() - 2)
            .map(|part| part.trim_end_matches(".git").to_owned())
            .filter(|part| !part.is_empty())
    };

    let folder_template = if settings.folder_template.contains("{owner}") && owner.is_none() {
        "{repo}".to_owned()
    } else {
        settings.folder_template.clone()
    };
    let mut folder = folder_template
        .replace("{owner}", owner.as_deref().unwrap_or(""))
        .replace("{repo}", &repo);
    // Keep the template from escaping importRoot.
    folder = folder
        .split('/')
        .filter(|part| !part.is_empty() && *part != "." && *part != "..")
        .collect::<Vec<_>>()
        .join("/");
    if folder.is_empty() {
        folder = repo.clone();
    }

    let host_config = host.as_deref().and_then(|host| settings.hosts.get(host));
    let provider = host_config
        .map(|config| config.provider)
        .or_else(|| host.as_deref().and_then(known_provider));
    let token_env = host_config
        .and_then(|config| config.token_env.clone())
        .or_else(|| known_token_env(provider));

    Ok(Inference {
        name: repo.clone(),
        owner,
        repo,
        folder: settings.import_root.join(folder),
        ingest: "git-ingest".into(),
        vcs: "git".into(),
        provider,
        token_env,
        url: raw.to_owned(),
    })
}

/// git@github.com:owner/repo.git is not a valid URL for `Url::parse`.
fn normalize_scp_syntax(raw: &str) -> String {
    if let Some(rest) = raw.strip_prefix("git@") {
        if let Some((host, path)) = rest.split_once(':') {
            return format!("ssh://git@{host}/{path}");
        }
    }
    raw.to_owned()
}

fn known_provider(host: &str) -> Option<ProviderKind> {
    match host {
        "github.com" => Some(ProviderKind::Github),
        "gitlab.com" => Some(ProviderKind::Gitlab),
        "codeberg.org" => Some(ProviderKind::Codeberg),
        _ => None,
    }
}

fn known_token_env(provider: Option<ProviderKind>) -> Option<String> {
    match provider {
        Some(ProviderKind::Github) => Some("RW_GITHUB_TOKEN".into()),
        Some(ProviderKind::Gitlab) => Some("RW_GITLAB_TOKEN".into()),
        Some(ProviderKind::Codeberg) => Some("RW_CODEBERG_TOKEN".into()),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings::default()
    }

    #[test]
    fn infers_https_git_url() {
        let plan = infer("https://github.com/acme/widget.git", &settings()).unwrap();
        assert_eq!(plan.name, "widget");
        assert_eq!(plan.owner.as_deref(), Some("acme"));
        assert_eq!(plan.folder, PathBuf::from("imports/acme/widget"));
        assert_eq!(plan.provider, Some(ProviderKind::Github));
        assert_eq!(plan.token_env.as_deref(), Some("RW_GITHUB_TOKEN"));
    }

    #[test]
    fn infers_scp_ssh_url() {
        let plan = infer("git@gitlab.com:group/sub/repo.git", &settings()).unwrap();
        assert_eq!(plan.name, "repo");
        assert_eq!(plan.repo, "repo");
        assert_eq!(plan.folder, PathBuf::from("imports/sub/repo"));
        assert_eq!(plan.provider, Some(ProviderKind::Gitlab));
    }

    #[test]
    fn handles_custom_host_and_template() {
        let mut settings = settings();
        settings.folder_template = "{repo}".into();
        settings
            .hosts
            .insert("git.example.com".into(), rw_model::HostConfig {
                provider: ProviderKind::Github,
                token_env: Some("RW_EXAMPLE_TOKEN".into()),
            });
        let plan = infer("https://git.example.com/team/tool/", &settings).unwrap();
        assert_eq!(plan.folder, PathBuf::from("imports/tool"));
        assert_eq!(plan.token_env.as_deref(), Some("RW_EXAMPLE_TOKEN"));
    }

    #[test]
    fn infers_local_file_url() {
        let plan = infer("file:///tmp/repos/local.git", &settings()).unwrap();
        assert_eq!(plan.name, "local");
        assert_eq!(plan.owner, None);
        assert_eq!(plan.folder, PathBuf::from("imports/local"));
        assert_eq!(plan.provider, None);
    }

    #[test]
    fn owner_is_chosen_by_position() {
        let plan = infer("https://github.com/acme/acme.git", &settings()).unwrap();
        assert_eq!(plan.owner.as_deref(), Some("acme"));
        assert_eq!(plan.folder, PathBuf::from("imports/acme/acme"));

        let plan = infer("https://gitea.local/widget.git", &settings()).unwrap();
        assert_eq!(plan.owner, None);
        assert_eq!(plan.folder, PathBuf::from("imports/widget"));
    }

    #[test]
    fn rejects_bad_urls() {
        assert!(infer("", &settings()).is_err());
        assert!(infer("not a url", &settings()).is_err());
        assert!(infer("ftp://example.com/repo.git", &settings()).is_err());
    }
}
