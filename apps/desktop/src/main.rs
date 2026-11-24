//! Thin Tauri shell. All logic lives in `rw daemon`; the webview loads the
//! daemon's own UI with the token in the URL fragment, like a browser.

use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{
    Manager, WindowEvent,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

fn main() {
    let daemon = Arc::new(Mutex::new(None::<Child>));
    let daemon_for_setup = daemon.clone();
    tauri::Builder::default()
        .setup(move |app| {
            let port: u16 = std::env::var("RW_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(3030);
            let config = std::env::var_os("RW_CONFIG")
                .map(PathBuf::from)
                .unwrap_or_else(|| config_dir().join("config.yaml"));
            *daemon_for_setup.lock().unwrap() = start_daemon(port, &config);
            if let Some(window) = app.get_webview_window("main") {
                let daemon_for_window = daemon.clone();
                window.on_window_event(move |event| {
                    if matches!(event, WindowEvent::Destroyed) {
                        if let Some(child) = daemon_for_window.lock().unwrap().as_mut() {
                            let _ = child.kill();
                            let _ = child.wait();
                        }
                    }
                });
                let fallback_dir = config_dir();
                let token_path = config.parent().unwrap_or(&fallback_dir).join("token");
                if let Ok(token) = std::fs::read_to_string(token_path) {
                    let url = format!("http://127.0.0.1:{port}/#token={}", token.trim());
                    let _ = window.eval(&format!(
                        "setTimeout(()=>window.location.replace('{url}'),500)"
                    ));
                }
            }
            let settings = load_settings(&config);
            if settings.start_minimized && settings.tray {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            if settings.tray {
                build_tray(app, daemon.clone())?;
            }
            // shortcut: autostart is stored in config but not wired to the OS
            // launcher yet; upgrade when the desktop shell is built on a target
            // with a known login-service mechanism.
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run RepoWatcher");
}

fn config_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(".config")
        .join("rw")
}

fn load_settings(path: &PathBuf) -> rw_model::Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| yaml_serde::from_str::<rw_model::Config>(&text).ok())
        .map(|config| config.settings)
        .unwrap_or_default()
}

fn build_tray(
    app: &tauri::App,
    daemon: Arc<Mutex<Option<Child>>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Open RepoWatcher", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let icon = app
        .default_window_icon()
        .ok_or("no default window icon")?
        .clone();

    TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => {
                if let Some(child) = daemon.lock().unwrap().as_mut() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;
    Ok(())
}

fn start_daemon(port: u16, config: &PathBuf) -> Option<Child> {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();

    if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
        return None;
    }
    let binary = std::env::var("RW_BIN").unwrap_or_else(|_| "rw".into());
    let child = std::process::Command::new(&binary)
        .args([
            "--config",
            &config.to_string_lossy(),
            "--port",
            &port.to_string(),
            "daemon",
        ])
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) => {
            eprintln!("could not start {binary}: {error}");
            return None;
        }
    };
    for _ in 0..10 {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_ok() {
            return Some(child);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    eprintln!("daemon did not become ready on port {port}");
    let _ = child.kill();
    let _ = child.wait();
    None
}
