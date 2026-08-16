pub mod bidirectional_channel;
pub mod log_writer;

/// Returns the default data directory for the application: ~/.openswap
///
/// Matches the openswap core / makerd layout: dashboard state (auth.json,
/// tor/) lives at the top level, and each maker's data directory (including
/// its config.toml) lives under `~/.openswap/<id>`.
pub fn default_data_dir() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".openswap")
}
