import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Eye, EyeOff, LoaderCircle, Play, X } from "lucide-react";
import {
  backend,
  makers,
  monitoring,
  type MakerBackend,
  type TorStatusInfo,
} from "../api";

const BITCOIND_DEFAULTS = {
  rpc: "127.0.0.1:38332",
  zmq: "tcp://127.0.0.1:28332",
  rpcUser: "user",
  rpcPassword: "password",
};

/** Default hosted Electrum server (ssl port). User-configurable below. */
const ELECTRUM_DEFAULT_URL = "ssl://electrum.citadelfoss.xyz:50002";

function torStatusLabel(status: TorStatusInfo | null): string {
  if (!status) return "Checking Tor status…";
  if (!status.managed) return "Tor is not running";
  const source =
    status.source === "embedded"
      ? "embedded Tor"
      : status.source === "system"
        ? "system Tor"
        : "host binary";
  return `Tor is running (${source})`;
}

export default function BackendSetup() {
  const navigate = useNavigate();
  const [kind, setKind] = useState<MakerBackend>("electrum");
  const [electrumUrl, setElectrumUrl] = useState(ELECTRUM_DEFAULT_URL);
  const [rpc, setRpc] = useState(BITCOIND_DEFAULTS.rpc);
  const [zmq, setZmq] = useState(BITCOIND_DEFAULTS.zmq);
  const [rpcUser, setRpcUser] = useState(BITCOIND_DEFAULTS.rpcUser);
  const [rpcPassword, setRpcPassword] = useState(BITCOIND_DEFAULTS.rpcPassword);
  const [showRpcPassword, setShowRpcPassword] = useState(false);
  const [torStatus, setTorStatus] = useState<TorStatusInfo | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    monitoring
      .getTorStatus()
      .then(setTorStatus)
      .catch(() => {});
  }, []);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (submitting) return;

    setError(null);
    setSubmitting(true);
    try {
      await backend.set(
        kind === "electrum"
          ? { kind, electrum_url: electrumUrl }
          : { kind, rpc, zmq, rpc_user: rpcUser, rpc_password: rpcPassword },
      );
      const count = await makers.count().catch(() => 0);
      navigate(count === 0 ? "/addMaker" : "/", { replace: true });
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to configure backend",
      );
      setSubmitting(false);
    }
  }

  return (
    <div className="cs-page">
      <main className="cs-add-page">
        <header className="cs-add-head">
          <div>
            <h1>Chain Backend Setup</h1>
            <p>
              Choose where your makers get Bitcoin chain data from. This applies
              to all makers and is kept in memory only — you will see this
              screen on each app start.
            </p>
          </div>
          <div className="cs-network-badge cs-add-network">
            <span className="cs-dot" />
            Signet · v0.4.2
          </div>
        </header>

        {error && (
          <div className="cs-banner error">
            <span>{error}</span>
            <button
              type="button"
              className="cs-home-icon"
              onClick={() => setError(null)}
              aria-label="Dismiss error"
            >
              <X size={15} />
            </button>
          </div>
        )}

        <form className="cs-add-layout" onSubmit={handleSubmit}>
          <section className="cs-card">
            <div className="cs-card-head">
              <div>
                <h2>Tor</h2>
                <p>Makers advertise and communicate over Tor.</p>
              </div>
            </div>
            <div className="cs-card-body">
              <div className="flex items-center gap-2 text-sm">
                <span
                  className={`cs-dot ${
                    torStatus?.managed
                      ? "text-[var(--cs-green)]"
                      : "text-[var(--cs-text-3)]"
                  }`}
                />
                <span>{torStatusLabel(torStatus)}</span>
              </div>
            </div>
          </section>

          <section className="cs-card">
            <div className="cs-card-head">
              <div>
                <h2>Chain backend</h2>
                <p>Where your makers get their Bitcoin chain data from.</p>
              </div>
            </div>
            <div className="cs-card-body cs-field-grid">
              <div className="cs-field cs-span-2">
                <div className="flex gap-2">
                  <button
                    type="button"
                    disabled={submitting}
                    onClick={() => setKind("electrum")}
                    className={`flex-1 py-2.5 rounded-lg text-sm font-semibold transition-all ${
                      kind === "electrum"
                        ? "bg-orange-600 text-white"
                        : "bg-gray-800 text-gray-400 hover:bg-gray-700"
                    }`}
                  >
                    Electrum
                  </button>
                  <button
                    type="button"
                    disabled={submitting}
                    onClick={() => setKind("bitcoind")}
                    className={`flex-1 py-2.5 rounded-lg text-sm font-semibold transition-all ${
                      kind === "bitcoind"
                        ? "bg-orange-600 text-white"
                        : "bg-gray-800 text-gray-400 hover:bg-gray-700"
                    }`}
                  >
                    Bitcoin Core
                  </button>
                </div>
                <p className="cs-hint">
                  {kind === "electrum"
                    ? "Uses a hosted Electrum server — no local node needed."
                    : "Uses your own Bitcoin Core node via RPC + ZMQ."}
                </p>
              </div>

              {kind === "electrum" && (
                <div className="cs-field cs-span-2">
                  <div className="cs-field-label-row">
                    <label>
                      Electrum server<span className="cs-required"> *</span>
                    </label>
                  </div>
                  <input
                    className="cs-input"
                    name="electrumUrl"
                    value={electrumUrl}
                    onChange={(e) => setElectrumUrl(e.target.value)}
                    placeholder={ELECTRUM_DEFAULT_URL}
                    disabled={submitting}
                    required
                  />
                  <p className="cs-hint">
                    Format: tcp://host:port or ssl://host:port. Defaults to the
                    hosted citadelfoss server.
                  </p>
                </div>
              )}

              {kind === "bitcoind" && (
                <>
                  <div className="cs-field cs-span-2">
                    <div className="cs-field-label-row">
                      <label>
                        Bitcoin RPC endpoint
                        <span className="cs-required"> *</span>
                      </label>
                    </div>
                    <input
                      className="cs-input"
                      name="rpc"
                      value={rpc}
                      onChange={(e) => setRpc(e.target.value)}
                      placeholder={BITCOIND_DEFAULTS.rpc}
                      disabled={submitting}
                      required
                    />
                    <p className="cs-hint">Format: host:port</p>
                  </div>

                  <div className="cs-field">
                    <div className="cs-field-label-row">
                      <label>
                        RPC username<span className="cs-required"> *</span>
                      </label>
                    </div>
                    <input
                      className="cs-input"
                      name="rpcUser"
                      value={rpcUser}
                      onChange={(e) => setRpcUser(e.target.value)}
                      placeholder={BITCOIND_DEFAULTS.rpcUser}
                      disabled={submitting}
                      required
                    />
                  </div>

                  <div className="cs-field">
                    <div className="cs-field-label-row">
                      <label>
                        RPC password<span className="cs-required"> *</span>
                      </label>
                    </div>
                    <div className="cs-input-wrap">
                      <input
                        className="cs-input"
                        type={showRpcPassword ? "text" : "password"}
                        name="rpcPassword"
                        value={rpcPassword}
                        onChange={(e) => setRpcPassword(e.target.value)}
                        placeholder={BITCOIND_DEFAULTS.rpcPassword}
                        disabled={submitting}
                        required
                      />
                      <button
                        type="button"
                        className="cs-eye"
                        onClick={() => setShowRpcPassword((v) => !v)}
                        aria-label="Toggle RPC password visibility"
                      >
                        {showRpcPassword ? (
                          <EyeOff size={16} />
                        ) : (
                          <Eye size={16} />
                        )}
                      </button>
                    </div>
                  </div>

                  <div className="cs-field cs-span-2">
                    <div className="cs-field-label-row">
                      <label>
                        ZMQ endpoint<span className="cs-required"> *</span>
                      </label>
                    </div>
                    <input
                      className="cs-input"
                      name="zmq"
                      value={zmq}
                      onChange={(e) => setZmq(e.target.value)}
                      placeholder={BITCOIND_DEFAULTS.zmq}
                      disabled={submitting}
                      required
                    />
                    <p className="cs-hint">
                      Subscribe to rawblock + rawtx notifications.
                    </p>
                  </div>
                </>
              )}
            </div>
          </section>

          <div className="cs-add-actions">
            <button
              type="submit"
              className="cs-btn primary"
              disabled={submitting}
            >
              {submitting ? (
                <LoaderCircle size={18} className="cs-spin" />
              ) : (
                <Play size={18} />
              )}
              {submitting ? "Configuring..." : "Continue"}
            </button>
          </div>
        </form>
      </main>
    </div>
  );
}
