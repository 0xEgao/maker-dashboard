use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use openswap::maker::MakerServerConfig;

use super::maker_pool::MakerId;
use super::MakerConfig;

/// Directory names under the dashboard data dir that are never makers.
const NON_MAKER_DIRS: [&str; 2] = ["tor", "taker"];

/// Returns the path to a maker's core config file: `<data_dir>/config.toml`.
/// This is the same location makerd uses (`data_dir.join("config.toml")`).
pub fn maker_config_toml_path(data_dir: &Path) -> PathBuf {
    data_dir.join("config.toml")
}

/// Writes a maker's static fields to `<data_dir>/config.toml` via core's own
/// `MakerServerConfig::write_to_file`, matching makerd's write-back behavior.
pub fn write_maker_toml(config: &MakerConfig) -> Result<()> {
    let Some(data_dir) = &config.data_directory else {
        return Ok(());
    };
    let path = maker_config_toml_path(data_dir);
    let server_config = MakerServerConfig {
        network_port: config.network_port,
        rpc_port: config.rpc_port,
        socks_port: config.socks_port,
        control_port: config.control_port,
        tor_auth_password: config.tor_auth.clone().unwrap_or_default(),
        min_swap_amount: config.min_swap_amount,
        fidelity_amount: config.fidelity_amount,
        fidelity_timelock: config.fidelity_timelock,
        fidelity_feerate: config.fidelity_feerate,
        base_fee: config.base_fee,
        amount_relative_fee_pct: config.amount_relative_fee_pct,
        time_relative_fee_pct: config.time_relative_fee_pct,
        required_confirms: config.required_confirms,
        ..Default::default()
    };
    server_config
        .write_to_file(&path)
        .with_context(|| format!("Failed to write maker config file: {}", path.display()))
}

/// Loads a maker's config from `<data_dir>/config.toml` via core's own
/// `MakerServerConfig::new` (which auto-creates a default file if missing,
/// like makerd). The wallet name is recovered from `<data_dir>/wallets/`.
pub fn read_maker_toml(id: &MakerId, data_dir: &Path) -> Result<MakerConfig> {
    let path = maker_config_toml_path(data_dir);
    let c = MakerServerConfig::new(Some(&path))
        .map_err(|e| anyhow::anyhow!("Failed to parse {}: {:?}", path.display(), e))?;
    Ok(MakerConfig {
        data_directory: Some(data_dir.to_path_buf()),
        tor_auth: if c.tor_auth_password.is_empty() {
            None
        } else {
            Some(c.tor_auth_password)
        },
        wallet_name: Some(read_wallet_name(data_dir, id)),
        network_port: c.network_port,
        rpc_port: c.rpc_port,
        socks_port: c.socks_port,
        control_port: c.control_port,
        min_swap_amount: c.min_swap_amount,
        fidelity_amount: c.fidelity_amount,
        fidelity_timelock: c.fidelity_timelock,
        fidelity_feerate: c.fidelity_feerate,
        base_fee: c.base_fee,
        amount_relative_fee_pct: c.amount_relative_fee_pct,
        time_relative_fee_pct: c.time_relative_fee_pct,
        required_confirms: c.required_confirms,
    })
}

/// Reads the wallet name from `<data_dir>/wallets/` — the name of its single
/// entry, as created by core. Falls back to the maker id when no wallet
/// exists yet (maker registered but never initialized).
fn read_wallet_name(data_dir: &Path, id: &MakerId) -> String {
    let wallets_dir = data_dir.join("wallets");
    if let Ok(mut entries) = fs::read_dir(&wallets_dir) {
        if let Some(Ok(entry)) = entries.next() {
            if let Ok(name) = entry.file_name().into_string() {
                return name;
            }
        }
    }
    id.clone()
}

/// Discovers makers and loads their config.toml files under the dashboard
/// data dir. Maker dirs contain core-managed files only; there is no
/// dashboard-specific per-maker state.
pub struct PersistenceManager {
    pub config_dir: PathBuf,
}

impl PersistenceManager {
    /// Creates a new PersistenceManager, ensuring the data directory exists.
    pub fn new(config_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&config_dir).with_context(|| {
            format!(
                "Failed to create dashboard data directory: {}",
                config_dir.display()
            )
        })?;

        Ok(Self { config_dir })
    }

    /// Scans `config_dir` for maker data dirs (subdirectories containing a
    /// `config.toml`), skipping dashboard-owned dirs. Each maker's config is
    /// loaded from its TOML; entries that fail to load are logged and skipped.
    pub fn discover_makers(&self) -> Vec<(MakerId, MakerConfig)> {
        let mut makers = Vec::new();
        let entries = match fs::read_dir(&self.config_dir) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!(
                    "Failed to scan data directory {} for makers: {}",
                    self.config_dir.display(),
                    e
                );
                return makers;
            }
        };

        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }
            let Ok(id) = entry.file_name().into_string() else {
                continue;
            };
            if id.starts_with('.') || NON_MAKER_DIRS.contains(&id.as_str()) {
                continue;
            }
            let data_dir = entry.path();
            if !maker_config_toml_path(&data_dir).exists() {
                continue;
            }
            match read_maker_toml(&id, &data_dir) {
                Ok(config) => makers.push((id, config)),
                Err(e) => {
                    tracing::warn!("Skipping maker '{}': {}", id, e);
                }
            }
        }

        makers
    }
}
