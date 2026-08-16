use std::net::IpAddr;
use std::path::PathBuf;

use clap::Parser;

/// Maker Dashboard, HTTP API server for managing openswap makers
#[derive(Parser, Debug)]
#[command(
    name = "maker-dashboard",
    version,
    about = "HTTP dashboard for managing openswap maker nodes",
    long_about = None,
)]
pub struct Cli {
    /// IP address to bind the server to
    #[arg(long, default_value = "127.0.0.1", env = "MAKER_DASHBOARD_HOST")]
    pub host: IpAddr,

    /// Port to listen on
    #[arg(short, long, default_value_t = 3000, env = "MAKER_DASHBOARD_PORT")]
    pub port: u16,

    /// Path to the frontend static files directory
    #[arg(
        long,
        default_value = "frontend/build/client",
        env = "MAKER_DASHBOARD_FRONTEND_PATH"
    )]
    pub frontend_path: PathBuf,

    /// Path to the SPA fallback index.html file
    #[arg(
        long,
        default_value = "frontend/build/client/index.html",
        env = "MAKER_DASHBOARD_SPA_INDEX"
    )]
    pub spa_index: PathBuf,

    /// Allow requests from non-localhost addresses (enabled by default for security)
    #[arg(long, default_value_t = true, env = "MAKER_DASHBOARD_ALLOW_REMOTE")]
    pub allow_remote: bool,

    /// Disable the Secure attribute on session cookies for plain HTTP deployments.
    #[arg(
        long,
        default_value_t = false,
        env = "MAKER_DASHBOARD_DISABLE_SECURE_COOKIES"
    )]
    pub disable_secure_cookies: bool,

    /// Log filter directive for stdout (e.g. "info", "debug", "tower_http=debug,info").
    /// Defaults to "off": no logs on stdout, only the dashboard URL is printed.
    /// Per-maker log files (<data_dir>/<id>/debug.log) are always written at
    /// full verbosity (all levels including DEBUG/TRACE) regardless of this
    /// setting.
    #[arg(long, default_value = "off", env = "MAKER_DASHBOARD_LOG_FILTER")]
    pub log_filter: String,

    /// Disable ANSI colors in log output (useful for log files / CI)
    #[arg(long, default_value_t = false, env = "MAKER_DASHBOARD_NO_COLOR")]
    pub no_color: bool,

    /// Application data directory. Stores dashboard state (auth.json) and, by
    /// default, each maker's data directory (`<data_dir>/<id>/`, including its
    /// config.toml). Mirrors makerd's `-d/--data-directory`; default: `~/.openswap`.
    #[arg(long, short = 'd', env = "MAKER_DASHBOARD_DATA_DIR")]
    pub data_directory: Option<PathBuf>,
}
