//! One polling loop for all sources. A source that fails never blocks the others.

use crate::{SharedState, drivers};
use rw_model::{Event, Source, SourceStatus};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SYNC_TIMEOUT: Duration = Duration::from_secs(300);

/// Run one source now, from an API command rather than the timer.
pub async fn refresh_source(state: &SharedState, id: &str) -> Result<(), String> {
    let config = state.config.read().await.clone();
    let source = config
        .sources
        .iter()
        .find(|source| source.id == id)
        .cloned()
        .ok_or_else(|| format!("unknown source: {id}"))?;
    let fresh = run_refresh(state, source, config.root_folder).await?;
    let error = fresh.error.clone();
    publish(state, fresh).await;
    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

pub fn spawn(state: SharedState) {
    tokio::spawn(async move {
        let mut next_run: HashMap<String, Instant> = HashMap::new();
        let mut tick = tokio::time::interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            let (sources, root, polling_enabled, defaults) = {
                let config = state.config.read().await;
                let enabled = match config.settings.background {
                    rw_model::BackgroundMode::Always => true,
                    rw_model::BackgroundMode::WhileWindowOpen => state.ui_connected(),
                };
                (
                    config.sources.clone(),
                    config.root_folder.clone(),
                    enabled,
                    config.settings.source_defaults.clone(),
                )
            };
            if !polling_enabled {
                continue;
            }
            let live: std::collections::HashSet<String> =
                sources.iter().map(|source| source.id.clone()).collect();
            next_run.retain(|id, _| live.contains(id));
            {
                let mut statuses = state.statuses.write().await;
                statuses.retain(|id, _| live.contains(id));
            }

            let now = Instant::now();
            for source in sources {
                let (interval, auto_fetch) = effective_settings(&source, &defaults);
                if !auto_fetch
                    || state.running.lock().await.contains(&source.id)
                    || next_run.get(&source.id).is_some_and(|due| *due > now)
                {
                    continue;
                }
                next_run.insert(source.id.clone(), now + Duration::from_secs(interval.max(1)));
                let source_id = source.id.clone();
                let state = state.clone();
                let root = root.clone();
                tokio::spawn(async move {
                    match run_refresh(&state, source, root).await {
                        Ok(fresh) => publish(&state, fresh).await,
                        Err(error) => state.emit(Event::Failed {
                            source: source_id,
                            message: error,
                            at: unix_now(),
                        }),
                    }
                });
            }
        }
    });
}

async fn run_refresh(
    state: &SharedState,
    source: Source,
    root: PathBuf,
) -> Result<SourceStatus, String> {
    {
        let mut running = state.running.lock().await;
        if !running.insert(source.id.clone()) {
            return Err(format!("source {} is already syncing", source.id));
        }
    }
    let id = source.id.clone();
    let mut handle = tokio::task::spawn_blocking(move || refresh(&source, &root));
    let mut did_timeout = false;
    let completed = tokio::select! {
        result = &mut handle => Some(result),
        _ = tokio::time::sleep(SYNC_TIMEOUT) => {
            // Report the timeout, but wait for the blocking task before freeing the
            // guard. Two concurrent fetches on one repo can corrupt refs.
            did_timeout = true;
            None
        }
    };

    let joined = match completed {
        Some(result) => result,
        None => handle.await,
    };
    state.running.lock().await.remove(&id);

    if did_timeout {
        return Ok(SourceStatus {
            id,
            units: Vec::new(),
            error: Some(format!("sync timed out after {}s", SYNC_TIMEOUT.as_secs())),
        });
    }
    Ok(match joined {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => SourceStatus { id, units: Vec::new(), error: Some(e) },
        Err(e) => SourceStatus { id, units: Vec::new(), error: Some(format!("sync task: {e}")) },
    })
}

fn refresh(source: &Source, root: &Path) -> Result<SourceStatus, String> {
    drivers::sync(source, root)?;
    Ok(SourceStatus {
        id: source.id.clone(),
        units: drivers::status(source, root)?,
        error: None,
    })
}

/// Per-type defaults are overridden by tag defaults; the source's own value
/// is the starting point. Key format: "type:git" and "tag:work".
fn effective_settings(
    source: &Source,
    defaults: &std::collections::BTreeMap<String, rw_model::SourceDefault>,
) -> (u64, bool) {
    let vcs = match &source.vcs {
        rw_model::VcsConfig::Git(_) => "git",
        rw_model::VcsConfig::Rcs(_) => "rcs",
        rw_model::VcsConfig::Plugin(config) => config.plugin.as_str(),
    };
    let mut interval = source.interval_secs;
    let mut auto_fetch = true;
    for key in std::iter::once(format!("type:{vcs}"))
        .chain(source.tags.iter().map(|tag| format!("tag:{}", tag.to_lowercase())))
    {
        if let Some(default) = defaults.get(&key) {
            if let Some(value) = default.interval_secs {
                interval = value;
            }
            if let Some(value) = default.auto_fetch {
                auto_fetch = value;
            }
        }
    }
    (interval, auto_fetch)
}

async fn publish(state: &SharedState, fresh: SourceStatus) {
    let previous = {
        let mut statuses = state.statuses.write().await;
        let previous = statuses.get(&fresh.id).cloned();
        statuses.insert(fresh.id.clone(), fresh.clone());
        previous
    };

    let event = if let Some(error) = &fresh.error {
        Some(Event::Failed {
            source: fresh.id.clone(),
            message: error.clone(),
            at: unix_now(),
        })
    } else if previous.as_ref() != Some(&fresh) {
        Some(Event::Changed {
            source: fresh.id.clone(),
            message: fresh
                .units
                .iter()
                .map(|unit| format!("{}: {:?}", unit.name, unit.state))
                .collect::<Vec<_>>()
                .join(", "),
            at: unix_now(),
        })
    } else {
        None
    };
    if let Some(event) = event {
        state.emit(event);
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rw_model::{GitConfig, SourceDefault};

    fn source(tags: &[&str]) -> Source {
        Source {
            id: "s".into(),
            label: None,
            interval_secs: 120,
            imports: vec![],
            notes: String::new(),
            tags: tags.iter().map(|tag| tag.to_string()).collect(),
            vcs: rw_model::VcsConfig::Git(GitConfig { remotes: vec![], checkouts: vec![] }),
        }
    }

    #[test]
    fn tag_defaults_override_type_defaults() {
        let mut defaults = std::collections::BTreeMap::new();
        defaults.insert("type:git".into(), SourceDefault {
            interval_secs: Some(60), auto_fetch: Some(true), token_env: None,
        });
        defaults.insert("tag:work".into(), SourceDefault {
            interval_secs: Some(30), auto_fetch: Some(false), token_env: None,
        });
        assert_eq!(effective_settings(&source(&[]), &defaults), (60, true));
        assert_eq!(effective_settings(&source(&["work"]), &defaults), (30, false));
        assert_eq!(effective_settings(&source(&["home"]), &defaults), (60, true));
    }
}
