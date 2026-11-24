//! Remote forge APIs. Opt-in per git remote; no secrets are stored in config.
//! Tokens come from RW_GITHUB_TOKEN / RW_GITLAB_TOKEN / RW_CODEBERG_TOKEN.

use futures::stream::{self, StreamExt};
use rw_model::{Config, PipelineRun, ProviderKind, ProviderRef, ProviderSnapshot, PullRequest, VcsConfig};

pub async fn fetch_all(config: &Config) -> Vec<ProviderSnapshot> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let mut tasks = Vec::new();
    for source in &config.sources {
        let VcsConfig::Git(git) = &source.vcs else { continue };
        for remote in &git.remotes {
            let Some(provider) = &remote.provider else { continue };
            let source_id = source.id.clone();
            let remote_name = remote.name.clone();
            let provider = provider.clone();
            let default_token = source.default_token_env(&config.settings);
            let client = client.clone();
            tasks.push(async move {
                fetch(&client, &source_id, &remote_name, &provider, default_token.as_deref()).await
            });
        }
    }
    stream::iter(tasks).buffer_unordered(4).collect::<Vec<_>>().await
}

async fn fetch(
    client: &reqwest::Client,
    source: &str,
    remote: &str,
    provider: &ProviderRef,
    default_token_env: Option<&str>,
) -> ProviderSnapshot {
    let mut snapshot = ProviderSnapshot {
        source: source.into(),
        remote: remote.into(),
        kind: provider.kind,
        pull_requests: Vec::new(),
        pipelines: Vec::new(),
        error: None,
    };
    let token_name = provider
        .token_env
        .clone()
        .or_else(|| default_token_env.map(str::to_owned))
        .unwrap_or_else(|| token_var(provider.kind));
    let Ok(token) = std::env::var(&token_name) else {
        snapshot.error = Some(format!("{token_name} is not set"));
        return snapshot;
    };
    let headers = [
        ("User-Agent", "repowatcher"),
        ("Authorization", &format!("Bearer {token}")),
    ];

    let pulls_url = pulls_url(provider.kind, &provider.repo);
    match get_json(&client, &pulls_url, &headers).await {
        Ok(value) => snapshot.pull_requests = map_pulls(provider.kind, &value),
        Err(error) => snapshot.error = Some(format!("pull requests: {error}")),
    }

    if let Some(runs_url) = runs_url(provider.kind, &provider.repo) {
        match get_json(&client, &runs_url, &headers).await {
            Ok(value) => snapshot.pipelines = map_pipelines(provider.kind, &value),
            Err(error) => {
                let message = format!("pipelines: {error}");
                snapshot.error = Some(match snapshot.error.take() {
                    Some(previous) => format!("{previous}; {message}"),
                    None => message,
                });
            }
        }
    }
    snapshot
}

async fn get_json(
    client: &reqwest::Client,
    url: &str,
    headers: &[(&str, &str)],
) -> Result<serde_json::Value, String> {
    let mut request = client.get(url);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = request.send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    response.json().await.map_err(|e| e.to_string())
}

pub(crate) fn base_url(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Github => "https://api.github.com",
        ProviderKind::Gitlab => "https://gitlab.com/api/v4",
        ProviderKind::Codeberg => "https://codeberg.org/api/v1",
    }
}

pub(crate) fn token_var(kind: ProviderKind) -> String {
    match kind {
        ProviderKind::Github => "RW_GITHUB_TOKEN",
        ProviderKind::Gitlab => "RW_GITLAB_TOKEN",
        ProviderKind::Codeberg => "RW_CODEBERG_TOKEN",
    }
    .into()
}

fn github_style(kind: ProviderKind) -> bool {
    matches!(kind, ProviderKind::Github | ProviderKind::Codeberg)
}

pub(crate) fn pulls_url(kind: ProviderKind, repo: &str) -> String {
    let base = base_url(kind);
    match kind {
        ProviderKind::Github | ProviderKind::Codeberg => {
            format!("{base}/repos/{repo}/pulls?state=all&per_page=10")
        }
        ProviderKind::Gitlab => format!(
            "{base}/projects/{}/merge_requests?state=all&per_page=10",
            repo.replace('/', "%2F")
        ),
    }
}

fn runs_url(kind: ProviderKind, repo: &str) -> Option<String> {
    let base = base_url(kind);
    match kind {
        ProviderKind::Github => Some(format!("{base}/repos/{repo}/actions/runs?per_page=10")),
        ProviderKind::Gitlab => Some(format!(
            "{base}/projects/{}/pipelines?per_page=10",
            repo.replace('/', "%2F")
        )),
        ProviderKind::Codeberg => None,
    }
}

pub(crate) fn map_pulls(kind: ProviderKind, value: &serde_json::Value) -> Vec<PullRequest> {
    let Some(items) = value.as_array() else { return Vec::new() };
    items
        .iter()
        .map(|item| PullRequest {
            number: field_u64(item, if github_style(kind) { "number" } else { "iid" }),
            title: field_string(item, "title"),
            state: field_string(item, "state"),
            url: field_string(item, if github_style(kind) { "html_url" } else { "web_url" }),
        })
        .collect()
}

pub(crate) fn map_pipelines(kind: ProviderKind, value: &serde_json::Value) -> Vec<PipelineRun> {
    let items = if github_style(kind) {
        value.get("workflow_runs").and_then(|v| v.as_array())
    } else {
        value.as_array()
    };
    let Some(items) = items else { return Vec::new() };
    items
        .iter()
        .map(|item| PipelineRun {
            id: field_u64(item, "id"),
            status: field_string(item, "status"),
            conclusion: item.get("conclusion").and_then(|v| v.as_str()).map(str::to_owned),
            url: field_string(item, if github_style(kind) { "html_url" } else { "web_url" }),
        })
        .collect()
}

fn field_string(value: &serde_json::Value, key: &str) -> String {
    value.get(key).and_then(|v| v.as_str()).unwrap_or("").to_owned()
}

fn field_u64(value: &serde_json::Value, key: &str) -> u64 {
    value.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_pulls_and_runs_are_mapped() {
        let pulls = serde_json::json!([{ "number": 7, "title": "Fix", "state": "open", "html_url": "https://example.com/7" }]);
        let runs = serde_json::json!({ "workflow_runs": [{ "id": 12, "status": "completed", "conclusion": "success", "html_url": "https://example.com/12" }] });
        let mapped = map_pulls(ProviderKind::Github, &pulls);
        let pipelines = map_pipelines(ProviderKind::Github, &runs);
        assert_eq!(mapped[0].number, 7);
        assert_eq!(mapped[0].title, "Fix");
        assert_eq!(pipelines[0].status, "completed");
        assert_eq!(pipelines[0].conclusion.as_deref(), Some("success"));
    }

    #[test]
    fn gitlab_merge_requests_and_pipelines_are_mapped() {
        let mrs = serde_json::json!([{ "iid": 3, "title": "MR", "state": "merged", "web_url": "https://example.com/3" }]);
        let pipelines = serde_json::json!([{ "id": 9, "status": "success", "web_url": "https://example.com/9" }]);
        assert_eq!(map_pulls(ProviderKind::Gitlab, &mrs)[0].number, 3);
        assert_eq!(map_pipelines(ProviderKind::Gitlab, &pipelines)[0].url, "https://example.com/9");
    }
}
