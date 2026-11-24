use crate::{AppState, SharedState, default_config_path, drivers, store::Store, watcher};
use futures::stream::StreamExt;
use axum::{
    Json, Router,
    extract::{State, WebSocketUpgrade, ws::Message, ws::WebSocket},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use rw_model::{ApiResult, Command, Event, ImportPreview, Source, State as ApiState};
use std::collections::BTreeMap;
use std::fs;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{RwLock, broadcast};

pub async fn run_daemon(config_path: Option<std::path::PathBuf>, port: u16) -> Result<(), String> {
    let path = config_path.unwrap_or_else(default_config_path);
    let store = Store::new(path);
    let _lock = store.lock()?;
    let config = store.load()?;
    let token = store.token()?;
    let (events, _) = broadcast::channel(256);

    let state: SharedState = Arc::new(AppState {
        store,
        config: RwLock::new(config),
        statuses: RwLock::new(BTreeMap::new()),
        events,
        token,
        dist: crate::default_dist_dir(),
        running: Default::default(),
        ui_clients: Default::default(),
    });

    println!("rw daemon on http://127.0.0.1:{port}/ (use `rw url` for the link)");
    crate::plugins::register_builtins();
    watcher::spawn(state.clone());

    let app = Router::new()
        .route("/", get(index))
        .route("/{*path}", get(static_file))
        .nest(
            "/api",
            Router::new()
                .route("/state", get(get_state))
                .route("/providers", get(get_providers))
                .route("/plugins", get(get_plugins))
                .route("/actions", get(get_actions))
                .route("/kinds", get(get_kinds))
                .route("/command", post(post_command))
                .route("/events", get(get_events))
                .layer(middleware::from_fn_with_state(state.clone(), auth)),
        )
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("bind {addr}: {e}"))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| format!("server: {e}"))
}

async fn auth(State(state): State<SharedState>, headers: HeaderMap, req: axum::extract::Request, next: Next) -> Response {
    let from_header = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let from_query = if req.uri().path().ends_with("/events") {
        req.uri().query().and_then(parse_token)
    } else {
        None
    };
    if token_matches(from_header, &state.token) || token_matches(from_query, &state.token) {
        next.run(req).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

fn token_matches(given: Option<&str>, expected: &str) -> bool {
    let given = given.unwrap_or("");
    let mut difference = given.len() ^ expected.len();
    for (actual, wanted) in given.bytes().zip(expected.bytes()) {
        difference |= (actual ^ wanted) as usize;
    }
    difference == 0
}

fn parse_token(query: &str) -> Option<&str> {
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("token="))
        .filter(|token| !token.is_empty())
}

async fn get_state(State(state): State<SharedState>) -> Json<ApiState> {
    Json(ApiState {
        config: state.config.read().await.clone(),
        statuses: state.statuses.read().await.values().cloned().collect(),
    })
}

async fn get_providers(State(state): State<SharedState>) -> Json<Vec<rw_model::ProviderSnapshot>> {
    let config = state.config.read().await.clone();
    Json(crate::providers::fetch_all(&config).await)
}

async fn get_actions() -> Json<Vec<rw_model::SourceAction>> {
    Json(crate::plugins::registry().actions())
}

async fn get_kinds() -> Json<Vec<rw_model::SourceKind>> {
    Json(crate::plugins::source_kinds())
}

async fn get_plugins() -> Json<rw_model::PluginNames> {
    Json(crate::plugins::registry().names())
}

async fn post_command(
    State(state): State<SharedState>,
    Json(command): Json<Command>,
) -> Json<ApiResult> {
    match command {
        Command::UpdateProject { id, notes, tags } => {
            let mut missing = false;
            let result = persist_config(&state, |config| {
                missing = !update_project(config, &id, notes, tags);
            })
            .await;
            if missing {
                return Json(ApiResult::err(format!("unknown source: {id}")));
            }
            result
        }
        Command::Integrations { source } => {
            let config = state.config.read().await.clone();
            let Some(found) = config.sources.iter().find(|item| item.id == source).cloned() else {
                return Json(ApiResult::err(format!("unknown source: {source}")));
            };
            let settings = config.settings.clone();
            let mut tasks = Vec::new();
            for name in crate::plugins::registry().integrations() {
                let Some(plugin) = crate::plugins::registry().integration(&name) else { continue };
                let source = found.clone();
                let plugin_settings = settings.clone();
                tasks.push(async move {
                    let result = tokio::task::spawn_blocking(move || plugin.fetch(&source, &plugin_settings)).await;
                    (name, result)
                });
            }
            let mut snapshots = Vec::new();
            for (name, result) in futures::future::join_all(tasks).await {
                match result {
                    Ok(Ok(snapshot)) => snapshots.push(snapshot),
                    Ok(Err(error)) => snapshots.push(rw_model::IntegrationSnapshot {
                        plugin: name.clone(),
                        title: name,
                        items: Vec::new(),
                        error: Some(error),
                    }),
                    Err(error) => snapshots.push(rw_model::IntegrationSnapshot {
                        plugin: name.clone(),
                        title: name,
                        items: Vec::new(),
                        error: Some(format!("integration task: {error}")),
                    }),
                }
            }
            let mut result = ApiResult::ok();
            result.integrations = snapshots;
            Json(result)
        }
        Command::IntegrationAction { source, plugin, action, value } => {
            let config = state.config.read().await.clone();
            let Some(found) = config.sources.iter().find(|item| item.id == source).cloned() else {
                return Json(ApiResult::err(format!("unknown source: {source}")));
            };
            let Some(integration) = crate::plugins::registry().integration(&plugin) else {
                return Json(ApiResult::err(format!("integration not registered: {plugin}")));
            };
            let root = config.root_folder.clone();
            let result = tokio::task::spawn_blocking(move || integration.run_action(&found, &root, &action, &value)).await;
            match result {
                Ok(Ok(message)) => Json(ApiResult::message(message)),
                Ok(Err(error)) => Json(ApiResult::err(error)),
                Err(error) => Json(ApiResult::err(format!("integration task: {error}"))),
            }
        }
        Command::RunAction { source, action } => {
            let config = state.config.read().await.clone();
            let Some(found) = config.sources.iter().find(|item| item.id == source).cloned() else {
                return Json(ApiResult::err(format!("unknown source: {source}")));
            };
            let Some(plugin) = crate::plugins::registry().action(&action) else {
                return Json(ApiResult::err(format!("action not registered: {action}")));
            };
            let vcs = match &found.vcs {
                rw_model::VcsConfig::Git(_) => "git",
                rw_model::VcsConfig::Rcs(_) => "rcs",
                rw_model::VcsConfig::Plugin(_) => "plugin",
            };
            let metadata = plugin.metadata();
            if !metadata.vcs.iter().any(|name| name == "*" || name == vcs) {
                return Json(ApiResult::err(format!("action {action} does not apply to {vcs}")));
            }
            let root = config.root_folder.clone();
            let result = tokio::task::spawn_blocking(move || plugin.run(&found, &root)).await;
            match result {
                Ok(Ok(message)) => Json(ApiResult::message(message)),
                Ok(Err(error)) => Json(ApiResult::err(error)),
                Err(error) => Json(ApiResult::err(format!("action task: {error}"))),
            }
        }
        Command::SaveConfig { config } => persist_config(&state, |current| *current = config).await,
        Command::Sync { source } => match watcher::refresh_source(&state, &source).await {
            Ok(()) => Json(ApiResult::ok()),
            Err(e) => Json(ApiResult::err(e)),
        },
        Command::Diff { source, unit } => {
            let config = state.config.read().await.clone();
            let root = config.root_folder.clone();
            let result = match config.sources.into_iter().find(|s| s.id == source) {
                Some(found) => tokio::task::spawn_blocking(move || {
                    drivers::diff(&found, &root, unit.as_deref())
                })
                .await,
                None => return Json(ApiResult::err(format!("unknown source: {source}"))),
            };
            match result {
                Ok(Ok(mut diff)) => {
                    diff.source = source;
                    Json(ApiResult::diff(diff))
                }
                Ok(Err(e)) => Json(ApiResult::err(e)),
                Err(e) => Json(ApiResult::err(format!("diff task: {e}"))),
            }
        }
        Command::Open { source } => {
            let config = state.config.read().await.clone();
            let Some(found) = config.sources.iter().find(|s| s.id == source) else {
                return Json(ApiResult::err(format!("unknown source: {source}")));
            };
            let open = &config.settings.open;
            if open.command.trim().is_empty() {
                return Json(ApiResult::err("no open command configured"));
            }
            let path = drivers::checkout_paths(found, &config.root_folder)
                .into_iter()
                .next()
                .map(|(_, path)| path)
                .unwrap_or_else(|| config.root_folder.clone());
            let args = open.args.iter().map(|arg| arg.replace("{path}", &path.to_string_lossy()));
            match tokio::process::Command::new(&open.command).args(args).spawn() {
                Ok(_) => Json(ApiResult::ok()),
                Err(e) => Json(ApiResult::err(format!("open {}: {e}", path.display()))),
            }
        }
        Command::Import { source } => {
            let config = state.config.read().await.clone();
            let Some(parent) = config.sources.iter().find(|item| item.id == source) else {
                return Json(ApiResult::err(format!("unknown source: {source}")));
            };
            if parent.imports.is_empty() {
                return Json(ApiResult::err("source has no imports configured"));
            }
            let mut failures = Vec::new();
            for import in &parent.imports {
                let plan = match crate::infer::infer(&import.url, &config.settings) {
                    Ok(plan) => plan,
                    Err(error) => {
                        failures.push(error);
                        continue;
                    }
                };
                let ingest = import.ingest.clone().unwrap_or(plan.ingest);
                let path = import.path.clone().unwrap_or(plan.folder);
                if let Err(error) = run_ingest(
                    &state,
                    &ingest,
                    &import.url,
                    &path,
                    &import.options,
                    config.settings.default_interval_secs,
                    None,
                )
                .await
                {
                    failures.push(error);
                }
            }
            if failures.is_empty() {
                Json(ApiResult::ok())
            } else {
                Json(ApiResult::err(failures.join("; ")))
            }
        }
        Command::ImportPreview { url } => {
            let config = state.config.read().await.clone();
            match crate::infer::infer(&url, &config.settings) {
                Ok(plan) => Json(ApiResult::preview(ImportPreview {
                    name: plan.name,
                    owner: plan.owner,
                    repo: Some(plan.repo),
                    folder: plan.folder.to_string_lossy().into_owned(),
                    ingest: plan.ingest,
                    vcs: plan.vcs,
                    provider: plan.provider.map(|kind| format!("{kind:?}").to_lowercase()),
                })),
                Err(error) => Json(ApiResult::err(error)),
            }
        }
        Command::QuickImport { url, folder, ingest } => {
            let config = state.config.read().await.clone();
            let plan = match crate::infer::infer(&url, &config.settings) {
                Ok(plan) => plan,
                Err(error) => return Json(ApiResult::err(error)),
            };
            let path = folder.map(std::path::PathBuf::from).unwrap_or(plan.folder);
            match run_ingest(
                &state,
                &ingest.unwrap_or(plan.ingest),
                &url,
                &path,
                &serde_json::Value::Null,
                config.settings.default_interval_secs,
                None,
            )
            .await
            {
                Ok(_) => Json(ApiResult::ok()),
                Err(error) => Json(ApiResult::err(error)),
            }
        }
        Command::ExportBundle => {
            let config = state.config.read().await.clone();
            let sources = config.sources.iter().filter(|source| source.can_export()).cloned().collect();
            let mut result = ApiResult::ok();
            result.export = Some(rw_model::ExportBundle {
                version: 1,
                exported_at: now(),
                settings: config.settings,
                sources,
            });
            Json(result)
        }
        Command::ImportBundle { bundle } => {
            if bundle.version != 1 {
                return Json(ApiResult::err(format!("unsupported bundle version: {}", bundle.version)));
            }
            let config = state.config.read().await.clone();
            let mut count = 0usize;
            let mut failures = Vec::new();
            let mut tasks = Vec::new();
            for source in bundle.sources.iter().filter(|source| source.can_export()) {
                let url = match &source.vcs {
                    rw_model::VcsConfig::Git(git) => git.remotes.first().map(|remote| remote.url.clone()),
                    rw_model::VcsConfig::Plugin(_) => source.imports.first().map(|import| import.url.clone()),
                    _ => None,
                };
                let Some(url) = url else { continue };
                let plan = match crate::infer::infer(&url, &config.settings) {
                    Ok(plan) => plan,
                    Err(error) => {
                        failures.push(error);
                        continue;
                    }
                };
                let state = state.clone();
                let interval = config.settings.default_interval_secs;
                let source = source.clone();
                tasks.push(async move {
                    run_ingest(
                        &state,
                        &plan.ingest,
                        &url,
                        &plan.folder,
                        &serde_json::Value::Null,
                        interval,
                        Some(&source),
                    )
                    .await
                });
            }
            let mut stream = futures::stream::iter(tasks).buffer_unordered(4);
            while let Some(result) = stream.next().await {
                match result {
                    Ok(true) => count += 1,
                    Ok(false) => {}
                    Err(error) => failures.push(error),
                }
            }
            let message = format!("Imported {count} sources{}", if failures.is_empty() { String::new() } else { format!("; failed: {}", failures.join("; ")) });
            let mut result = ApiResult::message(message);
            if !failures.is_empty() {
                result.ok = false;
                result.error = Some(failures.join("; "));
            }
            Json(result)
        }
    }
}

async fn run_ingest(
    state: &SharedState,
    ingest: &str,
    url: &str,
    path: &std::path::Path,
    options: &serde_json::Value,
    interval: u64,
    source_meta: Option<&rw_model::Source>,
) -> Result<bool, String> {
    let already_registered = {
        let current = state.config.read().await;
        registered_url(&current, url)
    };
    if already_registered {
        return Ok(false);
    }
    let plugin = crate::plugins::registry()
        .ingest(ingest)
        .ok_or_else(|| format!("ingest system not registered: {ingest}"))?;
    let source_url = url.to_owned();
    let relative = path.to_path_buf();
    let opts = options.clone();
    let root = state.config.read().await.root_folder.clone();
    let ingest_root = root.clone();
    let result =
        tokio::task::spawn_blocking(move || plugin.ingest(&source_url, &ingest_root, &relative, &opts)).await;
    let ingested = match result {
        Ok(Ok(ingested)) => ingested,
        Ok(Err(error)) => return Err(error),
        Err(error) => return Err(format!("ingest task: {error}")),
    };

    let mut vcs = ingested.vcs;
    if let rw_model::VcsConfig::Git(git) = &mut vcs {
        if let Some(branch) = detect_branch(&root, path) {
            for checkout in &mut git.checkouts {
                checkout.branch = branch.clone();
            }
        }
    }
    let base = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "imported".into());
    let label = source_meta
        .and_then(|source| source.label.clone())
        .or(ingested.label)
        .or_else(|| Some(base.clone()));
    let notes = source_meta.map(|source| source.notes.clone()).unwrap_or_default();
    let tags = source_meta.map(|source| source.tags.clone()).unwrap_or_default();
    let mut added = false;
    let saved = persist_config(state, |current| {
        // The check above is only a fast path; this one holds the write lock.
        if registered_url(current, url) {
            return;
        }
        let mut id = base.clone();
        let mut suffix = 2;
        while current.sources.iter().any(|item| item.id == id) {
            id = format!("{base}-{suffix}");
            suffix += 1;
        }
        current.sources.push(Source {
            id,
            label,
            interval_secs: interval,
            imports: Vec::new(),
            notes,
            tags,
            vcs,
        });
        added = true;
    })
    .await;
    match saved.error.clone() {
        Some(error) => Err(error),
        None => Ok(added),
    }
}

fn detect_branch(root: &std::path::Path, relative: &std::path::Path) -> Option<String> {
    git2::Repository::open(root.join(relative))
        .ok()?
        .head()
        .ok()?
        .shorthand()
        .ok()
        .map(str::to_owned)
}

fn registered_url(config: &rw_model::Config, url: &str) -> bool {
    config.sources.iter().any(|source| match &source.vcs {
        rw_model::VcsConfig::Git(git) => git.remotes.iter().any(|remote| remote.url == url),
        // A plugin source declares imports; it is not itself an imported clone.
        rw_model::VcsConfig::Plugin(_) => false,
        _ => false,
    })
}

fn update_project(config: &mut rw_model::Config, id: &str, notes: String, tags: Vec<String>) -> bool {
    let Some(source) = config.sources.iter_mut().find(|source| source.id == id) else {
        return false;
    };
    source.notes = notes;
    source.tags = tags;
    true
}

async fn persist_config<F>(state: &SharedState, mutate: F) -> Json<ApiResult>
where
    F: FnOnce(&mut rw_model::Config),
{
    // Hold the write lock across the read, mutation and disk write so two
    // saves cannot leave the file and memory holding different configs.
    let mut current = state.config.write().await;
    let mut config = current.clone();
    mutate(&mut config);
    match state.store.save(&config) {
        Ok(()) => {
            let live: std::collections::HashSet<&str> =
                config.sources.iter().map(|source| source.id.as_str()).collect();
            state.statuses.write().await.retain(|id, _| live.contains(id.as_str()));
            *current = config;
            state.emit(Event::ConfigSaved { at: now() });
            Json(ApiResult::ok())
        }
        Err(e) => Json(ApiResult::err(e)),
    }
}

async fn get_events(State(state): State<SharedState>, ws: WebSocketUpgrade) -> Response {
    // Auth is handled by the /api middleware, which also accepts ?token= for
    // browsers that cannot set a header on a WebSocket handshake.
    ws.on_upgrade(move |socket| async move {
        state.ui_clients.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        forward_events(socket, state.clone()).await;
        state.ui_clients.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    })
}

async fn forward_events(mut socket: WebSocket, state: SharedState) {
    let mut rx = state.events.subscribe();
    // A ping makes dead connections fail even when no events are emitted, so
    // the ui_clients counter (and WhileWindowOpen polling gate) stays accurate.
    let mut ping = tokio::time::interval(std::time::Duration::from_secs(10));
    enum Action {
        Event(Event),
        Ping,
    }
    loop {
        let action = tokio::select! {
            event = rx.recv() => match event {
                Ok(event) => Action::Event(event),
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            _ = ping.tick() => Action::Ping,
        };
        let message = match action {
            Action::Event(event) => Message::Text(serde_json::to_string(&event).unwrap_or_default().into()),
            Action::Ping => Message::Ping(Default::default()),
        };
        if socket.send(message).await.is_err() {
            break;
        }
    }
}

async fn index(State(state): State<SharedState>) -> Response {
    let path = state.dist.join("index.html");
    match fs::read_to_string(&path) {
        Ok(html) => Html(html).into_response(),
        Err(_) => Html(String::from(
            "<!doctype html><title>RepoWatcher</title><p>UI not built yet. Run trunk build in ui/.</p>",
        ))
        .into_response(),
    }
}

fn safe_dist_path(dist: &std::path::Path, relative: &str) -> Option<std::path::PathBuf> {
    let mut path = dist.to_path_buf();
    for part in std::path::Path::new(relative).components() {
        if !matches!(part, std::path::Component::Normal(_)) {
            return None;
        }
        path.push(part);
    }
    Some(path)
}

async fn static_file(
    State(state): State<SharedState>,
    axum::extract::Path(relative): axum::extract::Path<String>,
) -> Response {
    let Some(path) = safe_dist_path(&state.dist, &relative) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match fs::read(&path) {
        Ok(bytes) => {
            let content_type = match path.extension().and_then(|e| e.to_str()) {
                Some("wasm") => "application/wasm",
                Some("js") => "text/javascript",
                Some("css") => "text/css",
                Some("html") => "text/html; charset=utf-8",
                Some("svg") => "image/svg+xml",
                _ => "application/octet-stream",
            };
            ([(axum::http::header::CONTENT_TYPE, content_type)], bytes).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_files_cannot_escape_the_dist_directory() {
        let dist = std::path::Path::new("/tmp/rw-ui");
        assert_eq!(safe_dist_path(dist, "app.js"), Some(dist.join("app.js")));
        assert_eq!(safe_dist_path(dist, "../config.yaml"), None);
        assert_eq!(safe_dist_path(dist, "/etc/passwd"), None);
        assert_eq!(safe_dist_path(dist, "a/../b.js"), None);
    }

    #[test]
    fn plugin_import_is_not_a_registered_clone() {
        let url = "https://example.com/a.git";
        let config = rw_model::Config {
            root_folder: std::path::PathBuf::from("/tmp"),
            settings: rw_model::Settings::default(),
            sources: vec![rw_model::Source {
                id: "importer".into(),
                label: None,
                interval_secs: 120,
                imports: vec![rw_model::ImportSpec {
                    url: url.into(),
                    ingest: None,
                    path: None,
                    options: serde_json::Value::Null,
                }],
                notes: String::new(),
                tags: vec![],
                vcs: rw_model::VcsConfig::Plugin(rw_model::PluginConfig {
                    plugin: "plain-file".into(),
                    options: serde_json::Value::Null,
                    paths: vec![],
                }),
            }],
        };
        assert!(!registered_url(&config, url));

        let mut cloned = config;
        cloned.sources.push(rw_model::Source {
            id: "clone".into(),
            label: None,
            interval_secs: 120,
            imports: vec![],
            notes: String::new(),
            tags: vec![],
            vcs: rw_model::VcsConfig::Git(rw_model::GitConfig {
                remotes: vec![rw_model::GitRemote {
                    name: "origin".into(),
                    role: rw_model::RemoteRole::Upstream,
                    url: url.into(),
                    provider: None,
                }],
                checkouts: vec![],
            }),
        });
        assert!(registered_url(&cloned, url));
    }

    #[test]
    fn project_update_only_touches_the_named_source() {
        let source = |id: &str, notes: &str| rw_model::Source {
            id: id.into(),
            label: None,
            interval_secs: 10,
            imports: vec![],
            notes: notes.into(),
            tags: vec![],
            vcs: rw_model::VcsConfig::Rcs(rw_model::RcsConfig {
                file: "f".into(),
                rcs_dir: None,
            }),
        };
        let mut config = rw_model::Config {
            root_folder: std::path::PathBuf::from("/tmp"),
            settings: rw_model::Settings::default(),
            sources: vec![source("a", "old"), source("b", "keep")],
        };
        assert!(update_project(&mut config, "a", "new".into(), vec!["work".into()]));
        assert_eq!(config.sources[0].notes, "new");
        assert_eq!(config.sources[0].tags, vec!["work"]);
        assert_eq!(config.sources[1].notes, "keep");
        assert!(!update_project(&mut config, "missing", "x".into(), vec![]));
        assert_eq!(config.sources[0].notes, "new");
    }

    #[test]
    fn token_matches_checks_the_whole_value() {
        assert!(token_matches(Some("abc"), "abc"));
        assert!(!token_matches(Some("abd"), "abc"));
        assert!(!token_matches(Some("ab"), "abc"));
        assert!(!token_matches(Some("abcd"), "abc"));
        assert!(!token_matches(None, "abc"));
    }

    #[test]
    fn token_query_parses_only_named_parameter() {
        assert_eq!(parse_token("x=1&token=abc"), Some("abc"));
        assert_eq!(parse_token("token="), None);
        assert_eq!(parse_token("other=abc"), None);
    }
}
