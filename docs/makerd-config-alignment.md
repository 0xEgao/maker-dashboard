# Align maker-dashboard with makerd config semantics

Implementation doc. Goal: the dashboard stores maker configuration exactly like
openswap core / makerd does. No dashboard-specific maker config at the top level.

## Core semantics (what we align to)

- makerd runs one maker per data dir.
- Default data dir: `~/.openswap/maker`. Overridable with `-d`.
- The maker's config lives at `<data_dir>/config.toml`.
- Core reads it with `MakerServerConfig::new(Some(&path))`.
- Core writes it with `MakerServerConfig::write_to_file(&path)`.
- A missing or empty file is auto-created with defaults.

## What config.toml already covers

Core's file round-trips 13 fields (12 in openswap `776debc`; `fidelity_feerate`
added by PR #991, commit `5b874de`, now master HEAD):

- `network_port`, `rpc_port`, `socks_port`, `control_port`
- `tor_auth_password`
- `min_swap_amount`, `fidelity_amount`, `fidelity_timelock`, `fidelity_feerate`
- `base_fee`, `amount_relative_fee_pct`, `time_relative_fee_pct`
- `required_confirms`

`fidelity_feerate` (sat/vB): defaults to core's `MIN_FEE_RATE`; core validates
it as finite and `>= MIN_RELAY_FEE_RATE` on load. This makes the dashboard's
previously-unused `fee_rate` field real — rename it to `fidelity_feerate` and
store it in config.toml like the other static fields.

Core marks everything else "Runtime fields — not read from config file":

- backend connection (RPC URL, ZMQ, RPC credentials / Electrum URL)
  → user input at every dashboard start; held in memory only, never stored
  (see Startup flow)
- `wallet_name` → not stored; it is the wallet file name at
  `<data_dir>/wallets/<wallet_name>` (core: `src/maker/api.rs`), readable from disk
- `nostr_relays` → not stored; core's hardcoded `NOSTR_RELAYS` defaults apply
- `network`, `supported_protocols` → derived at runtime, not stored
- wallet `password` → never stored (see Password rule)

No per-maker JSON file exists.

## Target layout

```
~/.openswap/
├── auth.json              # dashboard login (unchanged)
├── tor/                   # tor data (unchanged)
├── <maker-1>/
│   ├── config.toml        # core MakerServerConfig file (13 static fields)
│   ├── wallets/           # wallet data (managed by core; dir entry = wallet name)
│   └── ...                # logs etc. (managed by core)
└── <maker-2>/
    └── ...
```

- Each maker gets its own data dir: `~/.openswap/<maker-name>`.
- A maker dir contains ONLY core-managed files. No dashboard-specific files inside.
- Dashboard-level data at the top level: just `auth.json` and `tor/`.
- No `~/.config/maker-dashboard`. No top-level `makers.json`. No per-maker JSON.
  No persisted backend config.

## Runtime backend config (never stored)

- One backend for the whole dashboard: Bitcoin Core **or** Electrum.
- Entered by the user at **every** dashboard start. Held in memory only.
- Nothing credentials-related ever touches disk. No encryption needed for it.
- A toggle picks Core vs Electrum; fields are prefilled with the hardcoded
  defaults (`127.0.0.1:38332`, ZMQ derived from RPC). The Electrum URL is
  prefilled with the hardcoded default `ssl://electrum.citadelfoss.xyz:50002`
  and stays editable by the user.
- Applies to every maker. Changing it re-inits all running makers.

## Startup flow (every server start)

1. Initial screen: check whether Tor is active (via `TorManager`), show status.
2. Same screen: backend toggle (Bitcoin Core / Electrum) with default values.
3. Submit → backend is stored in memory and applied to all makers.
4. First run (no maker dirs found): land on the create-maker page with the
   configurable maker config (the 13 config.toml fields, incl. fidelity
   feerate, + wallet password).
   The wallet password field has a confirm-password field next to it; both
   must match or the form does not proceed (client-side check — the backend
   never receives the confirmation).
5. Restart (maker dirs exist): same screen as steps 1–2 — Tor status check +
   backend toggle, every time. Only after submitting it are makers restored
   and the user lands on the dashboard home page. Auto-start per the Password
   rule below.

## Config read/write rules

- Loading a maker: read `<data_dir>/config.toml` via `MakerServerConfig::new`.
- Changing a maker's config: write it back via `MakerServerConfig::write_to_file`.
- config.toml is the single source of truth for a maker's static settings.
- Hand edits and makerd runs against the same data dir stay compatible.
- Runtime fields (the ones makerd takes as CLI args) come from the user's
  startup input (backend connection, in memory) or are read from core's own data
  (wallet name from `<data_dir>/wallets/`). Nothing else is stored per maker.
- No new core APIs. Only core's config reader/writer are used for config.toml.

## Password rule (security critical)

- The wallet password is **never stored**. Not in config.toml, not anywhere.
- Storing it would break wallet encryption.
- Auto-start policy: every maker auto-starts **if its wallet file can be opened**
  (unencrypted wallet, or wallet openable without a password).
- If the wallet needs a password, the maker stays stopped and the UI prompts.
  Submitting the password unlocks the wallet; only then does the maker start.
- The password is **not held in memory** by the dashboard. It is wrapped in
  `zeroize::Zeroizing` at the API boundary, used only during `MakerServer::init`
  (wallet load), and deterministically zeroized immediately after.
  (Caveat: openswap core currently keeps its own copy in the in-memory
  `MakerServerConfig` for the process lifetime — core's behavior, not ours.)

## Tasks

### Dependency bump

- Bump the openswap pin in `Cargo.toml`/`Cargo.lock` from `776debc` to at least
  `5b874de` (PR #991, adds `MakerServerConfig::fidelity_feerate` and its
  config.toml round-trip).
- Verify no other API breaks in that range (PR #988 was the Coinswap→OpenSwap
  rebrand; the pin already includes it).

### Persistence layer (`src/maker_manager/persistence.rs`)

- Delete `StoredMakerConfig`, `StoredState`, and the top-level makers.json code.
- Delete `DashboardSettings` and all `settings.json` handling (no longer needed —
  auto-start is unconditional, gated only by wallet openability).
- Delete the AES envelope helpers. Nothing sensitive is persisted anymore, so
  no encrypted-at-rest file exists.
- `src/auth.rs` shrinks to login only: keep `password_hash`, drop `enc_salt` and
  the AES key derivation (nothing left to encrypt).
- config.toml: thin wrappers around core's APIs only.
  - load: `MakerServerConfig::new(Some(&data_dir.join("config.toml")))`.
  - save: `MakerServerConfig::write_to_file(&data_dir.join("config.toml"))`.
- No per-maker JSON helpers. Maker dirs contain core-managed files only.

### Maker manager (`src/maker_manager/mod.rs`)

- `MakerConfig` = `MakerServerConfig` fields + wallet name read from disk.
  The existing `fee_rate` field is renamed to `fidelity_feerate` and mapped
  into config.toml (now consumed by core for fidelity bond transactions).
- Backend connection is runtime state on the manager, set from the startup
  screen input. Not per maker, not persisted.
- Create maker: resolve data dir `~/.openswap/<id>`, write config.toml via core's
  writer, init server (password passed separately if the wallet is encrypted).
- Update maker: rewrite config.toml, re-init server (password passed separately).
- Backend change: update the in-memory backend, re-init all makers.
- Restore on startup: scan `~/.openswap/` for subdirs containing a config.toml
  (skipping `tor/` and core's `taker/`), load each via `MakerServerConfig::new`,
  read the wallet name from `<data_dir>/wallets/`, register makers as stopped.
  They init/start once the backend is set from the startup screen.
- Passwords never enter `MakerConfig`: `create_maker`, `update_config`, and
  `start_maker` take `Option<Zeroizing<String>>`, used only during init and
  zeroized on return.
- Remove maker: stop and unregister; leave the data dir untouched (like makerd).

### Onboarding / session flow

- Every server start begins at the startup screen: Tor status check + backend
  toggle with hardcoded defaults. This replaces the current first-run-only
  onboarding for backend selection.
- First run (no maker dirs): continue to the create-maker page.
- Restart (maker dirs exist): same startup screen again (Tor check + backend
  toggle); after submitting, makers are restored and the user lands on the
  dashboard home page; makers auto-start if their wallet opens without a
  password.
- Makers whose wallet needs a password stay stopped; the UI prompts for each.
  Submitting a password unlocks that wallet and starts the maker.
- Dashboard login (auth.json) stays as today.
- No `auto_start_makers` setting. Auto-start is unconditional, gated only by
  wallet openability.

### Startup / CLI

- `src/utils/mod.rs`: default dir becomes `~/.openswap` (`dirs::home_dir().join(".openswap")`).
- `src/cli.rs`: rename `--config-dir` to `-d/--data-directory`, env `MAKER_DASHBOARD_DATA_DIR`.
- `src/main.rs`, `src/server.rs`: update call sites and comments.
- `auth.json`, `tor/` move to `~/.openswap/` top level automatically
  (they are derived from the data dir).

### Deploy, packaging, docs

- Replace every `~/.config/maker-dashboard` reference with `~/.openswap`:
  - `README.md`, `SECURITY.md`.
  - `deploy/setup.sh`, `deploy/maker-dashboard.service`.
  - `docker/docker-compose.yml` (volume target `/home/appuser/.openswap`).
  - `packaging/README.md`, `packaging/mynode/`, `packaging/umbrel/`.
  - Root `Dockerfile` if it references the old path/env.
- `SECURITY.md` must state the new at-rest model:
  - wallet passwords are never stored (prompted per session);
  - node credentials are never stored either — backend config is runtime input;
  - the only dashboard file left is `auth.json` (login hash, nothing decryptable);
  - maker dirs contain no dashboard-specific files and no secrets beyond
    core's own config.toml and wallet files;
  - the old encrypted top-level makers.json is gone.

### Tests

- Update all tests to the new layout. No top-level makers.json fixtures.
- `tests/api/auth.rs`: drop makers.json/encryption fixtures; login and
  first-run setup coverage only.
- `src/maker_manager/mod.rs` tests: assert creating a maker writes config.toml
  (including `fidelity_feerate`); assert a hand-edited config.toml wins on
  reload; assert the wallet name is recovered from `<data_dir>/wallets/`;
  assert no password ever lands on disk.
- Backend tests: backend is set via the startup input, held in memory, applied
  to all makers; backend change re-inits makers; nothing is written to disk.
- `src/api/monitoring.rs` tests: same temp-dir pattern, new file expectations.
- Add a test for the start-with-password flow (prompt required, in-memory only).
- Run `cargo build`, `cargo test`, `cargo clippy`. All must pass.

## Notes

- No migration from `~/.config/maker-dashboard`. Existing installs start fresh.
- Maker named `maker` maps to makerd's default dir `~/.openswap/maker` (full CLI
  interop). A maker named `taker` would collide with core's taker dir; left as-is.
