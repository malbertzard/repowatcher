use js_sys::JsString;
use std::cell::RefCell;
use rw_model::{ApiResult, Command as ApiCommand, Event, PluginNames, ProviderSnapshot, State};
use serde::de::DeserializeOwned;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, MessageEvent, Request, RequestInit, Response, WebSocket};

fn token() -> Result<String, String> {
    let window = web_sys::window().ok_or("no browser window")?;
    window
        .location()
        .hash()
        .ok()
        .and_then(|hash| hash.strip_prefix('#').map(str::to_owned))
        .and_then(|hash| {
            hash.split('&')
                .find_map(|pair| pair.strip_prefix("token="))
                .filter(|token| !token.is_empty())
                .map(str::to_owned)
        })
        .ok_or_else(|| String::from("token is missing; open the URL printed by rw daemon"))
}

fn host() -> Result<String, String> {
    web_sys::window()
        .ok_or("no browser window")?
        .location()
        .host()
        .map_err(|_| "cannot read location".into())
}

async fn request<T: DeserializeOwned>(path: &str, method: &str, body: Option<&str>) -> Result<T, String> {
    let init = RequestInit::new();
    init.set_method(method);
    let headers = Headers::new().map_err(|_| "headers failed")?;
    headers
        .append("Authorization", &format!("Bearer {}", token()?))
        .map_err(|_| "header failed")?;
    if body.is_some() {
        headers.append("Content-Type", "application/json").map_err(|_| "header failed")?;
    }
    init.set_headers(&headers);
    if let Some(body) = body {
        init.set_body(&JsValue::from_str(body));
    }
    let request = Request::new_with_str_and_init(path, &init).map_err(|_| "request failed")?;
    let window = web_sys::window().ok_or("no browser window")?;
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| "network error")?
        .dyn_into::<Response>()
        .map_err(|_| "bad response")?;
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    let text = JsFuture::from(response.text().map_err(|_| "body failed")?)
        .await
        .map_err(|_| "body failed")?
        .as_string()
        .ok_or("body was not text")?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

pub async fn state() -> Result<State, String> {
    request("/api/state", "GET", None).await
}

pub async fn preview(url: &str) -> Result<ApiResult, String> {
    command(ApiCommand::ImportPreview { url: url.into() }).await
}

pub async fn quick_import(url: &str, folder: Option<String>, ingest: Option<String>) -> Result<ApiResult, String> {
    command(ApiCommand::QuickImport { url: url.into(), folder, ingest }).await
}

pub async fn export_bundle() -> Result<ApiResult, String> {
    command(ApiCommand::ExportBundle).await
}

pub async fn import_bundle(text: &str) -> Result<ApiResult, String> {
    let bundle: rw_model::ExportBundle =
        serde_json::from_str(text).map_err(|error| format!("Invalid bundle JSON: {error}"))?;
    command(ApiCommand::ImportBundle { bundle }).await
}

pub async fn run_import(source: &str) -> Result<ApiResult, String> {
    command(ApiCommand::Import { source: source.into() }).await
}

pub async fn kinds() -> Result<Vec<rw_model::SourceKind>, String> {
    request("/api/kinds", "GET", None).await
}

pub async fn integrations(source: &str) -> Result<rw_model::ApiResult, String> {
    command(ApiCommand::Integrations { source: source.into() }).await
}

pub async fn integration_action(
    source: &str,
    plugin: &str,
    action: &str,
    value: &str,
) -> Result<ApiResult, String> {
    command(ApiCommand::IntegrationAction {
        source: source.into(),
        plugin: plugin.into(),
        action: action.into(),
        value: value.into(),
    })
    .await
}

pub async fn actions() -> Result<Vec<rw_model::SourceAction>, String> {
    request("/api/actions", "GET", None).await
}

pub async fn run_action(source: &str, action: &str) -> Result<ApiResult, String> {
    command(ApiCommand::RunAction { source: source.into(), action: action.into() }).await
}

pub async fn plugins() -> Result<PluginNames, String> {
    request("/api/plugins", "GET", None).await
}

pub async fn providers() -> Result<Vec<ProviderSnapshot>, String> {
    request("/api/providers", "GET", None).await
}

pub async fn command(command: ApiCommand) -> Result<ApiResult, String> {
    let body = serde_json::to_string(&command).map_err(|e| e.to_string())?;
    request("/api/command", "POST", Some(&body)).await
}

struct EventStream {
    url: String,
    delay: u32,
    socket: Option<WebSocket>,
    on_message: Closure<dyn FnMut(MessageEvent)>,
    on_open: Closure<dyn FnMut()>,
    on_close: Closure<dyn FnMut()>,
    retry: Closure<dyn FnMut()>,
}

thread_local! {
    static STREAM: RefCell<Option<EventStream>> = const { RefCell::new(None) };
}

fn open_socket(stream: &mut EventStream) -> Result<(), String> {
    let socket = WebSocket::new(&stream.url).map_err(|_| "websocket failed")?;
    socket.set_onmessage(Some(stream.on_message.as_ref().unchecked_ref()));
    socket.set_onopen(Some(stream.on_open.as_ref().unchecked_ref()));
    socket.set_onclose(Some(stream.on_close.as_ref().unchecked_ref()));
    stream.socket = Some(socket);
    Ok(())
}

fn schedule_retry() {
    STREAM.with(|cell| {
        let mut borrow = cell.borrow_mut();
        if let Some(stream) = borrow.as_mut() {
            stream.delay = stream.delay.saturating_mul(2).min(30_000);
            if let Some(window) = web_sys::window() {
                let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                    stream.retry.as_ref().unchecked_ref(),
                    stream.delay as i32,
                );
            }
        }
    });
}

pub fn on_event<F>(handler: F) -> Result<(), String>
where
    F: Fn(Event) + Clone + 'static,
{
    let url = format!("ws://{}/api/events?token={}", host()?, token()?);
    let message_handler = handler;
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        if let Ok(text) = event.data().dyn_into::<JsString>() {
            if let Some(text) = text.as_string() {
                if let Ok(parsed) = serde_json::from_str::<Event>(&text) {
                    message_handler(parsed);
                }
            }
        }
    });
    let on_open = Closure::<dyn FnMut()>::new(|| {
        STREAM.with(|cell| {
            if let Some(stream) = cell.borrow_mut().as_mut() {
                stream.delay = 1000;
            }
        });
    });
    let on_close = Closure::<dyn FnMut()>::new(schedule_retry);
    let retry = Closure::<dyn FnMut()>::new(|| {
        STREAM.with(|cell| {
            if let Some(stream) = cell.borrow_mut().as_mut() {
                let _ = open_socket(stream);
            }
        });
    });
    let mut stream = EventStream {
        url,
        delay: 1000,
        socket: None,
        on_message,
        on_open,
        on_close,
        retry,
    };
    open_socket(&mut stream)?;
    STREAM.with(|cell| *cell.borrow_mut() = Some(stream));
    Ok(())
}
