pub mod maker_pool;
pub mod message;
pub mod persistence;

use std::collections::HashMap;
use std::net::TcpListener;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Arc;

use crate::tor_manager::TorManager;
use crate::utils::log_writer::MakerLogWriter;
use anyhow::{anyhow, Result};
use maker_pool::{MakerId, MakerPool};
use message::{MessageRequest, MessageResponse};
use openswap::bitcoin::Network;
use openswap::bitcoind::bitcoincore_rpc::Auth;
use openswap::maker::{MakerServer, MakerServerConfig};
use openswap::wallet::{BackendConfig, CoreRpcConfig, ElectrumConfig};
use persistence::PersistenceManager;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Default Electrum server makers use for chain data when running with the
/// Electrum backend. Shown prefilled on the startup screen; user-configurable.
pub const ELECTRUM_URL: &str = "ssl://electrum.citadelfoss.xyz:50002";

/// Default transaction fee rate (sat/vB), matching openswap core's `MIN_FEE_RATE`.
pub const DEFAULT_FEE_RATE: f64 = 2.0;

/// Chain-data backend a maker uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MakerBackend {
    /// Local Bitcoin Core node via RPC + ZMQ.
    Bitcoind,
    /// Hardcoded Electrum server ([`ELECTRUM_URL`]); no local node needed.
    Electrum,
}

/// Configuration for creating a new maker.
///
/// All fields are persisted in the maker's `<data_dir>/config.toml` via
/// openswap core's own config writer — the same file makerd uses. The wallet
/// password is NOT part of this struct: it is accepted as a separate parameter
/// where needed, zeroized immediately after wallet load, and never persisted.
#[derive(Debug, Clone)]
pub struct MakerConfig {
    /// Optional data directory. Default: `<dashboard data dir>/<id>`
    pub data_directory: Option<PathBuf>,
    /// Optional Tor authentication string
    pub tor_auth: Option<String>,
    /// Optional wallet name. Default: the maker id.
    pub wallet_name: Option<String>,
    pub network_port: u16,
    pub rpc_port: u16,
    pub socks_port: u16,
    pub control_port: u16,
    pub min_swap_amount: u64,
    pub fidelity_amount: u64,
    pub fidelity_timelock: u32,
    /// Fidelity bond transaction fee rate (sat/vB), consumed by core.
    pub fidelity_feerate: f64,
    pub required_confirms: u32,
    pub base_fee: u64,
    pub amount_relative_fee_pct: f64,
    pub time_relative_fee_pct: f64,
}

impl Default for MakerConfig {
    fn default() -> Self {
        Self {
            data_directory: None,
            tor_auth: None,
            wallet_name: None,
            network_port: 6102,
            rpc_port: 6103,
            socks_port: 9050,
            control_port: 9051,
            min_swap_amount: 10000,
            fidelity_amount: 10000,
            fidelity_timelock: 15000,
            fidelity_feerate: DEFAULT_FEE_RATE,
            required_confirms: 1,
            base_fee: 1000,
            amount_relative_fee_pct: 0.0025,
            time_relative_fee_pct: 0.0001,
        }
    }
}

/// Chain-data backend selected on the startup screen. Runtime-only: entered by
/// the user at every dashboard start, held in memory, applied to all makers.
/// Never persisted.
#[derive(Debug, Clone)]
pub struct RuntimeBackend {
    pub kind: MakerBackend,
    /// Bitcoin Core RPC address (bitcoind backend only)
    pub rpc: String,
    /// Bitcoin Core ZMQ address (bitcoind backend only)
    pub zmq: String,
    /// Bitcoin Core RPC username (bitcoind backend only)
    pub rpc_user: String,
    /// Bitcoin Core RPC password (bitcoind backend only)
    pub rpc_password: String,
    /// Electrum server URL (electrum backend only). Default: [`ELECTRUM_URL`].
    pub electrum_url: String,
}

impl Default for RuntimeBackend {
    fn default() -> Self {
        Self {
            kind: MakerBackend::Electrum,
            rpc: "127.0.0.1:38332".to_string(),
            zmq: "tcp://127.0.0.1:28332".to_string(),
            rpc_user: "user".to_string(),
            rpc_password: "password".to_string(),
            electrum_url: ELECTRUM_URL.to_string(),
        }
    }
}

/// Operational state of a maker
#[derive(Debug, Clone, PartialEq)]
pub enum MakerState {
    Running,
    Stopped,
}

/// Full information about a registered maker
#[derive(Debug, Clone)]
pub struct MakerInfo {
    pub id: MakerId,
    pub state: MakerState,
    pub config: MakerConfig,
}

/// High-level manager for creating and interacting with makers
pub struct MakerManager {
    pool: MakerPool,
    /// Maker configs keyed by maker ID, discovered from per-maker config.toml
    /// files under the dashboard data dir.
    configs: HashMap<MakerId, MakerConfig>,
    /// Handles maker discovery and config.toml IO
    persistence: PersistenceManager,
    /// Backend selected on the startup screen. `None` until the user submits it;
    /// runtime-only, never persisted.
    backend: Option<RuntimeBackend>,
    /// Running bitcoind child process spawned by the dashboard, if any
    bitcoind_process: Option<std::process::Child>,
    /// Network bitcoind was started on (e.g. "regtest", "signet")
    bitcoind_network: Option<String>,
    #[allow(dead_code)]
    tor_manager: TorManager,
}

impl MakerManager {
    const DEFAULT_WALLET_NAME: &'static str = "maker-wallet";
    const LEGACY_RPC_WALLET_NAME: &'static str = "random";

    /// Creates a new MakerManager rooted at the given data directory.
    ///
    /// Makers are discovered by scanning the data dir for per-maker
    /// `config.toml` files and registered as stopped. They are initialized and
    /// auto-started once the backend is set via [`MakerManager::set_backend`].
    /// `quiet_tor` silences the embedded Tor's console logging (used when the
    /// stdout log filter is off).
    pub fn new(config_dir: PathBuf, quiet_tor: bool) -> Result<Self> {
        let tor_manager = TorManager::detect_or_start(&config_dir, quiet_tor).unwrap_or_else(|e| {
            tracing::warn!(
                "Tor could not be started: {}. Tor-dependent makers will fail to start.",
                e
            );
            TorManager::noop()
        });
        Self::new_with_tor(config_dir, tor_manager)
    }

    /// Creates a MakerManager without starting or detecting Tor. Use in tests only.
    #[allow(dead_code)]
    pub fn new_for_testing(config_dir: PathBuf) -> Result<Self> {
        Self::new_with_tor(config_dir, TorManager::noop())
    }

    fn new_with_tor(config_dir: PathBuf, tor_manager: TorManager) -> Result<Self> {
        let persistence = PersistenceManager::new(config_dir)?;
        let configs: HashMap<MakerId, MakerConfig> = persistence
            .discover_makers()
            .into_iter()
            .map(|(id, config)| {
                tracing::info!("Discovered maker '{}' from config.toml", id);
                (id, config)
            })
            .collect();

        Ok(Self {
            pool: MakerPool::new(),
            configs,
            persistence,
            backend: None,
            bitcoind_process: None,
            bitcoind_network: None,
            tor_manager,
        })
    }

    /// Returns the currently configured backend, if the startup screen has
    /// been submitted.
    pub fn backend(&self) -> Option<&RuntimeBackend> {
        self.backend.as_ref()
    }

    /// Sets the backend for all makers (runtime-only, never persisted), then
    /// initializes every registered maker and auto-starts those whose wallet
    /// opens without a password. Makers that fail to initialize or start are
    /// left stopped; the UI can start them later with a wallet password.
    pub fn set_backend(&mut self, backend: RuntimeBackend) {
        self.backend = Some(backend);
        self.init_and_auto_start_all();
    }

    /// Retries initialization and auto-start for all stopped makers, e.g. after
    /// bitcoind becomes available. No-op if the backend is not set yet.
    pub fn retry_stopped_makers(&mut self) {
        if self.backend.is_some() {
            self.init_and_auto_start_all();
        }
    }

    fn init_and_auto_start_all(&mut self) {
        let ids: Vec<MakerId> = self.configs.keys().cloned().collect();
        for id in ids {
            if !self.pool.contains(&id) {
                let config = self.configs.get(&id).cloned().expect("key from configs");
                if let Err(e) = self.create_maker_internal(id.clone(), config, None) {
                    tracing::warn!("Maker '{}' failed to initialize: {}. Left stopped.", id, e);
                    continue;
                }
            }
            match self.start_maker(&id, None) {
                Ok(()) => tracing::info!("Maker '{}' auto-started", id),
                Err(MakerManagerError::AlreadyRunning(_)) => {}
                Err(e) => tracing::warn!("Maker '{}' did not auto-start: {}", id, e),
            }
        }
    }

    /// Returns the default data directory for a maker.
    /// Defaults to `<dashboard data dir>/{id}`. With the default data dir and
    /// id `maker` this is exactly makerd's default dir `~/.openswap/maker`,
    /// so that maker is fully interoperable with the CLI.
    fn default_maker_data_dir(&self, id: &MakerId) -> PathBuf {
        self.persistence.config_dir.join(id)
    }

    fn normalize_wallet_name(id: &MakerId, wallet_name: Option<String>) -> Option<String> {
        match wallet_name {
            Some(name) => {
                let trimmed = name.trim();
                if trimmed.is_empty()
                    || trimmed == Self::DEFAULT_WALLET_NAME
                    || trimmed == Self::LEGACY_RPC_WALLET_NAME
                {
                    Some(id.clone())
                } else {
                    Some(trimmed.to_string())
                }
            }
            None => Some(id.clone()),
        }
    }

    fn normalize_config(id: &MakerId, mut config: MakerConfig) -> MakerConfig {
        config.wallet_name = Self::normalize_wallet_name(id, config.wallet_name);
        config
    }

    fn panic_payload_to_string(payload: Box<dyn std::any::Any + Send>) -> String {
        if let Some(message) = payload.downcast_ref::<String>() {
            message.clone()
        } else if let Some(message) = payload.downcast_ref::<&'static str>() {
            (*message).to_string()
        } else {
            "unknown panic".to_string()
        }
    }

    fn init_maker_server(config: MakerServerConfig) -> Result<MakerServer> {
        let init_result = catch_unwind(AssertUnwindSafe(|| MakerServer::init(config)));

        init_result
            .map_err(|panic| {
                anyhow!(
                    "Maker server initialization panicked: {}",
                    Self::panic_payload_to_string(panic)
                )
            })?
            .map_err(|e| anyhow!("Failed to initialize maker server: {e:?}"))
    }

    fn infer_network(&self) -> Network {
        match self.bitcoind_network.as_deref() {
            Some("regtest") => Network::Regtest,
            Some("signet") => Network::Signet,
            Some("testnet") => Network::Testnet,
            Some("mainnet") => Network::Bitcoin,
            _ => match self
                .backend
                .as_ref()
                .and_then(|b| b.rpc.rsplit(':').next())
                .and_then(|port| port.parse::<u16>().ok())
            {
                Some(18443) => Network::Regtest,
                Some(38332) => Network::Signet,
                Some(18332) => Network::Testnet,
                Some(8332) => Network::Bitcoin,
                _ => Network::Signet,
            },
        }
    }

    /// Internal: write the maker's config.toml, initialise the maker, and
    /// register it in the pool. Does NOT start the openswap server.
    ///
    /// `password` is the wallet-opening password for encrypted wallets. It is
    /// used only during `MakerServer::init` and zeroized on return; it is never
    /// stored in dashboard state or on disk. (Note: openswap core currently
    /// retains its own copy inside the `MakerServer` for the process lifetime —
    /// that is core's behavior, outside our control.)
    fn create_maker_internal(
        &mut self,
        id: MakerId,
        config: MakerConfig,
        password: Option<Zeroizing<String>>,
    ) -> Result<()> {
        let mut config = Self::normalize_config(&id, config);
        if config.data_directory.is_none() {
            let maker_dir = self.default_maker_data_dir(&id);
            std::fs::create_dir_all(&maker_dir)?;
            config.data_directory = Some(maker_dir);
        }

        // Write static fields to <data_dir>/config.toml via core's writer,
        // exactly as makerd does.
        persistence::write_maker_toml(&config)?;

        let backend_input = self
            .backend
            .clone()
            .ok_or_else(|| anyhow!("Backend is not configured yet"))?;
        let backend = match backend_input.kind {
            MakerBackend::Electrum => BackendConfig::Electrum(ElectrumConfig {
                url: backend_input.electrum_url.clone(),
                ..Default::default()
            }),
            MakerBackend::Bitcoind => BackendConfig::CoreRpc(CoreRpcConfig {
                url: backend_input.rpc.clone(),
                auth: Auth::UserPass(backend_input.rpc_user, backend_input.rpc_password),
                wallet_name: config.wallet_name.clone().unwrap_or_else(|| id.clone()),
                zmq_addr: backend_input.zmq.clone(),
            }),
        };

        let data_dir = config
            .data_directory
            .clone()
            .expect("maker data directory is initialized above");
        MakerLogWriter::register_maker(&id, &data_dir, config.network_port)?;
        let wallet_name = config.wallet_name.clone().unwrap_or_else(|| id.clone());
        let network = self.infer_network();
        let server_config = MakerServerConfig {
            data_dir,
            network_port: config.network_port,
            rpc_port: config.rpc_port,
            base_fee: config.base_fee,
            amount_relative_fee_pct: config.amount_relative_fee_pct,
            time_relative_fee_pct: config.time_relative_fee_pct,
            min_swap_amount: config.min_swap_amount,
            required_confirms: config.required_confirms,
            supported_protocols: MakerServerConfig::default().supported_protocols,
            fidelity_amount: config.fidelity_amount,
            fidelity_timelock: config.fidelity_timelock,
            fidelity_feerate: config.fidelity_feerate,
            network,
            wallet_name,
            backend,
            control_port: config.control_port,
            socks_port: config.socks_port,
            tor_auth_password: config.tor_auth.clone().unwrap_or_default(),
            // Moved into core's config; our own `Zeroizing` copy is zeroed on
            // return from this function.
            password: password.as_ref().map(|p| p.as_str().to_string()),
            nostr_relays: MakerServerConfig::default().nostr_relays,
        };
        let maker = Arc::new(Self::init_maker_server(server_config)?);
        self.pool
            .spawn_maker(id.clone(), maker, config.network_port)?;

        self.configs.insert(id, config);
        Ok(())
    }

    pub fn get_maker_config(&self, id: &str) -> Option<&MakerConfig> {
        self.configs.get(id)
    }

    #[cfg(test)]
    pub(crate) fn insert_config_for_testing(&mut self, id: MakerId, config: MakerConfig) {
        let config = Self::normalize_config(&id, config);
        self.configs.insert(id, config);
    }

    /// Creates and registers a new maker (init + message loop only, NOT started).
    /// Use `start_maker` to start the openswap server.
    ///
    /// `password` sets wallet encryption for a new wallet (or opens an existing
    /// encrypted wallet). It is zeroized after wallet load and never persisted.
    pub fn create_maker(
        &mut self,
        id: MakerId,
        config: MakerConfig,
        password: Option<Zeroizing<String>>,
    ) -> Result<()> {
        self.create_maker_internal(id, config, password)
    }

    pub fn is_port_in_use(&self, port: u16, exclude_id: Option<&str>) -> bool {
        self.configs.iter().any(|(id, cfg)| {
            if let Some(excl) = exclude_id {
                if id.as_str() == excl {
                    return false;
                }
            }
            cfg.network_port == port || cfg.rpc_port == port
        })
    }

    pub fn is_local_port_available(port: u16) -> bool {
        TcpListener::bind(("127.0.0.1", port)).is_ok()
    }

    fn find_available_port(
        &self,
        start_port: u16,
        exclude_id: Option<&str>,
        reserved_ports: &[u16],
    ) -> Result<u16> {
        for port in start_port..=u16::MAX {
            if reserved_ports.contains(&port) {
                continue;
            }
            if self.is_port_in_use(port, exclude_id) {
                continue;
            }
            if Self::is_local_port_available(port) {
                return Ok(port);
            }
        }

        Err(anyhow!(
            "Failed to find an available port starting from {}",
            start_port
        ))
    }

    pub fn assign_available_maker_ports(
        &self,
        requested_network_port: u16,
        requested_rpc_port: u16,
        socks_port: u16,
        control_port: u16,
        exclude_id: Option<&str>,
    ) -> Result<(u16, u16)> {
        let network_port = self.find_available_port(
            requested_network_port,
            exclude_id,
            &[socks_port, control_port],
        )?;
        let rpc_port = self.find_available_port(
            requested_rpc_port,
            exclude_id,
            &[socks_port, control_port, network_port],
        )?;

        Ok((network_port, rpc_port))
    }

    /// Starts the openswap server for a registered maker.
    /// The maker must already be created (via `create_maker`).
    ///
    /// `password` is the wallet-opening password for encrypted wallets, used
    /// only if the maker needs re-initialization. It is zeroized after wallet
    /// load and never stored. Makers whose wallet opens without a password (or
    /// that are already initialized) can pass `None`.
    pub fn start_maker(
        &mut self,
        id: &MakerId,
        password: Option<Zeroizing<String>>,
    ) -> Result<(), MakerManagerError> {
        if !self.configs.contains_key(id) {
            return Err(MakerManagerError::NotFound(id.clone()));
        }
        if self.pool.is_server_running(id) {
            return Err(MakerManagerError::AlreadyRunning(id.clone()));
        }
        if !self.pool.contains(id) {
            // Pool entry was lost (e.g. init failed at startup because the
            // wallet needed a password or bitcoind was down). Re-initialize
            // now using the current config.
            let config = self.configs.get(id).cloned().expect("checked above");
            self.create_maker_internal(id.clone(), config, password)
                .map_err(MakerManagerError::Other)?;
        }
        self.pool.start_server(id).map_err(MakerManagerError::Other)
    }

    /// Stops the openswap server for a running maker.
    /// The maker remains registered — wallet queries still work.
    pub fn stop_maker(&mut self, id: &MakerId) -> Result<(), MakerManagerError> {
        if !self.configs.contains_key(id) {
            return Err(MakerManagerError::NotFound(id.clone()));
        }
        if !self.pool.is_server_running(id) {
            return Err(MakerManagerError::AlreadyStopped(id.clone()));
        }
        self.pool.stop_server(id).map_err(MakerManagerError::Other)
    }

    /// Returns full info (id, state, config) for a maker
    pub fn get_maker_info(&self, id: &MakerId) -> Option<MakerInfo> {
        self.configs.get(id).map(|config| MakerInfo {
            id: id.clone(),
            state: if self.pool.is_server_running(id) {
                MakerState::Running
            } else {
                MakerState::Stopped
            },
            config: config.clone(),
        })
    }

    /// Sends a ping to a maker to check connectivity
    pub async fn ping(&self, id: &MakerId) -> Result<()> {
        match self.request(id, MessageRequest::Ping).await? {
            MessageResponse::Pong => Ok(()),
            MessageResponse::ServerError(e) => Err(anyhow!(e)),
            _ => Err(anyhow!("Unexpected response")),
        }
    }

    /// Gets all UTXOs from a maker's wallet
    pub async fn get_utxos(&self, id: &MakerId) -> Result<MessageResponse> {
        self.request(id, MessageRequest::Utxo).await
    }

    /// Gets swap UTXOs from a maker's wallet
    pub async fn get_swap_utxos(&self, id: &MakerId) -> Result<MessageResponse> {
        self.request(id, MessageRequest::SwapUtxo).await
    }

    /// Gets contract UTXOs from a maker's wallet
    pub async fn get_contract_utxos(&self, id: &MakerId) -> Result<MessageResponse> {
        self.request(id, MessageRequest::ContractUtxo).await
    }

    /// Gets fidelity UTXOs from a maker's wallet
    pub async fn get_fidelity_utxos(&self, id: &MakerId) -> Result<MessageResponse> {
        self.request(id, MessageRequest::FidelityUtxo).await
    }

    /// Gets swept (completed) incoming swap coin UTXOs from a maker's wallet
    pub async fn get_swept_swap_utxos(&self, id: &MakerId) -> Result<MessageResponse> {
        self.request(id, MessageRequest::SweptSwapUtxo).await
    }

    /// Gets the balances from a maker's wallet
    pub async fn get_balances(&self, id: &MakerId) -> Result<MessageResponse> {
        self.request(id, MessageRequest::Balances).await
    }

    /// Generates a new address from a maker's wallet
    pub async fn get_new_address(&self, id: &MakerId) -> Result<MessageResponse> {
        self.request(id, MessageRequest::NewAddress).await
    }

    /// Sends funds to an address from a maker's wallet
    pub async fn send_to_address(
        &self,
        id: &MakerId,
        address: String,
        amount: u64,
        feerate: f64,
    ) -> Result<MessageResponse> {
        self.pool
            .request(
                id,
                MessageRequest::SendToAddress {
                    address,
                    amount,
                    feerate,
                },
            )
            .await
    }

    /// Gets the Tor address of a maker
    pub async fn get_tor_address(&self, id: &MakerId) -> Result<MessageResponse> {
        self.request(id, MessageRequest::GetTorAddress).await
    }

    /// Gets the data directory of a maker
    pub async fn get_data_dir(&self, id: &MakerId) -> Result<MessageResponse> {
        self.request(id, MessageRequest::GetDataDir).await
    }

    /// Lists fidelity bonds of a maker
    pub async fn list_fidelity(&self, id: &MakerId) -> Result<MessageResponse> {
        self.request(id, MessageRequest::ListFidelity).await
    }

    /// Syncs a maker's wallet with the blockchain
    pub async fn sync_wallet(&self, id: &MakerId) -> Result<MessageResponse> {
        self.request(id, MessageRequest::SyncWallet).await
    }

    /// Verifies the deniability proof for a specific swap
    pub async fn verify_deniability(&self, id: &MakerId, swap_id: &str) -> Result<MessageResponse> {
        self.pool
            .request(
                id,
                MessageRequest::VerifyDeniability {
                    swap_id: swap_id.to_string(),
                },
            )
            .await
    }

    /// Sends a raw request to a maker
    pub async fn request(&self, id: &MakerId, req: MessageRequest) -> Result<MessageResponse> {
        self.pool.request(id, req).await
    }

    /// Updates a maker's configuration, re-initialising the maker with the new
    /// settings. Rewrites config.toml via core's writer.
    ///
    /// `password` is the wallet-opening password, needed because the wallet is
    /// re-opened during re-init. Zeroized after use, never stored.
    pub fn update_config(
        &mut self,
        id: &MakerId,
        config: MakerConfig,
        password: Option<Zeroizing<String>>,
    ) -> Result<()> {
        let config = Self::normalize_config(id, config);
        let previous = self
            .configs
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow!("Maker with id '{id}' not found"))?;
        let was_running = self.pool.is_server_running(id);

        // If only stopped, we can just update the config without re-init
        // But since config changes may affect wallet/RPC, we re-init
        // Stop server if running
        if was_running {
            let _ = self.pool.stop_server(id);
        }

        // Remove from pool entirely (need to re-init with new config)
        self.pool.remove_maker(id);
        self.configs.remove(id);

        // Re-create with new config (rewrites config.toml via core's writer)
        match self.create_maker_internal(id.clone(), config, password.clone()) {
            Ok(()) => {
                // Restart server if it was running before
                if was_running {
                    if let Err(e) = self.pool.start_server(id) {
                        tracing::warn!(
                            "Maker '{}' re-created but failed to restart server: {}",
                            id,
                            e
                        );
                    }
                }
                Ok(())
            }
            Err(e) => {
                // Rollback: restore previous config
                tracing::error!(
                    "Failed to re-create maker '{}' with new config: {}. Rolling back.",
                    id,
                    e
                );
                if let Err(restore_err) = self.create_maker_internal(id.clone(), previous, password)
                {
                    return Err(anyhow!(
                        "Failed to update maker '{id}': {e}; rollback also failed: {restore_err}"
                    ));
                }
                if was_running {
                    let _ = self.pool.start_server(id);
                }
                Err(e)
            }
        }
    }

    /// Checks if a maker exists (registered, regardless of server state)
    pub fn has_maker(&self, id: &MakerId) -> bool {
        self.configs.contains_key(id)
    }

    /// Returns a clone of the config for a maker, if it exists
    pub fn get_config(&self, id: &MakerId) -> Option<MakerConfig> {
        self.configs.get(id).cloned()
    }

    /// Returns the number of registered makers (running + stopped)
    pub fn maker_count(&self) -> usize {
        self.configs.len()
    }

    /// Returns a list of all registered maker IDs (running + stopped)
    pub fn list_makers(&self) -> Vec<&MakerId> {
        self.configs.keys().collect()
    }

    /// Removes a maker entirely (stops server, removes from pool, unregisters it).
    /// The maker's data dir (config.toml, wallets) is left untouched — same as makerd.
    pub fn remove_maker(&mut self, id: &MakerId) -> bool {
        self.pool.remove_maker(id);
        MakerLogWriter::unregister_maker(id);
        self.configs.remove(id).is_some()
    }

    /// Restarts a maker by stopping and re-initializing its server.
    ///
    /// `MakerServer::start_server` owns background services such as the
    /// watchtower. Stopping the server shuts those services down permanently,
    /// so starting the same `MakerServer` instance again would leave it in
    /// recovery-only mode. Re-create the instance from the persisted config to
    /// give the restarted server fresh background services.
    pub fn restart_maker(&mut self, id: &MakerId) -> Result<(), MakerManagerError> {
        let config = self
            .configs
            .get(id)
            .cloned()
            .ok_or_else(|| MakerManagerError::NotFound(id.clone()))?;
        if self.pool.is_server_running(id) {
            self.pool
                .stop_server(id)
                .map_err(MakerManagerError::Other)?;
        }
        self.pool.remove_maker(id);
        self.create_maker_internal(id.clone(), config, false)
            .map_err(MakerManagerError::Other)?;
        self.pool.start_server(id).map_err(MakerManagerError::Other)
    }

    pub fn is_server_running(&mut self, id: &MakerId) -> bool {
        self.pool.is_server_running(id)
    }

    /// Returns the log file path for a given maker ID.
    pub fn log_file_path(&self, maker_id: &str) -> std::path::PathBuf {
        self.configs
            .get(maker_id)
            .and_then(|config| config.data_directory.clone())
            .unwrap_or_else(|| self.default_maker_data_dir(&maker_id.to_string()))
            .join("debug.log")
    }

    /// Starts a bitcoind process in the given network mode ("regtest" or "signet").
    /// Binary path is read from the `BITCOIND_EXE` environment variable; defaults to "bitcoind" on $PATH.
    pub fn start_bitcoind(&mut self, network: String) -> Result<()> {
        // Clean up a previously-exited process handle, if any
        if let Some(ref mut child) = self.bitcoind_process {
            match child.try_wait() {
                Ok(Some(_)) => {
                    self.bitcoind_process = None;
                    self.bitcoind_network = None;
                }
                Ok(None) => return Err(anyhow!("bitcoind is already running")),
                Err(e) => return Err(anyhow!("Failed to check bitcoind process status: {e}")),
            }
        }

        if !matches!(network.as_str(), "regtest" | "signet") {
            return Err(anyhow!(
                "Invalid network '{network}'. Must be 'regtest' or 'signet'."
            ));
        }

        let exe = std::env::var("BITCOIND_EXE").unwrap_or_else(|_| "bitcoind".to_string());
        let child = std::process::Command::new(&exe)
            .arg(format!("-{network}"))
            .arg("-server")
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn bitcoind ('{exe}'): {e}"))?;

        self.bitcoind_process = Some(child);
        self.bitcoind_network = Some(network);
        Ok(())
    }

    /// Extracts the dashboard-managed bitcoind child handle for async-safe shutdown.
    /// Returns `None` if no process is currently tracked as running.
    /// The caller is responsible for calling `kill()` and `wait()` on the returned handle.
    pub fn take_bitcoind(&mut self) -> Option<std::process::Child> {
        let child = self.bitcoind_process.take()?;
        self.bitcoind_network = None;
        Some(child)
    }

    /// Returns how Tor was obtained: "system", "host", or "docker"
    pub fn tor_source(&self) -> &'static str {
        self.tor_manager.source_label()
    }

    /// Returns `(running, network)` for the dashboard-managed bitcoind process.
    pub fn bitcoind_status(&mut self) -> (bool, Option<String>) {
        if let Some(ref mut child) = self.bitcoind_process {
            match child.try_wait() {
                Ok(Some(_)) => {
                    // Process exited on its own
                    self.bitcoind_process = None;
                    self.bitcoind_network = None;
                    (false, None)
                }
                Ok(None) => (true, self.bitcoind_network.clone()),
                Err(_) => (false, None),
            }
        } else {
            (false, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use super::{MakerConfig, MakerManager};

    #[test]
    fn normalize_wallet_name_defaults_to_maker_id() {
        assert_eq!(
            MakerManager::normalize_wallet_name(&"maker101".to_string(), None),
            Some("maker101".to_string())
        );
        assert_eq!(
            MakerManager::normalize_wallet_name(&"maker101".to_string(), Some("".to_string())),
            Some("maker101".to_string())
        );
        assert_eq!(
            MakerManager::normalize_wallet_name(
                &"maker101".to_string(),
                Some(" maker-wallet ".to_string())
            ),
            Some("maker101".to_string())
        );
    }

    #[test]
    fn normalize_wallet_name_preserves_custom_values() {
        assert_eq!(
            MakerManager::normalize_wallet_name(
                &"maker101".to_string(),
                Some("custom-wallet".to_string())
            ),
            Some("custom-wallet".to_string())
        );
    }

    #[test]
    fn normalize_config_updates_wallet_name() {
        let config = MakerConfig {
            wallet_name: Some("random".to_string()),
            ..MakerConfig::default()
        };

        let normalized = MakerManager::normalize_config(&"maker101".to_string(), config);
        assert_eq!(normalized.wallet_name.as_deref(), Some("maker101"));
    }

    #[test]
    fn assign_available_maker_ports_skips_taken_local_ports() {
        let config_dir =
            std::env::temp_dir().join(format!("maker-manager-port-test-{}", std::process::id()));
        if config_dir.exists() {
            std::fs::remove_dir_all(&config_dir).unwrap();
        }
        std::fs::create_dir_all(&config_dir).unwrap();

        let manager = MakerManager::new_for_testing(config_dir).unwrap();
        let network_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let rpc_listener = TcpListener::bind("127.0.0.1:0").unwrap();

        let requested_network_port = network_listener.local_addr().unwrap().port();
        let requested_rpc_port = rpc_listener.local_addr().unwrap().port();

        let (network_port, rpc_port) = manager
            .assign_available_maker_ports(
                requested_network_port,
                requested_rpc_port,
                9050,
                9051,
                None,
            )
            .unwrap();

        assert_ne!(network_port, requested_network_port);
        assert_ne!(rpc_port, requested_rpc_port);
        assert_ne!(network_port, rpc_port);
        assert_ne!(network_port, 9050);
        assert_ne!(network_port, 9051);
        assert_ne!(rpc_port, 9050);
        assert_ne!(rpc_port, 9051);
    }

    #[test]
    fn config_toml_roundtrip_and_discovery() {
        let config_dir =
            std::env::temp_dir().join(format!("maker-manager-toml-test-{}", std::process::id()));
        if config_dir.exists() {
            std::fs::remove_dir_all(&config_dir).unwrap();
        }
        let maker_dir = config_dir.join("maker1");
        std::fs::create_dir_all(&maker_dir).unwrap();

        // Write a maker config to <maker_dir>/config.toml via core's writer.
        let config = MakerConfig {
            data_directory: Some(maker_dir.clone()),
            base_fee: 4200,
            fidelity_feerate: 3.5,
            ..MakerConfig::default()
        };
        super::persistence::write_maker_toml(&config).unwrap();
        assert!(maker_dir.join("config.toml").exists());

        // A fresh manager discovers the maker from the TOML alone.
        let manager = MakerManager::new_for_testing(config_dir.clone()).unwrap();
        let loaded = manager.get_maker_config("maker1").unwrap();
        assert_eq!(loaded.base_fee, 4200);
        assert_eq!(loaded.fidelity_feerate, 3.5);
        assert_eq!(
            loaded.wallet_name.as_deref(),
            Some("maker1"),
            "wallet name falls back to the maker id without a wallets/ dir"
        );

        std::fs::remove_dir_all(&config_dir).unwrap();
    }

    #[test]
    fn hand_edited_config_toml_wins_on_reload() {
        let config_dir = std::env::temp_dir().join(format!(
            "maker-manager-toml-edit-test-{}",
            std::process::id()
        ));
        if config_dir.exists() {
            std::fs::remove_dir_all(&config_dir).unwrap();
        }
        let maker_dir = config_dir.join("maker1");
        std::fs::create_dir_all(&maker_dir).unwrap();

        let config = MakerConfig {
            data_directory: Some(maker_dir.clone()),
            ..MakerConfig::default()
        };
        super::persistence::write_maker_toml(&config).unwrap();

        // Simulate a hand edit (or a makerd run) against the same data dir.
        let toml_path = maker_dir.join("config.toml");
        let raw = std::fs::read_to_string(&toml_path).unwrap();
        let edited = raw.replacen("base_fee = 1000", "base_fee = 7777", 1);
        assert_ne!(raw, edited);
        std::fs::write(&toml_path, edited).unwrap();

        let manager = MakerManager::new_for_testing(config_dir.clone()).unwrap();
        let loaded = manager.get_maker_config("maker1").unwrap();
        assert_eq!(loaded.base_fee, 7777);

        std::fs::remove_dir_all(&config_dir).unwrap();
    }
}

impl Drop for MakerManager {
    fn drop(&mut self) {
        if let Some(mut child) = self.bitcoind_process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Typed errors for MakerManager operations
#[derive(Debug, thiserror::Error)]
pub enum MakerManagerError {
    #[error("Maker '{0}' not found")]
    NotFound(String),
    #[error("Maker '{0}' is already running")]
    AlreadyRunning(String),
    #[error("Maker '{0}' is already stopped")]
    AlreadyStopped(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
