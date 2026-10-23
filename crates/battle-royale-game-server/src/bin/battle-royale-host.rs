use battle_royale_core::BattleMatchId;
use battle_royale_game_server::{build_match_entries, build_match_host};
use game_server::{
    BrowserRoutePrefix, DEFAULT_HOST_STATUS_PORT, DEFAULT_RECONNECT_GRACE_TICKS,
    MatchHostRecoveryConfig, MatchHostStatusConfig, MatchHostWebTransportConfig,
    RejectMatchControlService, prepare_match_host_for_recovery,
    serve_match_host_with_status_and_control_and_shutdown,
    serve_prepared_match_host_with_status_and_control_and_shutdown,
};
use std::env;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;
use tokio::sync::mpsc;

const DEFAULT_PORT: u16 = 4433;
const DEFAULT_DRAIN_GRACE_MS: u64 = 500;
const DEFAULT_ROUTE_PREFIX: &str = "/game";
const DEFAULT_MATCH_IDS: &str = "1";

#[derive(Clone, Debug, Eq, PartialEq)]
struct ConfigError(String);

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ConfigError {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let match_ids = parse_match_ids(&read_string("BATTLE_ROYALE_MATCH_IDS", DEFAULT_MATCH_IDS)?)?;
    let port = read_number("BATTLE_ROYALE_PORT", DEFAULT_PORT)?;
    let status_port = read_number("BATTLE_ROYALE_STATUS_PORT", DEFAULT_HOST_STATUS_PORT)?;
    let reconnect_grace_ticks = read_number(
        "BATTLE_ROYALE_RECONNECT_GRACE_TICKS",
        DEFAULT_RECONNECT_GRACE_TICKS,
    )?;
    let drain_grace_ms = read_number("BATTLE_ROYALE_DRAIN_GRACE_MS", DEFAULT_DRAIN_GRACE_MS)?;
    let certificate_pem = PathBuf::from(read_string("BATTLE_ROYALE_CERT_PEM", "cert.pem")?);
    let private_key_pem = PathBuf::from(read_string("BATTLE_ROYALE_KEY_PEM", "key.pem")?);
    let recovery_directory = read_optional_path("BATTLE_ROYALE_RECOVERY_DIR")?;
    let route_prefix = BrowserRoutePrefix::new(read_string(
        "BATTLE_ROYALE_ROUTE_PREFIX",
        DEFAULT_ROUTE_PREFIX,
    )?)?;

    let (shutdown_sender, shutdown_receiver) = mpsc::channel(4);
    install_shutdown_forwarder(shutdown_sender)?;

    let transport_config = MatchHostWebTransportConfig {
        port,
        certificate_pem,
        private_key_pem,
        route_prefix,
        drain_grace: Duration::from_millis(drain_grace_ms),
        allowed_origins: None,
    };
    let status_config = MatchHostStatusConfig { port: status_port };

    if let Some(directory) = recovery_directory {
        let matches = build_match_entries(match_ids)?;
        let max_matches = matches.len();
        let prepared = prepare_match_host_for_recovery(
            matches,
            max_matches,
            reconnect_grace_ticks,
            MatchHostRecoveryConfig { directory },
        )
        .await?;
        serve_prepared_match_host_with_status_and_control_and_shutdown(
            prepared,
            RejectMatchControlService,
            transport_config,
            status_config,
            shutdown_receiver,
        )
        .await?;
    } else {
        let host = build_match_host(match_ids, reconnect_grace_ticks)?;
        serve_match_host_with_status_and_control_and_shutdown(
            host,
            RejectMatchControlService,
            transport_config,
            status_config,
            shutdown_receiver,
        )
        .await?;
    }

    Ok(())
}

fn parse_match_ids(value: &str) -> Result<Vec<BattleMatchId>, ConfigError> {
    if value.is_empty() {
        return Err(ConfigError(
            "BATTLE_ROYALE_MATCH_IDS must not be empty".to_owned(),
        ));
    }

    value
        .split(',')
        .map(|raw| {
            raw.parse::<u64>().map(BattleMatchId::new).map_err(|error| {
                ConfigError(format!(
                    "BATTLE_ROYALE_MATCH_IDS contains invalid match id {raw:?}: {error}"
                ))
            })
        })
        .collect()
}

fn read_string(name: &str, default: &str) -> Result<String, ConfigError> {
    match env::var(name) {
        Ok(value) => Ok(value),
        Err(env::VarError::NotPresent) => Ok(default.to_owned()),
        Err(error) => Err(ConfigError(format!("{name} is invalid: {error}"))),
    }
}

fn read_optional_path(name: &str) -> Result<Option<PathBuf>, ConfigError> {
    match env::var(name) {
        Ok(value) if value.is_empty() => Err(ConfigError(format!("{name} must not be empty"))),
        Ok(value) => Ok(Some(PathBuf::from(value))),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(ConfigError(format!("{name} is invalid: {error}"))),
    }
}

fn read_number<T>(name: &str, default: T) -> Result<T, ConfigError>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    match env::var(name) {
        Ok(value) => value
            .parse::<T>()
            .map_err(|error| ConfigError(format!("{name} is invalid: {error}"))),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(ConfigError(format!("{name} is invalid: {error}"))),
    }
}

#[cfg(unix)]
fn install_shutdown_forwarder(sender: mpsc::Sender<()>) -> Result<(), Box<dyn Error>> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut terminate = signal(SignalKind::terminate())?;
    tokio::spawn(async move {
        loop {
            let signal_received = tokio::select! {
                result = tokio::signal::ctrl_c() => result.is_ok(),
                received = terminate.recv() => received.is_some(),
            };
            if !signal_received || sender.send(()).await.is_err() {
                break;
            }
        }
    });
    Ok(())
}

#[cfg(not(unix))]
fn install_shutdown_forwarder(sender: mpsc::Sender<()>) -> Result<(), Box<dyn Error>> {
    tokio::spawn(async move {
        loop {
            if tokio::signal::ctrl_c().await.is_err() || sender.send(()).await.is_err() {
                break;
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ordered_match_ids() {
        assert_eq!(
            parse_match_ids("7,11,42").unwrap(),
            vec![
                BattleMatchId::new(7),
                BattleMatchId::new(11),
                BattleMatchId::new(42),
            ]
        );
    }

    #[test]
    fn rejects_empty_and_invalid_match_ids() {
        assert_eq!(
            parse_match_ids("").unwrap_err(),
            ConfigError("BATTLE_ROYALE_MATCH_IDS must not be empty".to_owned())
        );
        assert!(parse_match_ids("7,nope").is_err());
        assert!(parse_match_ids("7,").is_err());
    }
}
