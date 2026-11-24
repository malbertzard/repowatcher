mod api;
mod ui;

use leptos::ev::keydown;
use leptos::prelude::*;
use leptos::tachys::view::any_view::IntoAny;
use ui::alert::Alert;
use ui::badge::{Badge, BadgeVariant};
use ui::button::{Button, ButtonVariant};
use ui::card::Card;
use ui::empty::{Empty, EmptyDescription, EmptyTitle};
use ui::input::{Input, InputType};
use ui::label::Label;
use rw_model::{
    Command as ApiCommand, DiffResult, Event, GitCheckout, GitConfig, GitRemote, RemoteRole,
    BackgroundMode, ImportSpec, PluginConfig, RcsConfig, Source, SourceDefault, State, UnitState,
    VcsConfig,
};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <App /> });
}

fn source_core_eq(a: &Source, b: &Source) -> bool {
    a.id == b.id
        && a.label == b.label
        && a.interval_secs == b.interval_secs
        && a.imports == b.imports
        && a.vcs == b.vcs
}

async fn persist_source(
    source: Source,
    original: Option<Source>,
    set_snapshot: WriteSignal<Option<State>>,
    seq: RwSignal<u64>,
) -> Result<(), String> {
    let Ok(state) = api::state().await else {
        return Err("Daemon state is unavailable; cannot save source".into());
    };
    let mut config = state.config;
    match config.sources.iter().position(|item| item.id == source.id) {
        Some(index) => match original {
            Some(original) if source_core_eq(&config.sources[index], &original) => {
                let live = &config.sources[index];
                let mut updated = source;
                updated.notes = live.notes.clone();
                updated.tags = live.tags.clone();
                config.sources[index] = updated;
            }
            Some(_) => {
                return Err("This source changed elsewhere. Your edits were not saved — copy them out, close the editor, and reopen it".into());
            }
            None => return Err(format!("A source with ID '{}' already exists", source.id)),
        },
        None => config.sources.push(source),
    }
    let result = api::command(ApiCommand::SaveConfig { config }).await?;
    if !result.ok {
        return Err(result.error.unwrap_or_else(|| "save failed".into()));
    }
    if let Some(state) = fetch_state(seq).await {
        set_snapshot.set(Some(state));
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Dashboard,
    Sources,
    Activity,
    Remotes,
    Settings,
}

fn title(tab: Tab) -> &'static str {
    match tab {
        Tab::Dashboard => "Overview",
        Tab::Sources => "Sources",
        Tab::Activity => "Activity",
        Tab::Remotes => "Forges",
        Tab::Settings => "Settings",
    }
}

fn subtitle(tab: Tab) -> &'static str {
    match tab {
        Tab::Dashboard => "What needs you, across every repository, fork and file.",
        Tab::Sources => "Add sources and choose the VCS that manages each one.",
        Tab::Activity => "What the background watcher noticed, newest first.",
        Tab::Remotes => "Pull requests and pipeline status from connected forges.",
        Tab::Settings => "Background work, integrations and move to another device.",
    }
}

#[derive(Clone)]
enum PaletteTarget {
    Tab(Tab),
    Source(String),
    AddSource,
    Help,
    Refresh,
}

#[derive(Clone)]
struct PaletteCommand {
    label: String,
    hint: String,
    target: PaletteTarget,
}

struct SyncGuard(RwSignal<u32>);

impl Drop for SyncGuard {
    fn drop(&mut self) {
        self.0.update(|count| *count = count.saturating_sub(1));
    }
}

fn badge(state: &UnitState) -> (BadgeVariant, String) {
    match state {
        UnitState::Clean => (BadgeVariant::Success, "✓ clean".into()),
        UnitState::Dirty => (BadgeVariant::Warning, "● dirty".into()),
        UnitState::Behind { count } => (BadgeVariant::Destructive, format!("↓ −{count} behind")),
        UnitState::Ahead { count } => (BadgeVariant::Info, format!("↑ +{count} ahead")),
        UnitState::Diverged { ahead, behind } => {
            (BadgeVariant::Destructive, format!("↕ +{ahead} / −{behind}"))
        }
        UnitState::Missing => (BadgeVariant::Destructive, "✕ missing".into()),
    }
}

fn watching_text(mode: Option<BackgroundMode>) -> &'static str {
    match mode {
        Some(BackgroundMode::Always) => "Watching continuously",
        Some(BackgroundMode::WhileWindowOpen) => "Watching while the window is open",
        None => "Connecting to the watcher",
    }
}

fn contains_text(haystack: &str, needle: &str) -> bool {
    let needle = needle.trim().to_lowercase();
    needle.is_empty() || haystack.to_lowercase().contains(&needle)
}

fn is_text_entry(target: Option<web_sys::EventTarget>) -> bool {
    target
        .and_then(|target| target.dyn_into::<web_sys::HtmlElement>().ok())
        .map(|element| {
            let tag = element.tag_name().to_ascii_lowercase();
            tag == "input" || tag == "textarea" || tag == "select" || element.is_content_editable()
        })
        .unwrap_or(false)
}

fn trap_focus(event: &web_sys::KeyboardEvent, node: NodeRef<leptos::html::Div>) {
    if event.key() != "Tab" {
        return;
    }
    let Some(element) = node.get() else { return };
    let Ok(element) = element.dyn_into::<web_sys::Element>() else { return };
    let Ok(list) = element.query_selector_all("button, input, textarea, select, a[href]") else { return };
    let first = list.item(0).and_then(|node| node.dyn_into::<web_sys::HtmlElement>().ok());
    let last = list
        .item(list.length().saturating_sub(1))
        .and_then(|node| node.dyn_into::<web_sys::HtmlElement>().ok());
    let active = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.active_element())
        .and_then(|node| node.dyn_into::<web_sys::HtmlElement>().ok());
    if event.shift_key() && active.as_ref() == first.as_ref() {
        if let Some(last) = last {
            let _ = last.focus();
            event.prevent_default();
        }
    } else if !event.shift_key() && active.as_ref() == last.as_ref() {
        if let Some(first) = first {
            let _ = first.focus();
            event.prevent_default();
        }
    }
}

fn repo_hint(value: &str) -> Option<(Option<String>, String)> {
    let value = value.trim().trim_end_matches('/');
    if value.is_empty() || value.starts_with('/') || value.starts_with('.') || value.contains('\\') {
        return None;
    }
    let path = if let Some(index) = value.find("://") {
        value[index + 3..].split_once('/').map(|(_, rest)| rest)?
    } else if let Some((user, rest)) = value.split_once(':') {
        if user.contains('@') { rest } else { return None }
    } else {
        value
    };
    let parts = path.split('/').filter(|part| !part.is_empty()).collect::<Vec<_>>();
    if parts.len() < 2 {
        return None;
    }
    let last = parts[parts.len() - 1];
    let repo = last.strip_suffix(".git").unwrap_or(last);
    if repo.is_empty() {
        return None;
    }
    Some((Some(parts[parts.len() - 2].to_owned()), repo.to_owned()))
}

fn clone_target(owner: Option<&str>, repo: &str, root: &str, template: &str) -> String {
    let template = match owner {
        Some(owner) => template.replace("{owner}", owner),
        None => template.replace("{owner}/", "").replace("{owner}", ""),
    };
    let folder = template
        .replace("{repo}", repo)
        .split('/')
        .filter(|part| !part.is_empty() && *part != "." && *part != "..")
        .collect::<Vec<_>>()
        .join("/");
    let folder = if folder.is_empty() { repo } else { &folder };
    let root = root.trim().trim_end_matches('/');
    if root.is_empty() { folder.to_string() } else { format!("{root}/{folder}") }
}

async fn fetch_state(seq: RwSignal<u64>) -> Option<State> {
    let current = seq.get_untracked() + 1;
    seq.set(current);
    let state = api::state().await.ok()?;
    if seq.get_untracked() == current {
        Some(state)
    } else {
        None
    }
}

fn should_apply_save(
    generation: u32,
    current: u32,
    selected: Option<&str>,
    saved_id: &str,
) -> bool {
    generation == current && selected == Some(saved_id)
}

fn now_secs() -> u64 {
    (js_sys::Date::now() / 1000.0) as u64
}

fn relative_age(at: u64, now: u64) -> String {
    let seconds = now.saturating_sub(at);
    if seconds < 60 {
        "last change just now".into()
    } else if seconds < 3600 {
        format!("last change {}m ago", seconds / 60)
    } else if seconds < 86_400 {
        format!("last change {}h ago", seconds / 3600)
    } else {
        format!("last change {}d ago", seconds / 86_400)
    }
}

fn severity(status: Option<&rw_model::SourceStatus>) -> u8 {
    let Some(status) = status else { return 7 };
    if status.error.is_some() {
        return 0;
    }
    status
        .units
        .iter()
        .map(|unit| match unit.state {
            UnitState::Missing => 1,
            UnitState::Diverged { .. } => 2,
            UnitState::Behind { .. } => 3,
            UnitState::Dirty => 4,
            UnitState::Ahead { .. } => 5,
            UnitState::Clean => 6,
        })
        .min()
        .unwrap_or(6)
}

fn attention_text(status: &rw_model::SourceStatus) -> String {
    if status.error.is_some() {
        return "sync error".into();
    }
    for unit in &status.units {
        match unit.state {
            UnitState::Missing => return "missing".into(),
            UnitState::Diverged { ahead, behind } => return format!("diverged +{ahead}/-{behind}"),
            UnitState::Behind { count } => return format!("{count} behind"),
            _ => {}
        }
    }
    "needs attention".into()
}

#[component]
fn Freshness(last_updated: ReadSignal<u64>, clock: ReadSignal<u64>) -> impl IntoView {
    view! {
        <div class="card-fresh">{move || relative_age(last_updated.get(), clock.get())}</div>
    }
}

#[component]
fn App() -> impl IntoView {
    let (tab, set_tab) = signal(Tab::Dashboard);
    let (snapshot, set_snapshot) = signal::<Option<State>>(None);
    let state_seq = RwSignal::new(0u64);
    let (load_failed, set_load_failed) = signal(false);
    let (events, set_events) = signal::<Vec<Event>>(vec![]);
    let source_events = RwSignal::new(HashMap::<String, Vec<Event>>::new());
    let (notice, set_notice) = signal::<Option<(bool, String)>>(None);
    let (diff, set_diff) = signal::<Option<DiffResult>>(None);
    let (providers, set_providers) = signal::<Vec<rw_model::ProviderSnapshot>>(vec![]);
    let provider_seq = RwSignal::new(0u32);
    let (providers_failed, set_providers_failed) = signal(false);
    let (plugin_names, set_plugin_names) = signal::<rw_model::PluginNames>(rw_model::PluginNames { vcs: vec![], ingest: vec![] });
    let (actions, set_actions) = signal::<Vec<rw_model::SourceAction>>(vec![]);
    let (kinds, set_kinds) = signal::<Vec<rw_model::SourceKind>>(vec![]);
    let (kinds_failed, set_kinds_failed) = signal(false);
    let (editing, set_editing) = signal::<Option<Source>>(None);
    let selected_project = RwSignal::new(None::<String>);
    let pending_edits: RwSignal<HashMap<String, (String, Vec<String>)>> = RwSignal::new(HashMap::new());
    Effect::new(move |_| {
        let Some(state) = snapshot.get() else { return };
        let live: HashSet<&str> = state.config.sources.iter().map(|source| source.id.as_str()).collect();
        pending_edits.update(|map| map.retain(|id, _| live.contains(id.as_str())));
    });
    let (drawer_open, set_drawer_open) = signal(false);
    let drawer_ref = NodeRef::<leptos::html::Div>::new();
    let (confirm_delete, set_confirm_delete) = signal::<Option<String>>(None);
    let (palette_open, set_palette_open) = signal(false);
    let (help_open, set_help_open) = signal(false);
    let palette_query = RwSignal::new(String::new());
    let palette_input = NodeRef::<leptos::html::Input>::new();
    let palette_ref = NodeRef::<leptos::html::Div>::new();
    let help_ref = NodeRef::<leptos::html::Div>::new();
    let syncing = RwSignal::new(0u32);
    let (last_updated, set_last_updated) = signal(now_secs());
    let (clock, set_clock) = signal(now_secs());
    let _clock_timer = set_interval(
        move || set_clock.set(now_secs()),
        std::time::Duration::from_secs(30),
    );
    Effect::new(move |_| {
        tab.get();
        set_drawer_open.set(false);
    });
    let close_drawer = move || {
        let opener = selected_project
            .get_untracked()
            .map(|id| format!("Open project {id}"));
        set_drawer_open.set(false);
        if let Some(label) = opener {
            if let Some(document) = web_sys::window().and_then(|window| window.document()) {
                if let Ok(buttons) = document.query_selector_all("button") {
                    for index in 0..buttons.length() {
                        if let Some(node) = buttons.item(index) {
                            if let Ok(element) = node.dyn_into::<web_sys::Element>() {
                                if element.get_attribute("aria-label").as_deref() == Some(label.as_str()) {
                                    if let Ok(button) = element.dyn_into::<web_sys::HtmlElement>() {
                                        let _ = button.focus();
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    };
    {
        let close = close_drawer;
        let open = drawer_open;
        window_event_listener(keydown, move |event: web_sys::KeyboardEvent| {
            let key = event.key();
            if (event.meta_key() || event.ctrl_key()) && key.eq_ignore_ascii_case("k") {
                event.prevent_default();
                set_palette_open.update(|value| *value = !*value);
                set_help_open.set(false);
                return;
            }
            if key == "Escape" {
                if help_open.get_untracked() {
                    set_help_open.set(false);
                } else if palette_open.get_untracked() {
                    set_palette_open.set(false);
                } else if open.get_untracked() {
                    close();
                }
                return;
            }
            if key == "?" && !is_text_entry(event.target()) {
                event.prevent_default();
                set_help_open.update(|value| *value = !*value);
                set_palette_open.set(false);
            }
        });
    }
    Effect::new(move |_| {
        if palette_open.get() {
            palette_query.set(String::new());
            if let Some(input) = palette_input.get() {
                let _ = input.focus();
            }
        }
        if help_open.get() {
            if let Some(element) = help_ref.get() {
                if let Ok(element) = element.dyn_into::<web_sys::Element>() {
                    if let Ok(Some(button)) = element.query_selector("button") {
                        if let Ok(button) = button.dyn_into::<web_sys::HtmlElement>() {
                            let _ = button.focus();
                        }
                    }
                }
            }
        }
    });
    Effect::new(move |_| {
        if !drawer_open.get() {
            return;
        }
        if let Some(element) = drawer_ref.get() {
            if let Ok(element) = element.dyn_into::<web_sys::Element>() {
                if let Ok(Some(button)) = element.query_selector("button") {
                if let Ok(button) = button.dyn_into::<web_sys::HtmlElement>() {
                    let _ = button.focus();
                }
            }
            }
        }
    });

    let load = move || {
        let set_snapshot = set_snapshot;
        let set_load_failed = set_load_failed;
        let set_updated = set_last_updated;
        let seq_signal = state_seq;
        let seq = seq_signal.get_untracked() + 1;
        seq_signal.set(seq);
        set_load_failed.set(false);
        spawn_local(async move {
            match api::state().await {
                Ok(state) if seq == seq_signal.get_untracked() => {
                    set_snapshot.set(Some(state));
                    set_updated.set(now_secs());
                    set_load_failed.set(false);
                }
                Ok(_) => {}
                Err(_) => set_load_failed.set(true),
            }
        });
    };
    load();

    let load_providers = move || {
        let set_providers = set_providers;
        let set_providers_failed = set_providers_failed;
        let seq_signal = provider_seq;
        let seq = seq_signal.get_untracked() + 1;
        seq_signal.set(seq);
        set_providers_failed.set(false);
        spawn_local(async move {
            match api::providers().await {
                Ok(values) if seq == seq_signal.get_untracked() => {
                    set_providers.set(values);
                    set_providers_failed.set(false);
                }
                Ok(_) => {}
                Err(_) => set_providers_failed.set(true),
            }
        });
    };
    {
        let set_actions = set_actions;
        spawn_local(async move {
            if let Ok(values) = api::actions().await { set_actions.set(values); }
        });
    }
    let load_kinds = move || {
        let set_kinds = set_kinds;
        let set_failed = set_kinds_failed;
        set_failed.set(false);
        spawn_local(async move {
            match api::kinds().await {
                Ok(values) => {
                    set_kinds.set(values);
                    set_failed.set(false);
                }
                Err(_) => set_failed.set(true),
            }
        });
    };
    load_kinds();
    {
        let set_plugin_names = set_plugin_names;
        spawn_local(async move {
            if let Ok(names) = api::plugins().await {
                set_plugin_names.set(names);
            }
        });
    }

    let notify = move |is_error: bool, message: String| set_notice.set(Some((is_error, message)));
    Effect::new(move |_| {
        tab.get();
        set_notice.set(None);
    });

    let run = move |command: ApiCommand| {
        set_notice.set(None);
        let set_snapshot = set_snapshot;
        let set_diff = set_diff;
        let set_updated = set_last_updated;
        let seq_signal = state_seq;
        let notify = notify;
        let done = syncing;
        syncing.update(|count| *count += 1);
        spawn_local(async move {
            let _guard = SyncGuard(done);
            match api::command(command).await {
                Ok(result) => {
                    let rw_model::ApiResult { ok, error, diff: patch, .. } = result;
                    match patch {
                        Some(value) => set_diff.set(Some(value)),
                        None if ok => set_diff.set(None),
                        None => {}
                    }
                    if let Some(error) = error {
                        notify(true, error);
                    }
                    let seq = seq_signal.get_untracked() + 1;
                    seq_signal.set(seq);
                    if let Ok(state) = api::state().await {
                        if seq == seq_signal.get_untracked() {
                            set_snapshot.set(Some(state));
                            set_updated.set(now_secs());
                        }
                    }
                }
                Err(error) => notify(true, error),
            }
        });
    };

    let focus_add = move || {
        set_timeout(
            move || {
                if let Some(document) = web_sys::window().and_then(|window| window.document()) {
                    if let Some(element) = document.get_element_by_id("add-location") {
                        if let Ok(input) = element.dyn_into::<web_sys::HtmlInputElement>() {
                            let _ = input.focus();
                        }
                    }
                }
            },
            std::time::Duration::from_millis(0),
        );
    };
    let activate = move |target: PaletteTarget| {
        match target {
            PaletteTarget::Tab(tab) => {
                set_tab.set(tab);
                if tab == Tab::Remotes {
                    load_providers();
                }
            }
            PaletteTarget::Source(id) => {
                selected_project.set(Some(id));
                set_drawer_open.set(true);
            }
            PaletteTarget::AddSource => {
                set_tab.set(Tab::Dashboard);
                focus_add();
            }
            PaletteTarget::Help => set_help_open.set(true),
            PaletteTarget::Refresh => {
                load();
                if tab.get_untracked() == Tab::Remotes {
                    load_providers();
                }
            }
        }
        set_palette_open.set(false);
    };

    let save = move |source: Source, original: Source| {
        let set_snapshot = set_snapshot;
        let set_editing = set_editing;
        let set_notice = set_notice;
        let seq_signal = state_seq;
        let notify = notify;
        spawn_local(async move {
            match persist_source(source, Some(original), set_snapshot, seq_signal).await {
                Ok(()) => {
                    set_notice.set(None);
                    notify(false, "Saved".into());
                    set_editing.set(None);
                }
                Err(error) => notify(true, error),
            }
        });
    };

    let save_config = move |mutate: Box<dyn FnOnce(&mut rw_model::Config) + Send>| {
        set_notice.set(None);
        let set_snapshot = set_snapshot;
        let seq_signal = state_seq;
        let notify = notify;
        spawn_local(async move {
            let Ok(state) = api::state().await else {
                notify(true, "Daemon state is unavailable; cannot save".into());
                return;
            };
            let mut config = state.config;
            mutate(&mut config);
            match api::command(ApiCommand::SaveConfig { config }).await {
                Ok(result) if result.ok => {
                    if let Some(state) = fetch_state(seq_signal).await {
                        set_snapshot.set(Some(state));
                    }
                    notify(false, "Saved".into());
                }
                Ok(result) => notify(true, result.error.unwrap_or_else(|| "save failed".into())),
                Err(error) => notify(true, error),
            }
        });
    };

    let event_snapshot = set_snapshot;
    let event_updated = set_last_updated;
    let event_seq = state_seq;
    let source_history = source_events;
    if let Err(error) = api::on_event(move |event| {
        let history_event = event.clone();
        set_events.update(|list| {
            list.push(event);
            if list.len() > 100 {
                list.remove(0);
            }
        });
        source_history.update(|map| {
            if let Event::Changed { source, .. } | Event::Failed { source, .. } = &history_event {
                let list = map.entry(source.clone()).or_default();
                list.insert(0, history_event);
                list.truncate(25);
            }
        });
        let set_snapshot = event_snapshot;
        let set_updated = event_updated;
        let seq_signal = event_seq;
        let seq = seq_signal.get_untracked() + 1;
        seq_signal.set(seq);
        spawn_local(async move {
            if let Ok(state) = api::state().await {
                if seq == seq_signal.get_untracked() {
                    set_snapshot.set(Some(state));
                    set_updated.set(now_secs());
                }
            }
        });
    }) {
        notify(true, format!("Event stream: {error}"));
    }

    view! {
        <a class="skip-link" href="#main-content">"Skip to content"</a>
        <div class="shell">
            <aside class="sidebar">
                <div class="brand">repo<span>watcher</span></div>
                {move || view! {
                    <nav class="nav" aria-label="Main navigation">
                        <Button variant=if tab.get() == Tab::Dashboard { ButtonVariant::Accent } else { ButtonVariant::Ghost } class="nav-button" attr:aria-current=if tab.get()==Tab::Dashboard { "page" } else { "false" } attr:aria-label=if tab.get()==Tab::Dashboard { "Overview, current tab" } else { "Overview" } on:click=move |_| set_tab.set(Tab::Dashboard)>"Overview"</Button>
                        <Button variant=if tab.get() == Tab::Sources { ButtonVariant::Accent } else { ButtonVariant::Ghost } class="nav-button" attr:aria-current=if tab.get()==Tab::Sources { "page" } else { "false" } attr:aria-label=if tab.get()==Tab::Sources { "Sources, current tab" } else { "Sources" } on:click=move |_| set_tab.set(Tab::Sources)>"Sources"</Button>
                        <Button variant=if tab.get() == Tab::Activity { ButtonVariant::Accent } else { ButtonVariant::Ghost } class="nav-button" attr:aria-current=if tab.get()==Tab::Activity { "page" } else { "false" } attr:aria-label=if tab.get()==Tab::Activity { "Activity, current tab" } else { "Activity" } on:click=move |_| set_tab.set(Tab::Activity)>"Activity"</Button>
                        <Button variant=if tab.get() == Tab::Remotes { ButtonVariant::Accent } else { ButtonVariant::Ghost } class="nav-button" attr:aria-current=if tab.get()==Tab::Remotes { "page" } else { "false" } attr:aria-label=if tab.get()==Tab::Remotes { "Forges, current tab" } else { "Forges" } on:click=move |_| { set_tab.set(Tab::Remotes); load_providers(); }>"Forges"</Button>
                        <Button variant=if tab.get() == Tab::Settings { ButtonVariant::Accent } else { ButtonVariant::Ghost } class="nav-button" attr:aria-current=if tab.get()==Tab::Settings { "page" } else { "false" } attr:aria-label=if tab.get()==Tab::Settings { "Settings, current tab" } else { "Settings" } on:click=move |_| set_tab.set(Tab::Settings)>"Settings"</Button>
                    </nav>
                }}
                <div class="foot">
                    <Button variant=ButtonVariant::Ghost class="help-button" attr:aria-label="Keyboard shortcuts" on:click=move |_| set_help_open.set(true)>"? Keyboard shortcuts"</Button>
                </div>
            </aside>

            <main class="main" id="main-content" tabindex="-1" attr:aria-busy=move || syncing.get() > 0>
                <div class="topbar">
                    <Button variant=ButtonVariant::Secondary class="topbar-search" attr:aria-label="Search or jump to a page or project" on:click=move |_| { set_palette_open.set(true); set_help_open.set(false); }>"⌕ Search or jump to…    ⌘K"</Button>
                    <div class="topbar-right">
                        <span class="health" role="status" aria-live="polite">
                            <span class=move || if snapshot.get().is_some() { "health-dot" } else { "health-dot connecting" } aria-hidden="true"></span>
                            {move || watching_text(snapshot.get().map(|state| state.config.settings.background))}
                        </span>
                        <Button variant=ButtonVariant::Default class="topbar-add" on:click=move |_| { set_tab.set(Tab::Dashboard); focus_add(); }>"+ Add source"</Button>
                    </div>
                </div>
                <div class="page-head">
                    <div>
                        <h1>{move || title(tab.get())}</h1>
                        <p>{move || subtitle(tab.get())}</p>
                    </div>
                    <Button variant=ButtonVariant::Secondary class="header-button" on:click=move |_| { load(); if tab.get() == Tab::Remotes { load_providers(); } }>"Refresh"</Button>
                </div>

                {move || notice.get().map(|(is_error, message)| {
                    view! {
                        <Alert attr:role=if is_error { "alert" } else { "status" } class=if is_error { "border-destructive/50 text-destructive" } else { "border-success/50 text-success" }>
                            {message}
                        </Alert>
                    }.into_any()
                })}

                {move || match tab.get() {
                    Tab::Dashboard => view! {
                        <Dashboard snapshot=snapshot set_snapshot=set_snapshot load_failed=load_failed run=run notify=notify kinds=kinds kinds_failed=kinds_failed load_kinds=load_kinds diff=diff set_diff=set_diff selected_project=selected_project set_drawer_open=set_drawer_open last_updated=last_updated clock=clock state_seq=state_seq />
                    }.into_any(),
                    Tab::Sources => view! {
                        <Sources snapshot=snapshot load_failed=load_failed plugin_names=plugin_names editing=editing set_editing=set_editing save=save save_config=save_config notify=notify confirm_delete=confirm_delete set_confirm_delete=set_confirm_delete selected_project=selected_project set_drawer_open=set_drawer_open />
                    }.into_any(),
                    Tab::Activity => view! { <Activity events=events /> }.into_any(),
                    Tab::Remotes => view! { <Providers snapshots=providers load_failed=providers_failed selected_project=selected_project set_drawer_open=set_drawer_open /> }.into_any(),
                    Tab::Settings => view! { <SettingsView snapshot=snapshot set_snapshot=set_snapshot save_config=save_config notify=notify state_seq=state_seq /> }.into_any(),
                }}
            </main>
        </div>
        {move || if drawer_open.get() && selected_project.get().is_some() {
            view! {
                <div class="drawer-backdrop" on:click=move |event: web_sys::MouseEvent| {
                    if event.target() == event.current_target() {
                        close_drawer();
                    }
                }>
                    <div node_ref=drawer_ref class="drawer" role="dialog" aria-modal="true" aria-label="Project details"
                        on:keydown=move |event: web_sys::KeyboardEvent| trap_focus(&event, drawer_ref)
                    >
                        <Projects snapshot=snapshot set_snapshot=set_snapshot notify=notify actions=actions source_events=source_events selected=selected_project state_seq=state_seq pending_edits=pending_edits on_close=move |_| close_drawer() />
                    </div>
                </div>
            }.into_any()
        } else {
            ().into_any()
        }}
        {move || if palette_open.get() {
            let query = palette_query.get();
            let mut commands = vec![
                PaletteCommand { label: "Overview".into(), hint: "Go to".into(), target: PaletteTarget::Tab(Tab::Dashboard) },
                PaletteCommand { label: "Sources".into(), hint: "Go to".into(), target: PaletteTarget::Tab(Tab::Sources) },
                PaletteCommand { label: "Forges".into(), hint: "Go to".into(), target: PaletteTarget::Tab(Tab::Remotes) },
                PaletteCommand { label: "Activity".into(), hint: "Go to".into(), target: PaletteTarget::Tab(Tab::Activity) },
                PaletteCommand { label: "Settings".into(), hint: "Go to".into(), target: PaletteTarget::Tab(Tab::Settings) },
                PaletteCommand { label: "Add source".into(), hint: "Quick import".into(), target: PaletteTarget::AddSource },
                PaletteCommand { label: "Refresh status".into(), hint: "Check now".into(), target: PaletteTarget::Refresh },
                PaletteCommand { label: "Keyboard shortcuts".into(), hint: "Help".into(), target: PaletteTarget::Help },
            ];
            if let Some(state) = snapshot.get() {
                for source in &state.config.sources {
                    commands.push(PaletteCommand {
                        label: source.label.clone().unwrap_or_else(|| source.id.clone()),
                        hint: format!("Project · {}", source.id),
                        target: PaletteTarget::Source(source.id.clone()),
                    });
                }
            }
            commands.retain(|command| contains_text(&command.label, &query) || contains_text(&command.hint, &query));
            let first = commands.first().map(|command| command.target.clone());
            view! {
                <div class="modal-backdrop modal-backdrop--top" on:click=move |event: web_sys::MouseEvent| {
                    if event.target() == event.current_target() { set_palette_open.set(false); }
                }>
                    <div node_ref=palette_ref class="palette" role="dialog" aria-modal="true" aria-label="Command palette" on:keydown=move |event: web_sys::KeyboardEvent| trap_focus(&event, palette_ref)>
                        <div class="palette-search" on:keydown=move |event: web_sys::KeyboardEvent| {
                            if event.key() != "Enter" { return; }
                            let from_input = event.target().and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok()).is_some();
                            if from_input {
                                if let Some(target) = first.clone() { activate(target); }
                            }
                        }>
                            <Input node_ref=palette_input bind_value=palette_query placeholder="Search pages and projects…" attr:aria-label="Search commands" />
                        </div>
                        <div class="palette-list">
                            {commands.into_iter().map(|command| {
                                let activate = activate;
                                let label = command.label;
                                let hint = command.hint;
                                let target = command.target;
                                view! {
                                    <Button variant=ButtonVariant::Ghost class="palette-item" on:click=move |_| activate(target.clone())>
                                        <span>{label.clone()}</span>
                                        <span class="sub">{hint.clone()}</span>
                                    </Button>
                                }
                            }).collect_view()}
                        </div>
                        <div class="palette-foot">"Type to filter · Enter opens the first match · Esc closes"</div>
                    </div>
                </div>
            }.into_any()
        } else {
            ().into_any()
        }}
        {move || if help_open.get() {
            view! {
                <div class="modal-backdrop modal-backdrop--top" on:click=move |event: web_sys::MouseEvent| {
                    if event.target() == event.current_target() { set_help_open.set(false); }
                }>
                    <div node_ref=help_ref class="palette help" role="dialog" aria-modal="true" aria-label="Keyboard shortcuts" on:keydown=move |event: web_sys::KeyboardEvent| trap_focus(&event, help_ref)>
                        <div class="drawer-head">
                            <h2>"Keyboard shortcuts"</h2>
                            <Button variant=ButtonVariant::Ghost attr:aria-label="Close keyboard shortcuts" on:click=move |_| set_help_open.set(false)>"×"</Button>
                        </div>
                        <dl class="shortcuts">
                            <dt>"⌘K / Ctrl+K"</dt><dd>"Open command palette"</dd>
                            <dt>"?"</dt><dd>"Show this cheat sheet"</dd>
                            <dt>"Esc"</dt><dd>"Close a dialog or drawer"</dd>
                            <dt>"Enter"</dt><dd>"Add a source from the URL field, or open the first palette match"</dd>
                        </dl>
                    </div>
                </div>
            }.into_any()
        } else {
            ().into_any()
        }}
    }
}

#[component]
fn AddWizard(
    kinds: ReadSignal<Vec<rw_model::SourceKind>>,
    kinds_failed: ReadSignal<bool>,
    load_kinds: impl Fn() + Copy + Send + Sync + 'static,
    set_snapshot: WriteSignal<Option<State>>,
    notify: impl Fn(bool, String) + Copy + Send + Sync + 'static,
    state_seq: RwSignal<u64>,
    snapshot: ReadSignal<Option<State>>,
    on_added: impl Fn(String) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let chosen = RwSignal::new(Some(rw_model::SourceKind {
        name: "git-ingest".into(),
        label: "Git repository".into(),
        description: "Clone a repository from a URL and watch it.".into(),
        input_label: "Git URL".into(),
        mode: rw_model::SourceKindMode::Import,
    }));
    let location = RwSignal::new(String::new());
    let preview = RwSignal::new(None::<rw_model::ImportPreview>);
    let busy = RwSignal::new(false);
    let add_error = RwSignal::new(None::<String>);
    let inference = move || {
        if !chosen.get().as_ref().map(|kind| kind.name.contains("git")).unwrap_or(false) {
            return None;
        }
        let value = location.get();
        let (owner, repo) = repo_hint(&value)?;
        let settings = snapshot.get().map(|state| state.config.settings);
        let root = settings.as_ref().map(|item| item.import_root.to_string_lossy().into_owned()).unwrap_or_default();
        let template = settings.as_ref().map(|item| item.folder_template.clone()).unwrap_or_else(|| "{owner}/{repo}".into());
        let name = match &owner {
            Some(owner) => format!("{owner}/{repo}"),
            None => repo.clone(),
        };
        Some((name, clone_target(owner.as_deref(), &repo, &root, &template)))
    };

    let add_source = move || {
        let Some(kind) = chosen.get() else { return };
        let value = location.get().trim().to_owned();
        if value.is_empty() || busy.get() { return; }
        let set_busy = busy;
        let set_preview = preview;
        let set_error = add_error;
        let notify = notify;
        let set_snapshot = set_snapshot;
        let before: HashSet<String> = snapshot
            .get_untracked()
            .map(|state| state.config.sources.iter().map(|source| source.id.clone()).collect())
            .unwrap_or_default();
        let on_added = on_added;
        set_busy.set(true);
        set_error.set(None);
        spawn_local(async move {
            let result = if kind.mode == rw_model::SourceKindMode::Import {
                api::quick_import(&value, None, Some(kind.name.clone())).await
            } else {
                let id = PathBuf::from(&value)
                    .file_stem()
                    .map(|name| name.to_string_lossy().into_owned())
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| kind.name.clone());
                let vcs = if kind.name == "rcs" {
                    VcsConfig::Rcs(RcsConfig { file: value.clone().into(), rcs_dir: None })
                } else {
                    VcsConfig::Plugin(PluginConfig {
                        plugin: kind.name.clone(),
                        options: serde_json::Value::Null,
                        paths: value.split(',').map(str::trim).filter(|part| !part.is_empty()).map(PathBuf::from).collect(),
                    })
                };
                match persist_source(
                    Source {
                        id,
                        label: None,
                        interval_secs: 120,
                        imports: vec![],
                        notes: String::new(),
                        tags: vec![],
                        vcs,
                    },
                    None,
                    set_snapshot,
                    state_seq,
                )
                .await
                {
                    Ok(()) => Ok(rw_model::ApiResult::ok()),
                    Err(error) => Err(error),
                }
            };
            match result {
                Ok(result) if result.ok => {
                    if let Some(state) = fetch_state(state_seq).await {
                        if let Some(id) = state.config.sources.iter().find(|source| !before.contains(&source.id)) {
                            on_added(id.id.clone());
                        }
                        set_snapshot.set(Some(state));
                    }
                    set_preview.set(None);
                    set_error.set(None);
                    location.set(String::new());
                    notify(false, "Source added".into());
                }
                Ok(result) => {
                    let message = result.error.unwrap_or_else(|| "add failed".into());
                    set_error.set(Some(message.clone()));
                    notify(true, message);
                }
                Err(error) => {
                    set_error.set(Some(error.clone()));
                    notify(true, error);
                }
            }
            set_busy.set(false);
        });
    };

    view! {
        <Card>
            <div class="subhead"><h2>"Add a source"</h2></div>
            {move || if snapshot.get().map(|state| state.config.sources.is_empty()).unwrap_or(false) { view! {
                <div class="onboarding">
                    <span>"1. Paste a location → 2. We infer the rest → 3. It watches in the background."</span>
                    <Button variant=ButtonVariant::Ghost on:click=move |_| {
                        location.set("https://github.com/leptos-rs/leptos.git".into());
                        add_error.set(None);
                    }>"Try a public repo"</Button>
                </div>
            }.into_any() } else { ().into_any() }}
            <p class="sub">"Paste the location and click Add. Everything else is inferred from settings."</p>
            {move || if kinds_failed.get() {
                view! {
                    <div>
                        <Alert attr:role="alert" class="border-destructive/50 text-destructive">"Could not load source types."</Alert>
                        <Button variant=ButtonVariant::Secondary on:click=move |_| load_kinds()>"Retry"</Button>
                    </div>
                }.into_any()
            } else {
                chosen.get().map(|kind| {
                let placeholder = kind.input_label.clone();
                view! {
                    <details>
                        <summary>"Source type: " {kind.label.clone()} " (change)"</summary>
                        {if kinds.get().is_empty() {
                            view! { <div class="sub">"Loading source types…"</div> }.into_any()
                        } else {
                            view! {
                                <div class="actions">
                                    {kinds.get().into_iter().map(|item| {
                                        let current = item.clone();
                                        view! {
                                            <Button variant=ButtonVariant::Secondary on:click=move |_| {
                                                chosen.set(Some(current.clone()));
                                                location.set(String::new());
                                                preview.set(None);
                                            }>
                                                {item.label.clone()}
                                            </Button>
                                        }
                                    }).collect_view()}
                                </div>
                            }.into_any()
                        }}
                    </details>
                    <div class="form-field" style="margin-top:10px" on:input=move |_| { preview.set(None); add_error.set(None); } on:keydown=move |ev| {
                        let from_input = ev.target().and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok()).is_some();
                        if ev.key() == "Enter" && from_input { add_source(); }
                    }>
                        <Label html_for="add-location">{kind.input_label.clone()}</Label>
                        <Input id="add-location" bind_value=location placeholder=placeholder.clone() />
                    </div>
                    {move || inference().map(|(name, target)| view! {
                        <div class="inference" role="status">"Git · " {name} " · will clone to " <code>{target}</code></div>
                    })}
                    <div class="actions" style="margin-top:10px">
                        <Button variant=ButtonVariant::Secondary attr:disabled=move || busy.get() || location.get().trim().is_empty() on:click=move |_| add_source()>{move || if busy.get() { "Adding…" } else { "Add source" }}</Button>
                        {if kind.mode == rw_model::SourceKindMode::Import {
                        view! {
                                <Button variant=ButtonVariant::Ghost attr:disabled=move || busy.get() || location.get().trim().is_empty() on:click=move |_| {
                                        let value = location.get();
                                        let set_preview = preview;
                                        let set_busy = busy;
                                        set_busy.set(true);
                                        spawn_local(async move {
                                            match api::preview(&value).await {
                                                Ok(result) if result.ok => set_preview.set(result.preview),
                                                Ok(result) => notify(true, result.error.unwrap_or_else(|| "preview failed".into())),
                                                Err(error) => notify(true, error),
                                            }
                                            set_busy.set(false);
                                        });
                                }>"Preview plan"</Button>
                        }.into_any()
                        } else { ().into_any() }}
                    </div>
                    {move || add_error.get().map(|error| view! {
                        <Alert attr:role="alert" class="border-destructive/50 text-destructive">{error}</Alert>
                    })}
                    {move || preview.get().map(|plan| view! {
                        <div class="sub" role="status">
                            "Detected: " {plan.vcs} " · folder " {plan.folder}
                            {plan.provider.map(|provider| view! { " · " {provider} })}
                        </div>
                    })}
                }
                }).into_any()
            }}
        </Card>
    }
}

#[component]
fn Dashboard(
    snapshot: ReadSignal<Option<State>>,
    set_snapshot: WriteSignal<Option<State>>,
    load_failed: ReadSignal<bool>,
    run: impl Fn(ApiCommand) + Copy + Send + Sync + 'static,
    notify: impl Fn(bool, String) + Copy + Send + Sync + 'static,
    kinds: ReadSignal<Vec<rw_model::SourceKind>>,
    kinds_failed: ReadSignal<bool>,
    load_kinds: impl Fn() + Copy + Send + Sync + 'static,
    diff: ReadSignal<Option<DiffResult>>,
    set_diff: WriteSignal<Option<DiffResult>>,
    selected_project: RwSignal<Option<String>>,
    set_drawer_open: WriteSignal<bool>,
    last_updated: ReadSignal<u64>,
    clock: ReadSignal<u64>,
    state_seq: RwSignal<u64>,
) -> impl IntoView {
    let query = RwSignal::new(String::new());
    let active_tag = RwSignal::new(None::<String>);
    let all_tags = RwSignal::new(Vec::<String>::new());
    let pending_add = RwSignal::new(None::<String>);
    Effect::new(move |_| {
        let Some(id) = pending_add.get() else { return };
        if snapshot
            .get()
            .map(|state| state.statuses.iter().any(|status| status.id == id))
            .unwrap_or(false)
        {
            pending_add.set(None);
        }
    });
    Effect::new(move |_| {
        let mut tags: Vec<String> = snapshot
            .get()
            .map(|state| state.config.sources.iter().flat_map(|source| source.tags.iter().map(|tag| tag.to_lowercase())).collect())
            .unwrap_or_default();
        tags.sort();
        tags.dedup();
        all_tags.set(tags.clone());
        if let Some(active) = active_tag.get() {
            if !tags.contains(&active) {
                active_tag.set(None);
            }
        }
    });

    view! {
        <AddWizard kinds=kinds kinds_failed=kinds_failed load_kinds=load_kinds set_snapshot=set_snapshot notify=notify state_seq=state_seq snapshot=snapshot on_added=move |id| pending_add.set(Some(id)) />
        <Card>
            <div class="subhead"><h2>"Your sources"</h2></div>
            <div class="form-field">
                <Label html_for="project-search">"Search by name, tag or note"</Label>
                <Input id="project-search" r#type=InputType::Search bind_value=query placeholder="Type to filter sources" />
                {move || if query.get().is_empty() { ().into_any() } else {
                    view! { <Button variant=ButtonVariant::Ghost on:click=move |_| query.set(String::new())>"Clear search"</Button> }.into_any()
                }}
            </div>
            {move || {
                let tags: Vec<String> = snapshot
                    .get()
                    .map(|state| {
                        let mut tags = state.config.sources.iter().flat_map(|source| source.tags.iter().map(|tag| tag.to_lowercase())).collect::<Vec<_>>();
                        tags.sort();
                        tags.dedup();
                        tags
                    })
                    .unwrap_or_default();
                if tags.is_empty() {
                    ().into_any()
                } else {
                    view! {
                        <div class="actions" style="margin-top:10px">
                            <Button variant=if active_tag.get().is_none() { ButtonVariant::Secondary } else { ButtonVariant::Ghost } attr:aria-pressed=active_tag.get().is_none() on:click=move |_| active_tag.set(None)>"All"</Button>
                            {tags.into_iter().map(|tag| {
                                let current = tag.clone();
                                view! {
                                    <Button variant=if active_tag.get().as_deref() == Some(current.as_str()) { ButtonVariant::Secondary } else { ButtonVariant::Ghost } attr:aria-pressed=active_tag.get().as_deref() == Some(current.as_str()) on:click=move |_| active_tag.set(Some(current.clone()))>
                                        {tag}
                                    </Button>
                                }
                            }).collect_view()}
                        </div>
                    }.into_any()
                }
            }}
        </Card>
        {move || {
            let Some(state) = snapshot.get() else { return ().into_any() };
            let items: Vec<(String, String)> = state
                .statuses
                .iter()
                .filter(|status| severity(Some(status)) <= 3)
                .map(|status| (status.id.clone(), attention_text(status)))
                .collect();
            if items.is_empty() {
                ().into_any()
            } else {
                view! {
                    <Card class="attention">
                        <div class="subhead" role="status" aria-live="polite">
                            <h2>"Needs attention"</h2>
                            <span class="attention-title">{items.len()} " sources need you"</span>
                        </div>
                        <div class="actions">
                            {items.into_iter().map(|(id, text)| {
                                let open_id = id.clone();
                                let label_id = id.clone();
                                let aria_id = id.clone();
                                view! {
                                    <Button variant=ButtonVariant::Ghost class="attention-item" attr:aria-label=format!("Open project {aria_id}") on:click=move |_| {
                                        selected_project.set(Some(open_id.clone()));
                                        set_drawer_open.set(true);
                                    }>{label_id} " · " {text}</Button>
                                }
                            }).collect_view()}
                        </div>
                    </Card>
                }.into_any()
            }
        }}
        {move || diff.get().map(|value| view! {
            <Card>
                <div class="subhead">
                    <h2>"Diff — " {value.source.clone()} " / " {value.unit.clone()}</h2>
                    <Button variant=ButtonVariant::Ghost on:click=move |_| set_diff.set(None) attr:aria-label="Close diff".to_string()>"Close"</Button>
                </div>
                <pre class="diff">{if value.patch.is_empty() { "(no changes)".to_string() } else { value.patch.clone() }}</pre>
            </Card>
        })}

        {move || if load_failed.get() {
            view! {
                <Alert class="border-destructive/50 text-destructive">
                    "Could not reach the daemon. Start it with "
                    <code>"rw daemon"</code>
                    " and retry from the header."
                </Alert>
            }.into_any()
        } else {
            match snapshot.get() {
                None => view! { <Empty><EmptyTitle>"Connecting to daemon…"</EmptyTitle></Empty> }.into_any(),
                Some(state) if state.config.sources.is_empty() => view! {
                    <Empty><EmptyTitle>"No sources yet — paste a location above and click Add source."</EmptyTitle></Empty>
                }.into_any(),
                Some(state) => {
                    let statuses: HashMap<&str, &rw_model::SourceStatus> = state
                        .statuses
                        .iter()
                        .map(|status| (status.id.as_str(), status))
                        .collect();
                    let mut filtered = state.config.sources.into_iter()
                        .filter(|source| source.matches(&query.get()))
                        .filter(|source| active_tag.get().as_ref().is_none_or(|tag| source.tags.iter().any(|item| item.eq_ignore_ascii_case(tag))))
                        .collect::<Vec<_>>();
                    filtered.sort_by_key(|source| severity(statuses.get(source.id.as_str()).copied()));
                    if filtered.is_empty() {
                        return view! {
                            <Empty>
                                <EmptyTitle>"No sources match these filters."</EmptyTitle>
                                <Button variant=ButtonVariant::Secondary on:click=move |_| { query.set(String::new()); active_tag.set(None); }>"Clear filters"</Button>
                            </Empty>
                        }.into_any();
                    }
                    view! {
                    <div class="cards">
                        {move || pending_add.get().map(|id| view! {
                            <Card>
                                <div class="card-head">
                                    <div>
                                        <h2>{id.clone()}</h2>
                                        <div class="sub">"Taking first snapshot…"</div>
                                    </div>
                                    <Badge variant=BadgeVariant::Info>"adding"</Badge>
                                </div>
                            </Card>
                        })}
                        {filtered.into_iter().map(|source| {
                            let status = statuses.get(source.id.as_str()).copied();
                            let units = status.map(|s| s.units.clone()).unwrap_or_default();
                            let error = status.and_then(|s| s.error.clone());
                            let id = source.id.clone();
                            let has_imports = !source.imports.is_empty();
                            let import_id = id.clone();
                            let label = source.label.clone().unwrap_or_else(|| source.id.clone());
                            let show_id = source.id != label;
                            let import_label = label.clone();
                            let detail_id = id.clone();
                            let note_id = detail_id.clone();
                            let note_preview: String = source.notes.chars().take(80).collect();
                            let tags = source.tags.clone();
                            let vcs = source.vcs_name().to_owned();
                            let can_diff_global = vcs == "git";
                            let run = run;
                            view! {
                                <Card>
                                    <div class="card-head">
                                        <div>
                                            <h2>{label}</h2>
                                            {if show_id { view! { <div class="sub">{id.clone()}</div> }.into_any() } else { ().into_any() }}
                                            <div class="actions">
                                                {tags.into_iter().map(|tag| view! { <Badge variant=BadgeVariant::Muted>{tag}</Badge> }).collect_view()}
                                            </div>
                                            {if note_preview.is_empty() { ().into_any() } else { view! {
                                                <Button variant=ButtonVariant::Ghost attr:aria-label=format!("Open project {}", detail_id.clone()) on:click=move |_| { selected_project.set(Some(note_id.clone())); set_drawer_open.set(true); }>{note_preview.clone()}</Button>
                                            }.into_any() }}
                                        </div>
                                        <div>
                                            <Badge variant=BadgeVariant::Accent>{vcs}</Badge>
                                            <Freshness last_updated=last_updated clock=clock />
                                        </div>
                                    </div>
                                    <div class="units">
                                        {if let Some(error) = error {
                                            let retry_id = id.clone();
                                            view! {
                                                <Alert class="border-destructive/50 text-destructive">
                                                    <strong>"Sync failed: "</strong> {error}
                                                    <Button variant=ButtonVariant::Ghost on:click=move |_| run(ApiCommand::Sync { source: retry_id.clone() } )>"Retry"</Button>
                                                </Alert>
                                            }.into_any()
                                        } else if units.is_empty() {
                                            view! { <div class="sub">"Checking for changes… status will appear shortly."</div> }.into_any()
                                        } else {
                                            view! {
                                                {units.into_iter().map(|unit| {
                                                    let unit_state = unit.state;
                                                    let (variant, text) = badge(&unit_state);
                                                    let unit_name = unit.name.clone();
                                                    let label_name = unit.name.clone();
                                                    let diff_id = id.clone();
                                                    let diff_unit = unit.name.clone();
                                                    let can_diff = can_diff_global;
                                                    let run = run;
                                                    view! {
                                                        <div class="unit">
                                                            <div>
                                                                {if can_diff {
                                                                    view! {
                                                                        <Button
                                                                            variant=ButtonVariant::Ghost
                                                                            attr:aria-label=format!("Diff {}", unit_name.clone())
                                                                            on:click=move |_| run(ApiCommand::Diff {
                                                                                source: diff_id.clone(),
                                                                                unit: Some(diff_unit.clone()),
                                                                            })
                                                                        >
                                                                            {label_name.clone()}
                                                                        </Button>
                                                                    }.into_any()
                                                                } else {
                                                                    view! { <span class="unit-name">{unit_name.clone()}</span> }.into_any()
                                                                }}
                                                                {unit.detail.map(|detail| view! { <div class="sub">{detail}</div> })}
                                                            </div>
                                                            {let action_id = id.clone(); let action_unit = unit.name.clone(); view! {
                                                            <div class="unit-action">
                                                                {match unit_state {
                                                                    UnitState::Missing => view! { <Button variant=ButtonVariant::Ghost on:click=move |_| run(ApiCommand::Sync { source: action_id.clone() } )>"Recreate"</Button> }.into_any(),
                                                                    UnitState::Behind { .. } | UnitState::Diverged { .. } => view! { <Button variant=ButtonVariant::Ghost on:click=move |_| run(ApiCommand::Sync { source: action_id.clone() } )>"Fetch"</Button> }.into_any(),
                                                                    UnitState::Dirty if can_diff => view! { <Button variant=ButtonVariant::Ghost on:click=move |_| run(ApiCommand::Diff { source: action_id.clone(), unit: Some(action_unit.clone()) } )>"Review"</Button> }.into_any(),
                                                                    _ => ().into_any(),
                                                                }}
                                                                <Badge variant=variant>{text}</Badge>
                                                            </div>
                                                            }}
                                                        </div>
                                                    }
                                                }).collect_view()}
                                            }.into_any()
                                        }}
                                    </div>
                                    <div class="card-actions">
                                        {let sync_id = id.clone(); let open_id = id.clone(); view! {
                                            <Button variant=ButtonVariant::Secondary class="card-action" on:click=move |_| run(ApiCommand::Sync { source: sync_id.clone() } )>"Sync"</Button>
                                            <Button variant=ButtonVariant::Ghost class="card-action" on:click=move |_| run(ApiCommand::Open { source: open_id.clone() } )>"Open in editor"</Button>
                                            <Button variant=ButtonVariant::Secondary class="card-action" attr:aria-label=format!("Open project {}", detail_id.clone()) on:click=move |_| { selected_project.set(Some(detail_id.clone())); set_drawer_open.set(true); }>"Open project"</Button>
                                            {if has_imports { view! {
                                                <Button variant=ButtonVariant::Ghost class="card-action" attr:aria-label=format!("Run imports for {import_label}") on:click=move |_| {
                                                    let source_id = import_id.clone();
                                                    spawn_local(async move {
                                                        match api::run_import(&source_id).await {
                                                            Ok(result) if result.ok => {
                                                                if let Some(state) = fetch_state(state_seq).await { set_snapshot.set(Some(state)); }
                                                            }
                                                            Ok(result) => notify(true, result.error.unwrap_or_else(|| "import failed".into())),
                                                            Err(error) => notify(true, error),
                                                        }
                                                    });
                                                }>"Import"</Button>
                                            }.into_any() } else { ().into_any() }}
                                            <div class="sub" style="flex-basis:100%">"Sync refreshes status; it won't move your commits."</div>
                                        }}
                                    </div>
                                </Card>
                            }
                        }).collect_view()}
                    </div>
                    }.into_any()
                }
            }
        }}
    }
}

#[component]
fn Projects(
    snapshot: ReadSignal<Option<State>>,
    set_snapshot: WriteSignal<Option<State>>,
    notify: impl Fn(bool, String) + Copy + Send + Sync + 'static,
    actions: ReadSignal<Vec<rw_model::SourceAction>>,
    source_events: RwSignal<HashMap<String, Vec<Event>>>,
    selected: RwSignal<Option<String>>,
    state_seq: RwSignal<u64>,
    pending_edits: RwSignal<HashMap<String, (String, Vec<String>)>>,
    on_close: impl Fn(()) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let detail_notes = RwSignal::new(String::new());
    let detail_tags = RwSignal::new(Vec::<String>::new());
    let tag_input = RwSignal::new(String::new());
    let all_tags = RwSignal::new(Vec::<String>::new());
    let confirm_action = RwSignal::new(None::<String>);
    let action_busy = RwSignal::new(None::<String>);
    let integration_cards = RwSignal::new(Vec::<rw_model::IntegrationSnapshot>::new());
    let integrations_loaded = RwSignal::new(false);
    let integrations_failed = RwSignal::new(false);
    let integrations_retry = RwSignal::new(0u32);
    let save_status = RwSignal::new(String::new());
    let save_generation = RwSignal::new(0u32);
    let debounce_generation = RwSignal::new(0u32);
    let save_in_flight = RwSignal::new(false);
    let details_dirty = RwSignal::new(false);
    let save_failed = RwSignal::new(false);
    let save_retry = RwSignal::new(0u32);
    let detail_ref = NodeRef::<leptos::html::Div>::new();

    Effect::new(move |_| {
        let Some(id) = selected.get() else { return };
        save_generation.update(|generation| *generation += 1);
        debounce_generation.update(|generation| *generation += 1);
        save_in_flight.set(false);
        details_dirty.set(false);
        save_failed.set(false);
        save_status.set(String::new());
        if let Some((notes, tags)) = pending_edits.get_untracked().get(&id).cloned() {
            detail_notes.set(notes);
            detail_tags.set(tags);
            details_dirty.set(true);
            save_failed.set(true);
            save_status.set("Not saved — use Retry".into());
        } else if let Some(state) = snapshot.get_untracked() {
            if let Some(source) = state.config.sources.iter().find(|source| source.id == id) {
                detail_notes.set(source.notes.clone());
                detail_tags.set(source.tags.iter().map(|tag| tag.to_lowercase()).collect());
            }
        }
        if let Some(element) = detail_ref.get() {
            let _ = element.scroll_into_view();
        }
    });
    Effect::new(move |_| {
        if details_dirty.get() || save_in_flight.get() {
            return;
        }
        let Some(id) = selected.get() else { return };
        if pending_edits.get_untracked().contains_key(&id) {
            return;
        }
        let Some(state) = snapshot.get() else { return };
        if let Some(source) = state.config.sources.iter().find(|source| source.id == id) {
            detail_notes.set(source.notes.clone());
            detail_tags.set(source.tags.iter().map(|tag| tag.to_lowercase()).collect());
        }
    });
    Effect::new(move |_| {
        let Some(id) = selected.get() else {
            integration_cards.set(Vec::new());
            integrations_loaded.set(false);
            return;
        };
        let set_cards = integration_cards;
        let set_loaded = integrations_loaded;
        let set_failed = integrations_failed;
        let current = selected;
        integrations_retry.get();
        set_loaded.set(false);
        set_failed.set(false);
        spawn_local(async move {
            let result = api::integrations(&id).await;
            if current.get_untracked() == Some(id) {
                match result {
                    Ok(result) => {
                        set_cards.set(result.integrations);
                        set_failed.set(false);
                    }
                    Err(_) => {
                        set_cards.set(Vec::new());
                        set_failed.set(true);
                    }
                }
                set_loaded.set(true);
            }
        });
    });
    Effect::new(move |_| {
        let Some(id) = selected.get() else { return };
        if !details_dirty.get() {
            return;
        }
        save_retry.get();
        let notes = detail_notes.get();
        let tags = detail_tags.get();
        let pending_notes = notes.clone();
        let pending_tags = tags.clone();
        debounce_generation.update(|generation| *generation += 1);
        let debounce = debounce_generation.get_untracked();
        set_timeout(
            move || {
                if debounce != debounce_generation.get_untracked() {
                    return;
                }
                save_generation.update(|generation| *generation += 1);
                let generation = save_generation.get_untracked();
                let set_status = save_status;
                let set_dirty = details_dirty;
                let set_snapshot = set_snapshot;
                let set_busy = save_in_flight;
                let set_failed = save_failed;
                let pending = pending_edits;
                let seq_signal = state_seq;
                let current = selected;
                let check_id = id.clone();
                let notify = notify;
                set_status.set("Saving…".into());
                set_busy.set(true);
                spawn_local(async move {
                    let result = api::command(ApiCommand::UpdateProject { id, notes, tags }).await;
                    set_busy.set(false);
                    if should_apply_save(
                        generation,
                        save_generation.get_untracked(),
                        current.get_untracked().as_deref(),
                        &check_id,
                    ) {
                        match result {
                            Ok(result) if result.ok => {
                                let seq = seq_signal.get_untracked() + 1;
                                seq_signal.set(seq);
                                if let Ok(state) = api::state().await {
                                    if seq == seq_signal.get_untracked() {
                                        set_snapshot.set(Some(state));
                                    }
                                }
                                pending.update(|map| {
                                    map.remove(&check_id);
                                });
                                set_failed.set(false);
                                set_status.set("Saved ✓".into());
                                set_dirty.set(false);
                            }
                            Ok(result) => {
                                pending.update(|map| {
                                    map.insert(check_id, (pending_notes, pending_tags));
                                });
                                set_failed.set(true);
                                set_status.set("Not saved — use Retry".into());
                                notify(true, result.error.unwrap_or_else(|| "save failed".into()));
                            }
                            Err(error) => {
                                pending.update(|map| {
                                    map.insert(check_id, (pending_notes, pending_tags));
                                });
                                set_failed.set(true);
                                set_status.set("Not saved — use Retry".into());
                                notify(true, error);
                            }
                        }
                    }
                });
            },
            std::time::Duration::from_millis(450),
        );
    });
    Effect::new(move |_| {
        let mut tags: Vec<String> = snapshot
            .get()
            .map(|state| state.config.sources.iter().flat_map(|source| source.tags.iter().map(|tag| tag.to_lowercase())).collect())
            .unwrap_or_default();
        tags.sort();
        tags.dedup();
        all_tags.set(tags);
    });

    view! {
        <div class="drawer-head">
            <h2>"Project details"</h2>
            <Button variant=ButtonVariant::Ghost attr:aria-label="Close project details" on:click=move |_| on_close(())>"×"</Button>
        </div>
        {move || if selected.get().is_none() {
            view! { <Empty><EmptyTitle>"Select a project above to manage it."</EmptyTitle></Empty> }.into_any()
        } else {
            { selected.get().and_then(|id| snapshot.get_untracked().map(|state| (id, state))).map(|(id, state)| {
            let source = state.config.sources.iter().find(|source| source.id == id).cloned();
            match source {
                None => view! { <Empty><EmptyTitle>"Select a project above to manage it."</EmptyTitle></Empty> }.into_any(),
                Some(source) => {
                    let notes = detail_notes;
                    let tags = detail_tags;
                    let add_tag = move || {
                        let value = tag_input.get().trim().to_lowercase();
                        if value.is_empty() {
                            notify(true, "Tag cannot be empty".into());
                            return;
                        }
                        if tags.get().contains(&value) {
                            notify(true, format!("Tag {value} is already added"));
                            return;
                        }
                                                details_dirty.set(true);
                        tags.update(|list| list.push(value));
                        tag_input.set(String::new());
                    };
                    let source_id = source.id.clone();
                    let integration_source = RwSignal::new(source.id.clone());
                    let label = source.label.clone().unwrap_or_else(|| source.id.clone());
                    let show_id = source_id != label;
                    let vcs = source.vcs_name().to_owned();
                    view! {
                        <div node_ref=detail_ref>
                        <Card>
                            <div class="card-head">
                                <div><h2>{label}</h2>{if show_id { view! { <div class="sub">{source_id.clone()}</div> }.into_any() } else { ().into_any() }}</div>
                                <Button variant=ButtonVariant::Ghost attr:aria-label="Close project details" on:click=move |_| on_close(())>"Close details"</Button>
                            </div>
                            <div class="form-field">
                                <Label html_for="project-notes">"Notes and todos"</Label>
                                <textarea id="project-notes" class="field-input" style="min-height:120px" prop:value=notes on:input=move |ev| { notes.set(event_target_value(&ev)); details_dirty.set(true); } />
                            </div>
                            <div class="form-field" style="margin-top:10px">
                                <Label>"Tags"</Label>
                                <div class="actions">
                                    {move || tags.get().into_iter().enumerate().map(|(index, tag)| {
                                        let value = tag.clone();
                                        let label_value = tag.clone();
                                        view! {
                                            <Button variant=ButtonVariant::Secondary attr:aria-label=format!("Remove tag {label_value}") on:click=move |_| { details_dirty.set(true); tags.update(|list| { list.remove(index); }); }>
                                                {value} " ×"
                                            </Button>
                                        }
                                    }).collect_view()}
                                </div>
                                <div class="row" style="margin-top:6px" on:keydown=move |ev| {
                                    let from_input = ev.target().and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok()).is_some();
                                    if ev.key() == "Enter" && from_input { add_tag(); }
                                }>
                                    <Input bind_value=tag_input placeholder="New tag" attr:aria-label="New tag" />
                                    <Button variant=ButtonVariant::Secondary on:click=move |_| add_tag()>"Add tag"</Button>
                                </div>
                                <div class="actions" style="margin-top:6px">
                                    {move || {
                                        let current = tags.get();
                                        let suggestions: Vec<String> = all_tags.get().into_iter()
                                            .filter(|tag| !current.contains(tag))
                                            .take(12)
                                            .collect();
                                        if suggestions.is_empty() {
                                            ().into_any()
                                        } else {
                                            view! {
                                                <span class="sub">"Suggested:"</span>
                                                {suggestions.into_iter().map(|tag| {
                                                    let value = tag.clone();
                                                    view! { <Button variant=ButtonVariant::Ghost on:click=move |_| { details_dirty.set(true); tags.update(|list| list.push(value.to_lowercase())); }>{tag}</Button> }
                                                }).collect_view()}
                                            }.into_any()
                                        }
                                    }}
                                </div>
                            </div>
                            <div class="subhead"><h3>"Integrations"</h3></div>
                            {move || {
                                let cards = integration_cards.get();
                                if cards.is_empty() {
                                    if !integrations_loaded.get() {
                                        return view! { <div class="sub">"Checking for pull requests and pipelines…"</div> }.into_any();
                                    }
                                    if integrations_failed.get() {
                                        return view! {
                                            <div>
                                                <div class="sub">"Integrations are unavailable right now. Try again."</div>
                                                <Button variant=ButtonVariant::Ghost on:click=move |_| integrations_retry.update(|value| *value += 1)>"Retry"</Button>
                                            </div>
                                        }.into_any();
                                    }
                                    return view! { <div class="sub">"No integrations are connected to this project yet."</div> }.into_any();
                                }
                                view! {
                                    {cards.into_iter().map(|card| {
                                        let plugin = card.plugin.clone();
                                        let notify = notify;
                                        view! {
                                            <div class="form-field" style="margin-bottom:10px">
                                                <strong>{card.title.clone()}</strong>
                                                {match card.error.clone() {
                                                    Some(error) if error.contains("no forge remotes") => view! {
                                                        <div class="sub">"No forge is connected to this project yet. Add one on the Sources page under Remotes → Provider."</div>
                                                    }.into_any(),
                                                    Some(error) => view! { <div class="sub">{error}</div> }.into_any(),
                                                    None => ().into_any(),
                                                }}
                                                {if card.items.is_empty() && card.error.is_none() {
                                                    view! { <div class="sub">"No open pull requests or recent pipeline runs for this project."</div> }.into_any()
                                                } else { ().into_any() }}
                                                {card.items.into_iter().map(|item| {
                                                    let action = item.action.clone();
                                                    let action_label = item
                                                        .action_label
                                                        .clone()
                                                        .unwrap_or_else(|| "Run".into());
                                                    let value = item.id.clone();
                                                    let item_title = item.title.clone();
                                                    let button_label = action_label.clone();
                                                    let plugin = plugin.clone();
                                                    let notify = notify;
                                                    view! {
                                                        <div class="unit">
                                                            <div>
                                                                {match item.url.clone() {
                                                                    Some(url) => view! { <a href=url target="_blank" rel="noopener">{item.title.clone()}</a> }.into_any(),
                                                                    None => view! { <span>{item.title.clone()}</span> }.into_any(),
                                                                }}
                                                                {item.subtitle.map(|sub| view! { <div class="sub">{sub}</div> })}
                                                                {item.state.map(|state| view! { <Badge variant=BadgeVariant::Muted>{state}</Badge> })}
                                                            </div>
                                                            {match action {
                                                                Some(action) => view! {
                                                                    <Button variant=ButtonVariant::Secondary attr:aria-label=format!("{}: {}", button_label.clone(), item_title) on:click=move |_| {
                                                                        let source_id = integration_source.get();
                                                                        let plugin = plugin.clone();
                                                                        let action = action.clone();
                                                                        let value = value.clone();
                                                                        let notify = notify;
                                                                        spawn_local(async move {
                                                                            match api::integration_action(&source_id, &plugin, &action, &value).await {
                                                                                Ok(result) if result.ok => notify(false, result.message.unwrap_or_else(|| "action finished".into())),
                                                                                Ok(result) => notify(true, result.error.unwrap_or_else(|| "action failed".into())),
                                                                                Err(error) => notify(true, error),
                                                                            }
                                                                        });
                                                                    }>{action_label}</Button>
                                                                }.into_any(),
                                                                None => ().into_any(),
                                                            }}
                                                        </div>
                                                    }
                                                }).collect_view()}
                                            </div>
                                        }
                                    }).collect_view()}
                                }.into_any()
                            }}
                            <div class="subhead"><h3>"Actions"</h3></div>
                            <div class="actions">
                                {actions.get().into_iter()
                                    .filter(|action| action.vcs.iter().any(|name| name == "*" || name == vcs.as_str()))
                                    .map(|action| {
                                        let name = action.name.clone();
                                        let label = action.label.clone();
                                        let description = action.description.clone();
                                        let source_id = source_id.clone();
                                        let notify = notify;
                                        let set_snapshot = set_snapshot;
                                        let confirm_name = name.clone();
                                        let busy_name = name.clone();
                                        let label_name = label.clone();
                                        let label_confirm_name = name.clone();
                                        view! {
                                            <div class="form-field">
                                                <Button variant=ButtonVariant::Secondary
                                                    attr:disabled=move || action_busy.get().as_deref() == Some(busy_name.as_str())
                                                    on:click=move |_| {
                                                        if action_busy.get().is_some() { return; }
                                                        if confirm_action.get().as_deref() == Some(confirm_name.as_str()) {
                                                            confirm_action.set(None);
                                                            let id = source_id.clone();
                                                            let action_name = confirm_name.clone();
                                                            let notify = notify;
                                                            let set_snapshot = set_snapshot;
                                                            action_busy.set(Some(action_name.clone()));
                                                            spawn_local(async move {
                                                                match api::run_action(&id, &action_name).await {
                                                                    Ok(result) if result.ok => {
                                                                        if let Some(state) = fetch_state(state_seq).await { set_snapshot.set(Some(state)); }
                                                                        notify(false, result.message.unwrap_or_else(|| format!("{action_name} finished")));
                                                                    }
                                                                    Ok(result) => notify(true, result.error.unwrap_or_else(|| format!("{action_name} failed"))),
                                                                    Err(error) => notify(true, error),
                                                                }
                                                                action_busy.set(None);
                                                            });
                                                        } else {
                                                            confirm_action.set(Some(confirm_name.clone()));
                                                        }
                                                    }
                                                >
                                                    {move || if confirm_action.get().as_deref() == Some(label_confirm_name.as_str()) {
                                                        format!("Confirm {}", label_name)
                                                    } else {
                                                        label_name.clone()
                                                    }}
                                                </Button>
                                                <span class="sub">{description}</span>
                                            </div>
                                        }
                                    }).collect_view()}
                            </div>
                            <div class="subhead"><h3>"History"</h3></div>
                            {move || {
                                let id = selected.get().unwrap_or_default();
                                let history: Vec<(String, u64)> = source_events
                                    .get()
                                    .get(&id)
                                    .cloned()
                                    .unwrap_or_default()
                                    .into_iter()
                                    .filter_map(|event| match event {
                                        Event::Changed { source, message, at } if source == id => {
                                            Some((message, at))
                                        }
                                        Event::Failed { source, message, at } if source == id => {
                                            Some((message, at))
                                        }
                                        _ => None,
                                    })
                                    .take(8)
                                    .collect();
                                if history.is_empty() {
                                    view! { <div class="sub">"No recent changes for this source."</div> }.into_any()
                                } else {
                                    view! {
                                        <div class="events">
                                            {history.into_iter().map(|(message, at)| view! {
                                                <div class="event">
                                                    <time datetime=iso_timestamp(at)>{format_timestamp(at)}</time>
                                                    <span class="message">{message}</span>
                                                </div>
                                            }).collect_view()}
                                        </div>
                                    }.into_any()
                                }
                            }}
                            <div class="actions" style="margin-top:12px">
                                <span class="sub" role="status" aria-live="polite">{move || save_status.get()}</span>
                                {move || if save_failed.get() {
                                    view! {
                                        <Button variant=ButtonVariant::Ghost on:click=move |_| {
                                            save_failed.set(false);
                                            save_retry.update(|generation| *generation += 1);
                                        }>"Retry save"</Button>
                                    }.into_any()
                                } else {
                                    ().into_any()
                                }}
                            </div>
                        </Card>
                        </div>
                    }.into_any()
                }
            }
        }).unwrap_or_else(|| view! {}.into_any())}
        }.into_any()}
    }
}

#[component]
fn Sources(
    snapshot: ReadSignal<Option<State>>,
    load_failed: ReadSignal<bool>,
    plugin_names: ReadSignal<rw_model::PluginNames>,
    editing: ReadSignal<Option<Source>>,
    set_editing: WriteSignal<Option<Source>>,
    save: impl Fn(Source, Source) + Copy + Send + Sync + 'static,
    save_config: impl Fn(Box<dyn FnOnce(&mut rw_model::Config) + Send>) + Copy + Send + Sync + 'static,
    notify: impl Fn(bool, String) + Copy + Send + Sync + 'static,
    confirm_delete: ReadSignal<Option<String>>,
    set_confirm_delete: WriteSignal<Option<String>>,
    selected_project: RwSignal<Option<String>>,
    set_drawer_open: WriteSignal<bool>,
) -> impl IntoView {
    let add_git = move || {
        set_editing.set(Some(Source {
            id: String::new(),
            label: None,
            interval_secs: 120,
            imports: vec![],
            notes: String::new(),
            tags: vec![],
            vcs: VcsConfig::Git(GitConfig { remotes: vec![], checkouts: vec![] }),
        }));
    };
    let add_rcs = move || {
        set_editing.set(Some(Source {
            id: String::new(),
            label: None,
            interval_secs: 120,
            imports: vec![],
            notes: String::new(),
            tags: vec![],
            vcs: VcsConfig::Rcs(RcsConfig { file: Default::default(), rcs_dir: None }),
        }));
    };
    let add_plugin = move || {
        set_editing.set(Some(Source {
            id: String::new(),
            label: None,
            interval_secs: 120,
            imports: vec![],
            notes: String::new(),
            tags: vec![],
            vcs: VcsConfig::Plugin(PluginConfig {
                plugin: "plain-file".into(),
                options: serde_json::Value::Null,
                paths: vec![],
            }),
        }));
    };

    view! {
        <Card>
            <div class="subhead">
                <h2>"Configured sources"</h2>
                <div class="actions">
                    <Button variant=ButtonVariant::Secondary on:click=move |_| add_git()>"+ Git source"</Button>
                    <Button variant=ButtonVariant::Secondary on:click=move |_| add_rcs()>"+ RCS source"</Button>
                    <Button variant=ButtonVariant::Secondary on:click=move |_| add_plugin()>"+ Plugin source"</Button>
                </div>
            </div>
            <p class="sub">"Available VCS plugins: " {move || plugin_names.get().vcs.into_iter().map(|name| if name == "plain-file" { "plain file".to_owned() } else { name }).collect::<Vec<_>>().join(", ")} " · Ingest systems: " {move || plugin_names.get().ingest.join(", ")}</p>
            {move || if load_failed.get() {
                view! { <Alert class="border-destructive/50 text-destructive">"Could not reach the daemon. Use Refresh to retry."</Alert> }.into_any()
            } else {
                match snapshot.get() {
                None => view! { <div class="sub">"Loading…"</div> }.into_any(),
                Some(state) if state.config.sources.is_empty() => view! {
                    <Empty><EmptyTitle>"Nothing configured yet."</EmptyTitle></Empty>
                }.into_any(),
                Some(state) => view! {
                    {state.config.sources.into_iter().map(|source| {
                        let id = source.id.clone();
                        let label = source.label.clone().unwrap_or_else(|| id.clone());
                        let exportable = source.can_export();
                        let vcs = source.vcs_name().to_owned();
                        let is_confirming = confirm_delete.get().as_deref() == Some(id.as_str());
                        view! {
                            <div class="source-item">
                                <div>
                                    <strong>{label.clone()}</strong>
                                    <div class="sub">{id.clone()} " · " {vcs}
                                        {if exportable { view! { <Badge variant=BadgeVariant::Success>"exportable"</Badge> }.into_any() } else { ().into_any() }}
                                    </div>
                                </div>
                                <div class="actions">
                                    {let edit_id = id.clone(); let delete_id = id.clone(); let project_id = id.clone(); view! {
                                        <Button variant=ButtonVariant::Ghost on:click=move |_| {
                                            if let Some(state) = snapshot.get() {
                                                if let Some(found) = state.config.sources.iter().find(|s| s.id == edit_id) {
                                                    set_editing.set(Some(found.clone()));
                                                }
                                            }
                                        }>"Edit"</Button>
                                        <Button variant=ButtonVariant::Secondary attr:aria-label=format!("Open project {}", project_id.clone()) on:click=move |_| {
                                            selected_project.set(Some(project_id.clone()));
                                            set_drawer_open.set(true);
                                        }>"Open project"</Button>
                                        <Button
                                            variant=ButtonVariant::Destructive
                                            attr:aria-label=if is_confirming { format!("Confirm delete of source {delete_id}") } else { format!("Delete source {delete_id}") }
                                            on:click=move |_| {
                                                if is_confirming {
                                                    if let Some(mut state) = snapshot.get() {
                                                        state.config.sources.retain(|s| s.id != delete_id);
                                                        let source_id = delete_id.clone();
                                                        save_config(Box::new(move |config: &mut rw_model::Config| {
                                                            config.sources.retain(|source| source.id != source_id);
                                                        }));
                                                    }
                                                    set_confirm_delete.set(None);
                                                } else {
                                                    set_confirm_delete.set(Some(delete_id.clone()));
                                                }
                                            }
                                        >
                                            {if is_confirming { "Confirm delete" } else { "Delete" }}
                                        </Button>
                                    }}
                                </div>
                            </div>
                        }
                    }).collect_view()}
                }.into_any(),
                }
            }}

            {move || editing.get().map(|source| {
                view! {
                    <div class="editor-panel">
                        <div class="subhead"><h2>"Edit source"</h2></div>
                        <SourceForm source=source ingest_names=plugin_names.get().ingest on_save=save on_error=notify on_cancel=move |_| set_editing.set(None) />
                    </div>
                }
            })}
        </Card>
    }
}

#[component]
fn SourceForm(
    source: Source,
    ingest_names: Vec<String>,
    on_save: impl Fn(Source, Source) + Send + Sync + 'static,
    on_error: impl Fn(bool, String) + Send + Sync + 'static,
    on_cancel: impl Fn(()) + Send + Sync + 'static,
) -> impl IntoView {
    let is_new = source.id.is_empty();
    let is_git = matches!(source.vcs, VcsConfig::Git(_));
    let original = source.clone();
    let id = RwSignal::new(source.id.clone());
    let label = RwSignal::new(source.label.clone().unwrap_or_default());
    let interval = RwSignal::new(source.interval_secs.to_string());
    let mut git_config = GitConfig { remotes: vec![], checkouts: vec![] };
    let mut rcs_file = String::new();
    let mut rcs_dir = String::new();
    let mut plugin_name = String::new();
    let mut plugin_paths = String::new();
    let mut plugin_options = String::from("{}");
    match &source.vcs {
        VcsConfig::Git(git) => git_config = git.clone(),
        VcsConfig::Rcs(rcs) => {
            rcs_file = rcs.file.to_string_lossy().to_string();
            rcs_dir = rcs.rcs_dir.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        }
        VcsConfig::Plugin(plugin) => {
            plugin_name = plugin.plugin.clone();
            plugin_paths = plugin.paths.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>().join(", ");
            plugin_options = serde_json::to_string_pretty(&plugin.options).unwrap_or_else(|_| "{}".into());
        }
    }
    let (config, set_config) = signal(git_config);
    let file = RwSignal::new(rcs_file);
    let rcs_dir = RwSignal::new(rcs_dir);
    let plugin_name = RwSignal::new(plugin_name);
    let plugin_paths = RwSignal::new(plugin_paths);
    let plugin_options = RwSignal::new(plugin_options);
    let imports = RwSignal::new(source.imports.clone());
    let notes = source.notes.clone();
    let tags = source.tags.clone();
    let kind = source.vcs.clone();

    let vcs_fields = match kind {
        VcsConfig::Git(_) => {
            view! {
                <div class="subhead">
                    <h3>"Remotes — upstream and forks"</h3>
                    <Button variant=ButtonVariant::Secondary on:click=move |_| set_config.update(|c| {
                        c.remotes.push(GitRemote { name: String::new(), role: RemoteRole::Fork, url: String::new(), provider: None });
                    })>"+ Remote"</Button>
                </div>
                <p class="sub">"Mark one remote as upstream; the rest are forks. Provider repo is owner/name and is used for pull requests."</p>
                <For
                    each=move || config.with(|c| c.remotes.iter().cloned().enumerate().collect::<Vec<_>>())
                    key=|item| item.0
                    let((index, remote))
                >
                {
                    let name = remote.name.clone();
                    let name_for_input = name.clone();
                    let url = remote.url.clone();
                    let is_upstream = remote.role == RemoteRole::Upstream;
                    let provider_kind = match remote.provider.as_ref().map(|p| p.kind) {
                        Some(rw_model::ProviderKind::Github) => "github",
                        Some(rw_model::ProviderKind::Gitlab) => "gitlab",
                        Some(rw_model::ProviderKind::Codeberg) => "codeberg",
                        None => "none",
                    };
                    let (provider_kind_value, set_provider_kind_value) = signal(provider_kind.to_string());
                    let provider_repo = remote.provider.as_ref().map(|p| p.repo.clone()).unwrap_or_default();
                    view! {
                        <div class="row">
                            <label class="form-field">"Remote name"
                                <input class="field-input" aria-label=format!("Remote {index} name") prop:value=name_for_input on:input=move |ev| {
                                    let value = event_target_value(&ev);
                                    set_config.update(|c| c.remotes[index].name = value);
                                } />
                            </label>
                            <label class="form-field">"URL"
                                <input class="field-input" aria-label=format!("Remote {index} URL") prop:value=url on:input=move |ev| {
                                    let value = event_target_value(&ev);
                                    set_config.update(|c| c.remotes[index].url = value);
                                } />
                            </label>
                            <label class="form-field">
                                "Role"
                                <select class="field-input" aria-label=format!("Remote {index} role") on:change=move |ev| {
                                    let role = if event_target_value(&ev) == "upstream" { RemoteRole::Upstream } else { RemoteRole::Fork };
                                    set_config.update(|c| c.remotes[index].role = role);
                                }>
                                    <option value="fork" selected=!is_upstream>"fork"</option>
                                    <option value="upstream" selected=is_upstream>"upstream"</option>
                                </select>
                            </label>
                            <label class="form-field">
                                "Provider"
                                <select class="field-input" prop:value=provider_kind_value aria-label=format!("Remote {index} provider") on:change=move |ev| {
                                    let value = event_target_value(&ev);
                                    set_provider_kind_value.set(value.clone());
                                    set_config.update(|c| {
                                        let repo = c.remotes[index].provider.as_ref().map(|p| p.repo.clone()).unwrap_or_default();
                                        c.remotes[index].provider = match value.as_str() {
                                            "github" => Some(rw_model::ProviderRef { kind: rw_model::ProviderKind::Github, repo, token_env: None }),
                                            "gitlab" => Some(rw_model::ProviderRef { kind: rw_model::ProviderKind::Gitlab, repo, token_env: None }),
                                            "codeberg" => Some(rw_model::ProviderRef { kind: rw_model::ProviderKind::Codeberg, repo, token_env: None }),
                                            _ => None,
                                        };
                                    });
                                }>
                                    <option value="none" selected=provider_kind == "none">"none"</option>
                                    <option value="github" selected=provider_kind == "github">"github"</option>
                                    <option value="gitlab" selected=provider_kind == "gitlab">"gitlab"</option>
                                    <option value="codeberg" selected=provider_kind == "codeberg">"codeberg"</option>
                                </select>
                            </label>
                            <label class="form-field">
                                "Provider repo (owner/name)"
                                <input class="field-input" aria-label=format!("Remote {index} provider repo") prop:value=provider_repo on:input=move |ev| {
                                    let value = event_target_value(&ev);
                                    set_config.update(|c| {
                                        if let Some(provider) = &mut c.remotes[index].provider {
                                            provider.repo = value;
                                        } else {
                                            set_provider_kind_value.set("github".into());
                                            c.remotes[index].provider = Some(rw_model::ProviderRef {
                                                kind: rw_model::ProviderKind::Github,
                                                repo: value,
                                                token_env: None,
                                            });
                                        }
                                    });
                                } />
                            </label>
                            <Button
                                variant=ButtonVariant::Destructive
                                attr:aria-label=format!("Remove remote {name}")
                                on:click=move |_| set_config.update(|c| { c.remotes.remove(index); })
                            >"×"</Button>
                        </div>
                    }
                }
                </For>

                <div class="subhead">
                    <h3>"Local checkouts"</h3>
                    <Button variant=ButtonVariant::Secondary on:click=move |_| set_config.update(|c| {
                        let remote = c.remotes.first().map(|r| r.name.clone()).unwrap_or_default();
                        c.checkouts.push(GitCheckout {
                            name: String::new(),
                            path: Default::default(),
                            remote,
                            branch: "main".into(),
                        });
                    })>"+ Checkout"</Button>
                </div>
                <p class="sub">"Paths are relative to the root folder. Sync fetches every remote into each checkout."</p>
                <For
                    each=move || config.with(|c| c.checkouts.iter().cloned().enumerate().collect::<Vec<_>>())
                    key=|item| item.0
                    let((index, checkout))
                >
                {
                    let options = config.get_untracked().remotes.iter().map(|r| (r.name.clone(), r.name.clone())).collect::<Vec<_>>();
                        let name = checkout.name.clone();
                        let name_for_input = name.clone();
                        let path = checkout.path.to_string_lossy().to_string();
                        let branch = checkout.branch.clone();
                        let current = checkout.remote.clone();
                        view! {
                            <div class="row">
                                <label class="form-field">"Checkout name"
                                    <input class="field-input" aria-label=format!("Checkout {index} name") prop:value=name_for_input on:input=move |ev| {
                                        let value = event_target_value(&ev);
                                        set_config.update(|c| c.checkouts[index].name = value);
                                    } />
                                </label>
                                <label class="form-field">"Path (relative to root)"
                                    <input class="field-input" aria-label=format!("Checkout {index} path") prop:value=path on:input=move |ev| {
                                        let value = event_target_value(&ev);
                                        set_config.update(|c| c.checkouts[index].path = value.into());
                                    } />
                                </label>
                                <label class="form-field">"Branch"
                                    <input class="field-input" aria-label=format!("Checkout {index} branch") prop:value=branch on:input=move |ev| {
                                        let value = event_target_value(&ev);
                                        set_config.update(|c| c.checkouts[index].branch = value);
                                    } />
                                </label>
                                <label class="form-field">
                                    "Remote"
                                    <select class="field-input" aria-label=format!("Checkout {index} remote") on:change=move |ev| {
                                        let value = event_target_value(&ev);
                                        set_config.update(|c| c.checkouts[index].remote = value);
                                    }>
                                        {options.iter().map(|(value, text)| {
                                            let selected = *value == current;
                                            view! { <option value=value.clone() selected=selected>{text.clone()}</option> }
                                        }).collect_view()}
                                    </select>
                                </label>
                                <Button
                                    variant=ButtonVariant::Destructive
                                    attr:aria-label=format!("Remove checkout {name}")
                                    on:click=move |_| set_config.update(|c| { c.checkouts.remove(index); })
                                >"×"</Button>
                            </div>
                        }
                }
                </For>
            }.into_any()
        }
        VcsConfig::Plugin(_) => {
            view! {
                <div class="form-grid">
                    <div class="form-field"><Label html_for="plugin-name">"Plugin name"</Label><Input id="plugin-name" bind_value=plugin_name /></div>
                    <div class="form-field"><Label html_for="plugin-paths">"Paths (comma separated, relative to root)"</Label><Input id="plugin-paths" bind_value=plugin_paths /></div>
                    <div class="form-field"><Label html_for="plugin-options">"Options (JSON)"</Label><Input id="plugin-options" bind_value=plugin_options /></div>
                </div>
            }.into_any()
        }
        VcsConfig::Rcs(_) => {
            view! {
                <div class="form-grid">
                    <div class="form-field"><Label html_for="rcs-file">"Managed file"</Label><Input id="rcs-file" bind_value=file /></div>
                    <div class="form-field"><Label html_for="rcs-dir">"RCS directory (optional)"</Label><Input id="rcs-dir" bind_value=rcs_dir /></div>
                </div>
            }.into_any()
        }
    };

    view! {
        <div class="form-grid">
            <div class="form-field"><Label html_for="source-id">"ID"</Label><Input id="source-id" bind_value=id readonly=!is_new /></div>
            <div class="form-field"><Label html_for="source-label">"Label"</Label><Input id="source-label" bind_value=label /></div>
            <div class="form-field"><Label html_for="source-interval">"Interval (seconds)"</Label><Input id="source-interval" r#type=InputType::Number bind_value=interval /></div>
        </div>
        <details open=is_git>
            <summary>"Remotes, forks & checkouts (advanced)"</summary>
        {vcs_fields}
        <div class="subhead">
            <h3>"Imports — bring a remote into this source"</h3>
            <Button variant=ButtonVariant::Secondary on:click=move |_| imports.update(|list| {
                list.push(ImportSpec {
                    ingest: Some("git-ingest".into()),
                    url: String::new(),
                    path: None,
                    options: serde_json::Value::Null,
                });
            })>"+ Import"</Button>
        </div>
        <datalist id="ingest-systems">
            {ingest_names.iter().map(|name| view! { <option value=name.clone() /> }).collect_view()}
        </datalist>
        <For
            each=move || imports.with(|list| list.iter().cloned().enumerate().collect::<Vec<_>>())
            key=|item| item.0
            let((index, import))
        >
            <div class="row">
                <label class="form-field">
                    "Ingest system"
                    <input class="field-input" list="ingest-systems" aria-label=format!("Import {index} ingest") prop:value=import.ingest.clone().unwrap_or_default() on:input=move |ev| {
                        let value = event_target_value(&ev);
                        imports.update(|list| list[index].ingest = Some(value));
                    } />
                </label>
                <label class="form-field">
                    "Remote URL"
                    <input class="field-input" aria-label=format!("Import {index} URL") prop:value=import.url.clone() on:input=move |ev| {
                        let value = event_target_value(&ev);
                        imports.update(|list| list[index].url = value);
                    } />
                </label>
                <label class="form-field">
                    "Target folder (under root)"
                    <input class="field-input" aria-label=format!("Import {index} path") prop:value=import.path.clone().map(|p| p.to_string_lossy().to_string()).unwrap_or_default() on:input=move |ev| {
                        let value = event_target_value(&ev);
                        imports.update(|list| list[index].path = if value.is_empty() { None } else { Some(value.into()) });
                    } />
                </label>
                <Button variant=ButtonVariant::Destructive attr:aria-label=format!("Remove import {index}") on:click=move |_| imports.update(|list| { list.remove(index); })>"×"</Button>
            </div>
        </For>
        </details>
        <div class="actions">
            <Button variant=ButtonVariant::Secondary on:click=move |_| {
                for (index, import) in imports.get().iter().enumerate() {
                    if import.url.trim().is_empty() {
                        on_error(true, format!("Import {} needs a URL", index + 1));
                        return;
                    }
                }
                let vcs = match kind.clone() {
                    VcsConfig::Git(_) => VcsConfig::Git(config.get()),
                    VcsConfig::Rcs(_) => VcsConfig::Rcs(RcsConfig {
                        file: file.get().into(),
                        rcs_dir: if rcs_dir.get().is_empty() { None } else { Some(rcs_dir.get().into()) },
                    }),
                    VcsConfig::Plugin(_) => {
                        let options = match serde_json::from_str::<serde_json::Value>(&plugin_options.get()) {
                            Ok(value) => value,
                            Err(error) => {
                                on_error(true, format!("Invalid plugin options JSON: {error}"));
                                return;
                            }
                        };
                        VcsConfig::Plugin(PluginConfig {
                            plugin: plugin_name.get(),
                            options,
                            paths: plugin_paths.get().split(',').map(str::trim).filter(|p| !p.is_empty()).map(PathBuf::from).collect(),
                        })
                    }
                };
                on_save(Source {
                    id: id.get(),
                    label: if label.get().is_empty() { None } else { Some(label.get()) },
                    interval_secs: interval.get().parse().unwrap_or(120).max(1),
                    imports: imports.get(),
                    notes: notes.clone(),
                    tags: tags.clone(),
                    vcs,
                }, original.clone());
            }>"Save source"</Button>
            <Button variant=ButtonVariant::Ghost on:click=move |_| on_cancel(())>"Cancel"</Button>
        </div>
    }
}

#[component]
fn Providers(
    snapshots: ReadSignal<Vec<rw_model::ProviderSnapshot>>,
    load_failed: ReadSignal<bool>,
    selected_project: RwSignal<Option<String>>,
    set_drawer_open: WriteSignal<bool>,
) -> impl IntoView {
    view! {
        {move || {
            if load_failed.get() {
                return view! {
                    <Alert class="border-destructive/50 text-destructive">"Could not reach integrations. Use Refresh to retry."</Alert>
                }.into_any();
            }
            let list = snapshots.get();
            if list.is_empty() {
                view! {
                    <Empty>
                        <EmptyTitle>"No integrations configured."</EmptyTitle>
                        <EmptyDescription>"Add a provider to a git remote on the Sources page, then set RW_GITHUB_TOKEN, RW_GITLAB_TOKEN or RW_CODEBERG_TOKEN."</EmptyDescription>
                    </Empty>
                }.into_any()
            } else {
                view! {
                    <div class="cards">
                        {list.into_iter().map(|snapshot| {
                            let project_id = snapshot.source.clone();
                            view! {
                            <Card>
                                <div class="card-head">
                                    <div>
                                        <h2>{snapshot.remote.clone()}</h2>
                                        <div class="sub">{snapshot.source.clone()} " · " {match snapshot.kind {
                                    rw_model::ProviderKind::Github => "GitHub".to_string(),
                                    rw_model::ProviderKind::Gitlab => "GitLab".to_string(),
                                    rw_model::ProviderKind::Codeberg => "Codeberg".to_string(),
                                }}</div>
                                    </div>
                                    <Button variant=ButtonVariant::Ghost attr:aria-label=format!("Open project {}", project_id.clone()) on:click=move |_| {
                                        selected_project.set(Some(project_id.clone()));
                                        set_drawer_open.set(true);
                                    }>"Manage in project"</Button>
                                </div>
                                {snapshot.error.map(|error| view! {
                                    <Alert class="border-destructive/50 text-destructive">{error}</Alert>
                                })}
                                <div class="sub">"Pull requests and pipelines live in the project drawer."</div>
                            </Card>
                            }
                        }).collect_view()}
                    </div>
                }.into_any()
            }
        }}
    }
}

#[component]
fn SettingsView(
    snapshot: ReadSignal<Option<State>>,
    set_snapshot: WriteSignal<Option<State>>,
    save_config: impl Fn(Box<dyn FnOnce(&mut rw_model::Config) + Send>) + Copy + Send + Sync + 'static,
    notify: impl Fn(bool, String) + Copy + Send + Sync + 'static,
    state_seq: RwSignal<u64>,
) -> impl IntoView {
    let initial = snapshot
        .get()
        .map(|state| state.config.settings)
        .unwrap_or_default();
    let settings = RwSignal::new(initial.clone());
    let base = initial.clone();
    let bundle_text = RwSignal::new(String::new());
    let import_root = RwSignal::new(initial.import_root.to_string_lossy().to_string());
    let folder_template = RwSignal::new(initial.folder_template.clone());
    let default_interval = RwSignal::new(initial.default_interval_secs.to_string());
    let open_command = RwSignal::new(initial.open.command.clone());
    let open_args = RwSignal::new(initial.open.args.join("
"));
    let export_status = RwSignal::new(String::new());
    let plugin_config = RwSignal::new(
        serde_json::to_string_pretty(&initial.plugins).unwrap_or_else(|_| "{}".into()),
    );
    let defaults = RwSignal::new(
        initial
            .source_defaults
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Vec<_>>(),
    );
    let reseed = move |value: rw_model::Settings| {
        settings.set(value.clone());
        import_root.set(value.import_root.to_string_lossy().to_string());
        folder_template.set(value.folder_template.clone());
        default_interval.set(value.default_interval_secs.to_string());
        open_command.set(value.open.command.clone());
        open_args.set(value.open.args.join("\n"));
        plugin_config.set(serde_json::to_string_pretty(&value.plugins).unwrap_or_else(|_| "{}".into()));
        defaults.set(
            value
                .source_defaults
                .iter()
                .map(|(key, item)| (key.clone(), item.clone()))
                .collect::<Vec<_>>(),
        );
    };

    view! {
        <Card>
            <div class="subhead"><h2>"Background process"</h2></div>
            <p class="sub">"Choose when sources are checked in the background. Tray and minimized options apply after restarting the app."</p>
            <div class="form-grid">
                <div class="form-field">
                    <Label html_for="settings-background">"Watching"</Label>
                    <select id="settings-background" class="field-input" prop:value=move || match settings.get().background {
                        BackgroundMode::Always => "always",
                        BackgroundMode::WhileWindowOpen => "while_window_open",
                    } on:change=move |ev| {
                        let value = event_target_value(&ev);
                        settings.update(|current| {
                            current.background = if value == "always" { BackgroundMode::Always } else { BackgroundMode::WhileWindowOpen };
                        });
                    }>
                        <option value="always">"Always (tray / headless)"</option>
                        <option value="while_window_open">"Only while the window is open"</option>
                    </select>
                </div>
                <div class="form-field">
                    <Label html_for="settings-tray">"Show tray icon"</Label>
                    <input id="settings-tray" class="field-input" type="checkbox" prop:checked=settings.get().tray on:change=move |ev| {
                        let checked = event_target_checked(&ev);
                        settings.update(|current| current.tray = checked);
                    } />
                </div>
                <div class="form-field">
                    <Label html_for="settings-minimized">"Start minimized (needs Show tray icon to restore)"</Label>
                    <input id="settings-minimized" class="field-input" type="checkbox" prop:checked=settings.get().start_minimized on:change=move |ev| {
                        let checked = event_target_checked(&ev);
                        settings.update(|current| current.start_minimized = checked);
                    } />
                </div>
            </div>
            <details>
                <summary>"Integrations and tokens"</summary>
                <div class="sub">"Forge tokens come from RW_GITHUB_TOKEN, RW_GITLAB_TOKEN or RW_CODEBERG_TOKEN. HTTPS Git uses RW_GIT_USER_<HOST> and RW_GIT_TOKEN_<HOST>."</div>
            </details>
            <details>
                <summary>"Open action"</summary>
            <div class="subhead"><h3>"Open action"</h3></div>
            <div class="form-grid">
                <div class="form-field">
                    <Label html_for="settings-open-command">"Command"</Label>
                    <Input id="settings-open-command" bind_value=open_command placeholder="code" />
                </div>
                <div class="form-field" style="flex:2 1 300px">
                    <Label html_for="settings-open-args">"Arguments (one per line, use {path})"</Label>
                    <textarea id="settings-open-args" class="field-input" style="min-height:70px" prop:value=open_args on:input=move |ev| open_args.set(event_target_value(&ev)) />
                </div>
            </div>
            <p class="sub" role="status">
                {move || format!("Preview: {} {}", open_command.get(), open_args.get().lines()
                    .filter(|line| !line.trim().is_empty())
                    .map(|line| line.replace("{path}", "/path/to/source"))
                    .collect::<Vec<_>>().join(" "))}
            </p>
            {move || if open_args.get().contains("{path}") {
                ().into_any()
            } else {
                view! { <p class="sub">"Warning: no {'{path}'} argument — Open will not receive the source path."</p> }.into_any()
            }}
            </details>
            <details>
                <summary>"Import defaults"</summary>
            <div class="subhead"><h3>"Import defaults"</h3></div>
            <div class="form-grid">
                <div class="form-field">
                    <Label html_for="settings-import-root">"Import folder (under root)"</Label>
                    <Input id="settings-import-root" bind_value=import_root />
                </div>
                <div class="form-field">
                    <Label html_for="settings-folder-template">"Folder template"</Label>
                    <Input id="settings-folder-template" bind_value=folder_template />
                    <span class="sub">"Use {owner} and {repo}; {owner} is skipped when missing."</span>
                </div>
                <div class="form-field">
                    <Label html_for="settings-interval">"Default interval (seconds)"</Label>
                    <Input id="settings-interval" r#type=InputType::Number bind_value=default_interval />
                </div>
            </div>
            </details>
            <details>
                <summary>"Source defaults and plugins"</summary>
            <div class="subhead"><h3>"Source defaults by type and tag"</h3></div>
            <p class="sub">"Use keys like type:git or tag:work. Tag defaults override type defaults."</p>
            <div class="actions">
                <Button variant=ButtonVariant::Secondary on:click=move |_| defaults.update(|list| {
                    list.push((String::new(), SourceDefault {
                        interval_secs: Some(120),
                        auto_fetch: Some(true),
                        token_env: None,
                    }));
                })>"+ Default"</Button>
            </div>
            <For
                each=move || defaults.with(|list| list.iter().cloned().enumerate().collect::<Vec<_>>())
                key=|item| item.0
                let((index, row))
            >
                <div class="row">
                    <label class="form-field">
                        "Key"
                        <input class="field-input" aria-label=format!("Default {index} key") prop:value=row.0.clone() on:input=move |ev| {
                            let value = event_target_value(&ev);
                            defaults.update(|list| list[index].0 = value);
                        } />
                    </label>
                    <label class="form-field">
                        "Interval (seconds)"
                        <input class="field-input" type="number" aria-label=format!("Default {index} interval") prop:value=row.1.interval_secs.map(|v| v.to_string()).unwrap_or_default() on:input=move |ev| {
                            let value = event_target_value(&ev).parse::<u64>().ok();
                            defaults.update(|list| list[index].1.interval_secs = value);
                        } />
                    </label>
                    <label class="form-field">
                        "Auto fetch"
                        <input class="field-input" type="checkbox" aria-label=format!("Default {index} auto fetch") prop:checked=row.1.auto_fetch.unwrap_or(true) on:change=move |ev| {
                            let checked = event_target_checked(&ev);
                            defaults.update(|list| list[index].1.auto_fetch = Some(checked));
                        } />
                    </label>
                    <label class="form-field">
                        "Token env"
                        <input class="field-input" aria-label=format!("Default {index} token env") prop:value=row.1.token_env.clone().unwrap_or_default() on:input=move |ev| {
                            let value = event_target_value(&ev);
                            defaults.update(|list| list[index].1.token_env = if value.is_empty() { None } else { Some(value) });
                        } />
                    </label>
                    <Button variant=ButtonVariant::Destructive attr:aria-label=format!("Remove default {index}") on:click=move |_| defaults.update(|list| { list.remove(index); })>"×"</Button>
                </div>
            </For>
            <div class="form-field" style="margin-top:12px">
                <Label html_for="plugin-config">"Plugin configuration (JSON, one object per plugin name)"</Label>
                <textarea id="plugin-config" class="field-input" style="min-height:110px" prop:value=plugin_config on:input=move |ev| plugin_config.set(event_target_value(&ev)) />
            </div>
            </details>
            <div class="actions" style="margin-top:16px">
                <Button variant=ButtonVariant::Secondary attr:disabled=move || snapshot.get().is_none() on:click=move |_| {
                    let base = base.clone();
                    let plugin_values = match serde_json::from_str::<std::collections::BTreeMap<String, serde_json::Value>>(&plugin_config.get()) {
                        Ok(value) => value,
                        Err(error) => {
                            notify(true, format!("Invalid plugin configuration JSON: {error}"));
                            return;
                        }
                    };
                    let notify = notify;
                    let save_config = save_config;
                    let reseed = reseed;
                    spawn_local(async move {
                        let Ok(state) = api::state().await else {
                            notify(true, "Daemon state is unavailable; cannot save settings".into());
                            return;
                        };
                        let mut merged = state.config.settings;
                        let form = settings.get();
                        if form.background != base.background { merged.background = form.background; }
                        if form.tray != base.tray { merged.tray = form.tray; }
                        if form.start_minimized != base.start_minimized { merged.start_minimized = form.start_minimized; }
                        let import_root: PathBuf = import_root.get().into();
                        if import_root != base.import_root { merged.import_root = import_root; }
                        if folder_template.get() != base.folder_template { merged.folder_template = folder_template.get(); }
                        let interval = default_interval.get().parse().unwrap_or(120).max(1);
                        if interval != base.default_interval_secs { merged.default_interval_secs = interval; }
                        if open_command.get() != base.open.command { merged.open.command = open_command.get(); }
                        let args: Vec<String> = open_args.get().lines().filter(|line| !line.trim().is_empty()).map(str::to_owned).collect();
                        if args != base.open.args { merged.open.args = args; }
                        if plugin_values != base.plugins { merged.plugins = plugin_values; }
                        let rules: std::collections::BTreeMap<String, SourceDefault> = defaults
                            .get()
                            .into_iter()
                            .filter(|(key, _)| !key.trim().is_empty())
                            .collect();
                        if rules != base.source_defaults { merged.source_defaults = rules; }
                        let values = merged.clone();
                        save_config(Box::new(move |config: &mut rw_model::Config| {
                            config.settings = values;
                        }));
                        reseed(merged);
                    });
                }>"Save settings"</Button>
            </div>
        </Card>

        <Card>
            <div class="subhead"><h2>"Move to another device"</h2></div>
            <p class="sub">"Only sources with a remote can be exported — they can be re-cloned elsewhere. Local-only sources (RCS, plain files) stay on this device."</p>
            <div class="actions">
                <Button variant=ButtonVariant::Secondary on:click=move |_| {
                    let set_text = bundle_text;
                    let status_signal = export_status;
                    let notify = notify;
                    spawn_local(async move {
                        match api::export_bundle().await {
                            Ok(result) if result.ok => {
                                if let Some(bundle) = result.export {
                                    let count = bundle.sources.len();
                                    set_text.set(serde_json::to_string_pretty(&bundle).unwrap_or_default());
                                    let status = if count == 0 {
                                        "Exported 0 sources — none of them have a remote.".to_string()
                                    } else {
                                        format!("Exported {count} source(s). Copy the JSON to the other device.")
                                    };
                                    status_signal.set(status);
                                }
                            }
                            Ok(result) => { status_signal.set(String::new()); notify(true, result.error.unwrap_or_else(|| "export failed".into())); }
                            Err(error) => { status_signal.set(String::new()); notify(true, error); },
                        }
                    });
                }>"Export configuration"</Button>
                <Button variant=ButtonVariant::Secondary attr:disabled=move || bundle_text.get().trim().is_empty() on:click=move |_| {
                    let text = bundle_text.get();
                    let set_snapshot = set_snapshot;
                    let status_signal = export_status;
                    let notify = notify;
                    status_signal.set(String::new());
                    spawn_local(async move {
                        match api::import_bundle(&text).await {
                            Ok(result) if result.ok => {
                                let reseed = reseed;
                                if let Some(state) = fetch_state(state_seq).await {
                                    reseed(state.config.settings.clone());
                                    set_snapshot.set(Some(state));
                                }
                                notify(false, result.message.unwrap_or_else(|| "Bundle imported".into()));
                            }
                            Ok(result) => notify(true, result.error.unwrap_or_else(|| "import failed".into())),
                            Err(error) => notify(true, error),
                        }
                    });
                }>"Import configuration"</Button>
            </div>
            <p class="sub">"Export fills the box with JSON to copy. Import pastes JSON from another device."</p>
            <textarea class="field-input" style="min-height:180px" aria-label="Export or import bundle JSON" prop:value=bundle_text on:input=move |ev| bundle_text.set(event_target_value(&ev))></textarea>
            <p class="sub" role="status">{move || export_status.get()}</p>
        </Card>
    }
}

#[component]
fn Activity(events: ReadSignal<Vec<Event>>) -> impl IntoView {
    view! {
        {move || {
            let list = events.get();
            if list.is_empty() {
                view! { <Empty><EmptyTitle>"No events yet. Changes will appear here."</EmptyTitle></Empty> }.into_any()
            } else {
                view! {
                    <ul class="events">
                        {list.into_iter().rev().map(|event| {
                            let (kind, message, at) = match event {
                                Event::ConfigSaved { at } => ("settings".to_string(), "Configuration saved".to_string(), at),
                                Event::Changed { source, message, at } => {
                                    ("status".to_string(), format!("{source}: {message}"), at)
                                }
                                Event::Failed { source, message, at } => ("problem".to_string(), format!("{source}: {message}"), at),
                            };
                            view! {
                                <li class="event">
                                    <time datetime=iso_timestamp(at)>{format_timestamp(at)}</time>
                                    <Badge variant=if kind == "problem" { BadgeVariant::Destructive } else { BadgeVariant::Muted }>{kind.clone()}</Badge>
                                    <span class="message">{message}</span>
                                </li>
                            }
                        }).collect_view()}
                    </ul>
                }.into_any()
            }
        }}
    }
}

fn iso_timestamp(seconds: u64) -> String {
    js_sys::Date::new(&JsValue::from_f64((seconds as f64) * 1000.0))
        .to_iso_string()
        .as_string()
        .unwrap_or_default()
}

fn format_timestamp(seconds: u64) -> String {
    let date = js_sys::Date::new(&JsValue::from_f64((seconds as f64) * 1000.0));
    date.to_locale_string("en-GB", &JsValue::UNDEFINED)
        .as_string()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_autosave_does_not_block_a_source_edit() {
        let mut a = Source {
            id: "demo".into(),
            label: Some("Demo".into()),
            interval_secs: 120,
            imports: vec![],
            notes: "old".into(),
            tags: vec![],
            vcs: VcsConfig::Rcs(RcsConfig { file: "f".into(), rcs_dir: None }),
        };
        let mut b = a.clone();
        b.notes = "new".into();
        assert!(source_core_eq(&a, &b));
        a.label = Some("Changed".into());
        assert!(!source_core_eq(&a, &b));
    }

    #[test]
    fn stale_or_wrong_project_save_is_ignored() {
        assert!(should_apply_save(3, 3, Some("demo"), "demo"));
        assert!(!should_apply_save(2, 3, Some("demo"), "demo"));
        assert!(!should_apply_save(3, 3, Some("other"), "demo"));
        assert!(!should_apply_save(3, 3, None, "demo"));
    }

    #[test]
    fn repo_hint_parses_urls_and_ignores_local_paths() {
        assert_eq!(repo_hint("https://github.com/leptos-rs/leptos.git"), Some((Some("leptos-rs".into()), "leptos".into())));
        assert_eq!(repo_hint("git@github.com:me/rw.git"), Some((Some("me".into()), "rw".into())));
        assert_eq!(repo_hint("owner/repo"), Some((Some("owner".into()), "repo".into())));
        assert_eq!(repo_hint("/home/me/repo"), None);
        assert_eq!(repo_hint(""), None);
    }

    #[test]
    fn clone_target_drops_traversal_parts() {
        assert_eq!(
            clone_target(Some(".."), "evil", "imports", "{owner}/{repo}"),
            "imports/evil"
        );
    }

    #[test]
    fn clone_target_fills_the_folder_template() {
        assert_eq!(clone_target(Some("me"), "rw", "imports", "{owner}/{repo}"), "imports/me/rw");
        assert_eq!(clone_target(None, "rw", "imports", "{owner}/{repo}"), "imports/rw");
        assert_eq!(clone_target(Some("me"), "rw", "", "{repo}"), "rw");
    }

    #[test]
    fn palette_and_status_helpers() {
        assert!(contains_text("Overview", "view"));
        assert!(contains_text("Sources", "  "));
        assert!(!contains_text("Activity", "forge"));
        assert_eq!(watching_text(None), "Connecting to the watcher");
        assert_eq!(watching_text(Some(BackgroundMode::Always)), "Watching continuously");
        assert_eq!(badge(&UnitState::Behind { count: 2 }).1, "↓ −2 behind");
    }
}
