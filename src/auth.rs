use std::collections::HashMap;
use std::time::{Duration, Instant};

use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand::{thread_rng, RngCore};
use serde::{Deserialize, Serialize};

/// Stored in `{config_dir}/auth.json`. Login credential only — nothing is
/// encrypted at rest anymore, so no key-derivation material lives here.
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthConfig {
    /// argon2id PHC string (includes its own salt).
    pub password_hash: String,
}

impl AuthConfig {
    /// Creates a new `AuthConfig` by hashing `password` with argon2id.
    pub fn new(password: &str) -> anyhow::Result<Self> {
        let mut rng = thread_rng();

        // Hash password for verification.
        let salt = SaltString::generate(&mut rng);
        let password_hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("argon2 hash error: {e}"))?
            .to_string();

        Ok(Self { password_hash })
    }

    /// Loads from `{config_dir}/auth.json`. Returns `None` if the file doesn't exist.
    pub fn load(config_dir: &std::path::Path) -> anyhow::Result<Option<Self>> {
        let path = config_dir.join("auth.json");
        if !path.exists() {
            return Ok(None);
        }
        let data = std::fs::read_to_string(&path)?;
        let config: Self = serde_json::from_str(&data)?;
        Ok(Some(config))
    }

    /// Saves to `{config_dir}/auth.json`.
    /// On Unix the file is created with mode 0o600 (owner read/write only).
    pub fn save(&self, config_dir: &std::path::Path) -> anyhow::Result<()> {
        use std::io::Write;

        let path = config_dir.join("auth.json");
        let tmp = config_dir.join("auth.json.tmp");
        let json = serde_json::to_string(self)?;

        #[cfg(unix)]
        let mut file = {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&tmp)?
        };
        #[cfg(not(unix))]
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)?;

        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        drop(file);

        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Returns `true` if `password` matches the stored argon2id hash.
    pub fn verify(&self, password: &str) -> anyhow::Result<bool> {
        let parsed_hash = PasswordHash::new(&self.password_hash)
            .map_err(|e| anyhow::anyhow!("invalid PHC string: {e}"))?;
        match Argon2::default().verify_password(password.as_bytes(), &parsed_hash) {
            Ok(()) => Ok(true),
            Err(argon2::password_hash::Error::Password) => Ok(false),
            Err(e) => Err(anyhow::anyhow!("argon2 verify error: {e}")),
        }
    }
}

/// In-memory session management.
pub struct SessionStore {
    sessions: HashMap<String, Instant>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    /// Creates a new session with a 24 h TTL.
    /// Returns the session token (32 random bytes, hex-encoded).
    pub fn create(&mut self) -> String {
        let mut bytes = [0u8; 32];
        thread_rng().fill_bytes(&mut bytes);
        let token = hex::encode(bytes);
        let expiry = Instant::now() + Duration::from_secs(86400);
        self.sessions.insert(token.clone(), expiry);
        token
    }

    /// Returns `true` if `token` exists and hasn't expired.
    /// Prunes all expired sessions on every call.
    pub fn validate(&mut self, token: &str) -> bool {
        let now = Instant::now();
        self.sessions.retain(|_, expiry| *expiry > now);
        self.sessions.get(token).is_some_and(|expiry| *expiry > now)
    }

    /// Removes `token` from the store.
    pub fn remove(&mut self, token: &str) {
        self.sessions.remove(token);
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}
