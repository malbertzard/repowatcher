use clap::{Parser, Subcommand};
use rw_model::{ApiResult, Command as ApiCommand, State};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "rw", about = "Watch and sync code sources across VCS systems")]
struct Cli {
    /// Config file (defaults to ~/.config/rw/config.yaml)
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Daemon port
    #[arg(long, default_value_t = 3030)]
    port: u16,

    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Run the background daemon
    Daemon,
    /// Print the config path used by default
    ConfigPath,
    /// Print the UI link with the daemon token
    Url,
    /// List sources and their status
    List,
    /// Fetch the latest state of one source
    Sync { source: String },
    /// Show a diff for a source (optionally one unit)
    Diff { source: String, unit: Option<String> },
    /// Open a source in the configured editor
    Open { source: String },
}

fn main() -> Result<(), String> {
    let cli = Cli::parse();
    match cli.command {
        Action::Daemon => {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(|e| e.to_string())?;
            runtime.block_on(rw_core::run_daemon(cli.config, cli.port))
        }
        Action::ConfigPath => {
            println!("{}", rw_core::default_config_path().display());
            Ok(())
        }
        Action::Url => {
            let config = cli.config.clone().unwrap_or_else(rw_core::default_config_path);
            let dir = config.parent().ok_or("config has no parent directory")?;
            let token = std::fs::read_to_string(dir.join("token"))
                .map_err(|error| format!("read {}: {error}", dir.join("token").display()))?;
            println!("http://127.0.0.1:{}/#token={}", cli.port, token.trim());
            Ok(())
        }
        Action::List => {
            let state: State = client(&cli)?.get("/api/state")?;
            println!("{:<20} {:<8} {:<16} STATUS", "ID", "VCS", "UNIT");
            let statuses: HashMap<&str, &rw_model::SourceStatus> = state
                .statuses
                .iter()
                .map(|status| (status.id.as_str(), status))
                .collect();
            for source in &state.config.sources {
                let vcs = source.vcs_name();
                let status = statuses.get(source.id.as_str()).copied();
                let units: Vec<String> = status
                    .map(|s| {
                        if let Some(error) = &s.error {
                            vec![format!("error: {error}")]
                        } else {
                            s.units.iter().map(|u| format!("{}: {:?}", u.name, u.state)).collect()
                        }
                    })
                    .unwrap_or_default();
                if units.is_empty() {
                    println!("{:<20} {:<8} {:<16} pending", source.id, vcs, "-");
                }
                for (index, unit) in units.iter().enumerate() {
                    let id = if index == 0 { source.id.as_str() } else { "" };
                    let vcs = if index == 0 { vcs } else { "" };
                    println!("{id:<20} {vcs:<8} {:<16} {unit}", "");
                }
            }
            Ok(())
        }
        Action::Sync { ref source } => {
            let result: ApiResult = client(&cli)?.post(&ApiCommand::Sync { source: source.clone() })?;
            print_result(result)
        }
        Action::Diff { ref source, ref unit } => {
            let result: ApiResult = client(&cli)?.post(&ApiCommand::Diff {
                source: source.clone(),
                unit: unit.clone(),
            })?;
            match &result.diff {
                Some(diff) => {
                    print!("{}", diff.patch);
                    print_result(result)
                }
                None => print_result(result),
            }
        }
        Action::Open { ref source } => {
            let result: ApiResult = client(&cli)?.post(&ApiCommand::Open { source: source.clone() })?;
            print_result(result)
        }
    }
}

fn print_result(result: ApiResult) -> Result<(), String> {
    if result.ok {
        Ok(())
    } else {
        Err(result.error.unwrap_or_else(|| "command failed".into()))
    }
}

struct Client {
    base: String,
    http: reqwest::blocking::Client,
    token: String,
}

impl Client {
    fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        self.http
            .get(format!("{}{}", self.base, path))
            .bearer_auth(&self.token)
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json())
            .map_err(|e| format!("daemon request: {e}"))
    }

    fn post<T: serde::de::DeserializeOwned>(&self, command: &ApiCommand) -> Result<T, String> {
        self.http
            .post(format!("{}/api/command", self.base))
            .bearer_auth(&self.token)
            .json(command)
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json())
            .map_err(|e| format!("daemon request: {e}"))
    }
}

fn client(cli: &Cli) -> Result<Client, String> {
    let config = cli.config.clone().unwrap_or_else(rw_core::default_config_path);
    let dir = config.parent().ok_or("config has no parent directory")?;
    let token = std::fs::read_to_string(dir.join("token"))
        .map_err(|e| format!("read {}: {e}; is the daemon running?", dir.join("token").display()))?
        .trim()
        .to_owned();
    Ok(Client {
        base: format!("http://127.0.0.1:{}", cli.port),
        http: reqwest::blocking::Client::new(),
        token,
    })
}
