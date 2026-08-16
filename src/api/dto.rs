use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::maker_manager::{MakerBackend, MakerConfig, MakerInfo as ManagerMakerInfo, MakerState};

/// Request body for `POST /api/makers`
///
/// All fields map to the maker's `<data_dir>/config.toml` (core's config
/// file). `password` is the wallet password: used once to encrypt/open the
/// wallet, zeroized after load, never persisted.
#[derive(Deserialize, ToSchema)]
pub struct CreateMakerRequest {
    #[schema(example = "maker1")]
    pub id: String,
    pub tor_auth: Option<String>,
    #[schema(example = "maker1")]
    pub wallet_name: Option<String>,
    /// Wallet password. Runtime-only; never stored.
    pub password: Option<String>,
    #[schema(example = 6102)]
    pub network_port: Option<u16>,
    #[schema(example = 6103)]
    pub rpc_port: Option<u16>,
    #[schema(example = 9050)]
    pub socks_port: Option<u16>,
    #[schema(example = 9051)]
    pub control_port: Option<u16>,
    #[schema(example = 10000)]
    pub min_swap_amount: Option<u64>,
    #[schema(example = 10000)]
    pub fidelity_amount: Option<u64>,
    #[schema(example = 15000)]
    pub fidelity_timelock: Option<u32>,
    /// Fidelity bond transaction fee rate in sat/vB, consumed by core.
    /// Default: 2.0.
    #[schema(example = 2.0)]
    pub fidelity_feerate: Option<f64>,
    #[schema(example = 1)]
    pub required_confirms: Option<u32>,
    #[schema(example = 1000)]
    pub base_fee: Option<u64>,
    #[schema(example = 0.025)]
    pub amount_relative_fee_pct: Option<f64>,
    #[schema(example = 0.001)]
    pub time_relative_fee_pct: Option<f64>,
}

/// Request body for `PUT /api/makers/{id}/config`
///
/// `password` is accepted because the wallet is re-opened during re-init;
/// it is zeroized after use and never stored.
#[derive(Deserialize, ToSchema)]
pub struct UpdateMakerConfigRequest {
    pub tor_auth: Option<String>,
    #[schema(example = "maker1")]
    pub wallet_name: Option<String>,
    /// Wallet password. Runtime-only; never stored.
    pub password: Option<String>,
    #[schema(example = 6102)]
    pub network_port: Option<u16>,
    #[schema(example = 6103)]
    pub rpc_port: Option<u16>,
    #[schema(example = 9050)]
    pub socks_port: Option<u16>,
    #[schema(example = 9051)]
    pub control_port: Option<u16>,
    #[schema(example = 10000)]
    pub min_swap_amount: Option<u64>,
    #[schema(example = 10000)]
    pub fidelity_amount: Option<u64>,
    #[schema(example = 15000)]
    pub fidelity_timelock: Option<u32>,
    /// Fidelity bond transaction fee rate in sat/vB, consumed by core.
    #[schema(example = 2.0)]
    pub fidelity_feerate: Option<f64>,
    #[schema(example = 1)]
    pub required_confirms: Option<u32>,
    #[schema(example = 1000)]
    pub base_fee: Option<u64>,
    #[schema(example = 0.025)]
    pub amount_relative_fee_pct: Option<f64>,
    #[schema(example = 0.001)]
    pub time_relative_fee_pct: Option<f64>,
}

impl UpdateMakerConfigRequest {
    /// Merges the update request on top of a base `MakerConfig`, overriding only provided fields.
    pub fn apply_to(self, base: MakerConfig) -> MakerConfig {
        MakerConfig {
            data_directory: base.data_directory,
            tor_auth: self.tor_auth.or(base.tor_auth),
            wallet_name: self.wallet_name.or(base.wallet_name),
            network_port: self.network_port.unwrap_or(base.network_port),
            rpc_port: self.rpc_port.unwrap_or(base.rpc_port),
            socks_port: self.socks_port.unwrap_or(base.socks_port),
            control_port: self.control_port.unwrap_or(base.control_port),
            min_swap_amount: self.min_swap_amount.unwrap_or(base.min_swap_amount),
            fidelity_amount: self.fidelity_amount.unwrap_or(base.fidelity_amount),
            fidelity_timelock: self.fidelity_timelock.unwrap_or(base.fidelity_timelock),
            fidelity_feerate: self.fidelity_feerate.unwrap_or(base.fidelity_feerate),
            required_confirms: self.required_confirms.unwrap_or(base.required_confirms),
            base_fee: self.base_fee.unwrap_or(base.base_fee),
            amount_relative_fee_pct: self
                .amount_relative_fee_pct
                .unwrap_or(base.amount_relative_fee_pct),
            time_relative_fee_pct: self
                .time_relative_fee_pct
                .unwrap_or(base.time_relative_fee_pct),
        }
    }
}

/// Request body for `POST /api/makers/{id}/start`
///
/// `password` is the wallet-opening password for encrypted wallets. It is
/// used only during wallet load, zeroized immediately after, never stored.
#[derive(Deserialize, ToSchema)]
pub struct StartMakerRequest {
    pub password: Option<String>,
}

/// Request body for `POST /api/backend`: sets the chain-data backend for all
/// makers. Runtime-only — entered at every dashboard start, never persisted.
#[derive(Deserialize, ToSchema)]
pub struct SetBackendRequest {
    /// "electrum" (default, no local node) or "bitcoind".
    #[schema(example = "electrum")]
    pub kind: MakerBackend,
    /// Bitcoin Core RPC address (bitcoind only).
    #[schema(example = "127.0.0.1:38332")]
    pub rpc: Option<String>,
    /// Bitcoin Core ZMQ address (bitcoind only).
    #[schema(example = "tcp://127.0.0.1:28332")]
    pub zmq: Option<String>,
    /// Bitcoin Core RPC username (bitcoind only).
    #[schema(example = "user")]
    pub rpc_user: Option<String>,
    /// Bitcoin Core RPC password (bitcoind only).
    #[schema(example = "password")]
    pub rpc_password: Option<String>,
    /// Electrum server URL (electrum only). Default: ssl://electrum.citadelfoss.xyz:50002.
    #[schema(example = "ssl://electrum.citadelfoss.xyz:50002")]
    pub electrum_url: Option<String>,
}

/// Response for `GET /api/backend`. The RPC password is never returned.
#[derive(Debug, Serialize, ToSchema)]
pub struct BackendInfo {
    /// Whether the backend has been set this session.
    pub configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<MakerBackend>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zmq: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpc_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub electrum_url: Option<String>,
}

/// Request body for `POST /api/makers/{id}/send`
#[derive(Deserialize, ToSchema)]
pub struct SendToAddressRequest {
    #[schema(example = "bcrt1qxyzw0k8a3gp6n9lqz7th0gkc4e5mvetlkgkay")]
    pub address: String,
    #[schema(example = 50000)]
    pub amount: u64,
    #[schema(example = 1.0)]
    pub feerate: f64,
}

/// Generic success / error JSON envelope
#[derive(Debug, Serialize, ToSchema)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MakerInfo {
    pub id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SuggestedMakerPorts {
    pub network_port: u16,
    pub rpc_port: u16,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MakerStateDto {
    Running,
    Stopped,
}

impl From<MakerState> for MakerStateDto {
    fn from(s: MakerState) -> Self {
        match s {
            MakerState::Running => Self::Running,
            MakerState::Stopped => Self::Stopped,
        }
    }
}

/// Detailed maker information including config and state
#[derive(Debug, Serialize, ToSchema)]
pub struct MakerInfoDetailed {
    pub id: String,
    pub state: MakerStateDto,
    pub wallet_name: Option<String>,
    pub data_directory: Option<String>,
    pub network_port: u16,
    pub rpc_port: u16,
    pub socks_port: u16,
    pub control_port: u16,
    pub min_swap_amount: u64,
    pub fidelity_amount: u64,
    pub fidelity_timelock: u32,
    pub fidelity_feerate: f64,
    pub required_confirms: u32,
    pub base_fee: u64,
    pub amount_relative_fee_pct: f64,
    pub time_relative_fee_pct: f64,
}

impl From<ManagerMakerInfo> for MakerInfoDetailed {
    fn from(info: ManagerMakerInfo) -> Self {
        let config = info.config;

        Self {
            id: info.id,
            state: info.state.into(),
            wallet_name: config.wallet_name,
            data_directory: config.data_directory.and_then(|d| {
                if let Ok(path) = d.canonicalize() {
                    return path.to_str().map(str::to_string);
                }
                d.to_str().map(str::to_string)
            }),
            network_port: config.network_port,
            rpc_port: config.rpc_port,
            socks_port: config.socks_port,
            control_port: config.control_port,
            min_swap_amount: config.min_swap_amount,
            fidelity_amount: config.fidelity_amount,
            fidelity_timelock: config.fidelity_timelock,
            fidelity_feerate: config.fidelity_feerate,
            required_confirms: config.required_confirms,
            base_fee: config.base_fee,
            amount_relative_fee_pct: config.amount_relative_fee_pct,
            time_relative_fee_pct: config.time_relative_fee_pct,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BalanceInfo {
    pub regular: u64,
    pub swap: u64,
    pub contract: u64,
    pub fidelity: u64,
    pub spendable: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UtxoInfo {
    pub addr: String,
    pub amount: u64,
    pub confirmations: u32,
    pub utxo_type: String,
}

/// Swap history for a maker: active (in-flight) and completed (swept) swaps
#[derive(Debug, Serialize, ToSchema)]
pub struct SwapHistoryDto {
    /// In-progress incoming swap coins (2-of-2 multisig not yet swept)
    pub active: Vec<UtxoInfo>,
    /// Completed swaps whose coins have been swept to the regular wallet
    pub completed: Vec<UtxoInfo>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MakerFeeInfoDto {
    pub maker_index: usize,
    pub maker_address: String,
    pub base_fee: f64,
    pub amount_relative_fee: f64,
    pub time_relative_fee: f64,
    pub total_fee: f64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SwapReportDto {
    pub swap_id: String,
    pub role: String,
    pub status: String,
    pub swap_duration_seconds: f64,
    #[serde(default)]
    pub recovery_duration_seconds: f64,
    pub start_timestamp: u64,
    pub end_timestamp: u64,
    pub network: String,
    pub error_message: Option<String>,
    pub incoming_amount: u64,
    pub outgoing_amount: u64,
    pub fee_paid_or_earned: i64,
    pub incoming_contract_txid: Option<String>,
    pub outgoing_contract_txid: Option<String>,
    #[serde(default)]
    pub funding_txids: Vec<Vec<String>>,
    #[serde(default)]
    pub recovery_txids: Option<Vec<String>>,
    pub timelock: u16,
    pub makers_count: Option<usize>,
    #[serde(default)]
    pub maker_addresses: Vec<String>,
    #[serde(default)]
    pub maker_fee_info: Vec<MakerFeeInfoDto>,
    pub total_maker_fees: u64,
    pub mining_fee: u64,
    pub fee_percentage: f64,
    #[serde(default)]
    pub input_utxos: Vec<u64>,
    #[serde(default)]
    pub output_change_amounts: Vec<u64>,
    #[serde(default)]
    pub output_swap_amounts: Vec<u64>,
    #[serde(default)]
    pub output_change_utxos: Vec<(u64, String)>,
    #[serde(default)]
    pub output_swap_utxos: Vec<(u64, String)>,
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub deniability_proof: Option<DeniabilityProofDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MakerStatus {
    pub id: String,
    pub alive: bool,
    pub is_server_running: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: &'static str,
    pub makers: Vec<MakerStatus>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RpcStatusInfo {
    pub connected: bool,
    pub version: Option<u32>,
    pub network: Option<String>,
    pub block_height: Option<u64>,
    pub sync_progress: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum StartupCheckKind {
    Bitcoin,
    Rpc,
    Rest,
    Zmq,
    Tor,
    Electrum,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct StartupCheckRequest {
    pub check: StartupCheckKind,
    #[schema(example = "127.0.0.1:38332")]
    pub rpc: Option<String>,
    #[schema(example = "user")]
    pub rpc_user: Option<String>,
    #[schema(example = "password")]
    pub rpc_password: Option<String>,
    #[schema(example = "tcp://127.0.0.1:28332")]
    pub zmq: Option<String>,
    /// Electrum server URL to probe (electrum check only). Defaults to the
    /// hardcoded server when omitted.
    #[schema(example = "ssl://electrum.citadelfoss.xyz:50002")]
    pub electrum_url: Option<String>,
    #[schema(example = 9050)]
    pub socks_port: Option<u16>,
    #[schema(example = 9051)]
    pub control_port: Option<u16>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StartupCheckResponse {
    pub check: StartupCheckKind,
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Request body for `POST /api/bitcoind/start`
#[derive(Debug, Deserialize, ToSchema)]
pub struct StartBitcoindRequest {
    /// Network to run bitcoind on: "regtest" or "signet"
    #[schema(example = "regtest")]
    pub network: String,
}

/// A single log line tagged with the maker it came from.
#[derive(Debug, Serialize, ToSchema)]
pub struct CombinedLogLine {
    pub maker_id: String,
    pub line: String,
}

/// Status of the dashboard-managed bitcoind process
#[derive(Debug, Serialize, ToSchema)]
pub struct BitcoindStatusInfo {
    pub running: bool,
    pub network: Option<String>,
    /// True only when bitcoind was started by the dashboard (and can be stopped via /stop)
    pub managed: bool,
}

/// Tor connectivity status reported at startup
#[derive(Debug, Serialize, ToSchema)]
pub struct TorStatusInfo {
    /// How tor was obtained: "system", "host", or "docker"
    pub source: &'static str,
    /// Whether the dashboard started/manages the tor process
    pub managed: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VerifyDeniabilityRequest {
    pub swap_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VerifyDeniabilityResponse {
    pub valid: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DeniabilityProofDto {
    pub swap_id: String,
    pub role: String,
    pub protocol: String,
    pub outgoing_swapcoin: Option<String>,
    #[schema(value_type = Object)]
    pub proof: serde_json::Value,
    pub created_at: u64,
}
