# Security

## Authentication

The dashboard is protected by a password that you choose on first run. The password is
hashed with **Argon2id** and stored in `~/.openswap/auth.json`. On
subsequent starts you log in via the browser; a valid session is required for every
`/api/*` route.

Once logged in, the browser holds a session cookie (`HttpOnly`, `Secure`,
`SameSite=Strict`, 24 h expiry). All `/api/*` routes reject requests without a
valid session with HTTP 401.

### First-run setup

On a fresh install (no `auth.json` present), `/setup` is reachable and accepts a
password to initialize the dashboard. The chosen password is hashed with Argon2id.
Once `auth.json` exists, `/setup` returns 409, only `/login` works.

There is intentionally no token gating `/setup`. Before initialization there is no user
data to protect: if a hostile party on the network races the operator and completes
setup first, the recovery is to stop the server, delete `auth.json`, and run setup
again. To prevent races on multi-tenant or network-exposed hosts, restrict access to
the dashboard port until setup is complete (e.g. keep `--allow-remote` off, or use a
firewall rule).

## At-rest data model

No secrets are stored by the dashboard beyond the login hash:

- `~/.openswap/auth.json` stores only the argon2id password hash. There is no
  encrypted state file anymore — nothing on disk is decryptable with the dashboard
  password.
- **Wallet passwords are never stored.** They are prompted per session when a maker's
  wallet needs one, used only to open the wallet, and zeroized from dashboard memory
  immediately after the wallet is loaded. (Note: openswap core itself currently keeps
  its own copy in its in-memory `MakerServer` for the process lifetime.)
- **Node credentials are never stored.** The backend connection (Bitcoin Core RPC
  credentials or Electrum) is entered on the startup screen at every dashboard start
  and held in memory only.
- Per-maker settings live in `~/.openswap/{id}/config.toml` — openswap core's own
  config file, exactly as makerd writes it. It contains ports, fees, and fidelity
  settings; no credentials. (Caveat: `tor_auth_password`, if set, is a static field
  core writes to this file in plaintext — same as makerd.)

To change your password, use the **Change password** button in the dashboard nav bar,
which calls `POST /api/auth/rotate-password`. The endpoint atomically rewrites
`auth.json`. The new password takes effect immediately for the current session and on
subsequent logins; no restart or env-var update is required.

## Localhost-only access

When you are using `--allow-remote=false`, the dashboard would bind to `127.0.0.1` and a middleware rejects every request
whose source IP is not a loopback address, providing defence-in-depth on top of the
password layer. But by default, it allows non-Localhost requests without issues. This is an additional security layer if you want.

You may be able to access the dashboard if you use this option and run it inside a docker container. You will need to run it on the host network and may face a lot of issues with non-linux systems like [MacOS](https://forums.docker.com/t/enabling-network-host-on-macos/150379).

## Hot wallet

Each maker holds a hot wallet. Private keys are stored unencrypted on disk under
`~/.openswap/{id}/wallets/`. Anyone with read access to those files can sweep the wallet.
Secure the directory with appropriate filesystem permissions and treat that path like any
other hot wallet.

You can set a wallet password at creation time via the `password` field in the creation
request. When set, the openswap library encrypts the wallet file at rest.

Back up the wallet files before making configuration changes or upgrading.

## Fidelity bond timelock

The fidelity bond is a time-locked Bitcoin output. The funds are unspendable until the
timelock expires (the default is ~3 months). Deleting a maker from the dashboard does
not reclaim those funds. it only removes the dashboard registration. The locked output
stays on-chain and must be swept manually after the timelock expires using the wallet's
seed or a backup.

Before deleting a maker, check the remaining timelock via `GET /api/makers/{id}/fidelity`
and make sure you have a wallet backup or the seed phrase stored safely.

## Reporting vulnerabilities

Please report security issues privately to the maintainers via the
[GitHub Security Advisories](https://github.com/citadel-foss/maker-dashboard/security/advisories/new)
page rather than opening a public issue.
