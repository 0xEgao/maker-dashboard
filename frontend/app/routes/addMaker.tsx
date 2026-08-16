import { useState } from "react";
import type { ReactNode } from "react";
import { Link, useNavigate } from "react-router-dom";
import {
  ArrowLeft,
  Check,
  Eye,
  EyeOff,
  LoaderCircle,
  Play,
  X,
} from "lucide-react";
import {
  ApiError,
  makers,
  onboarding,
  type CreateMakerRequest,
  type MakerBackend,
} from "../api";

type CheckId = "electrum" | "bitcoin" | "tor";

type CheckState = {
  status: "idle" | "loading" | "success" | "error";
  message?: string;
};

type CheckRowDef = {
  id: CheckId;
  title: string;
  desc: string;
};

const ELECTRUM_CHECK: CheckRowDef = {
  id: "electrum",
  title: "Checking Electrum",
  desc: "Reaching the Electrum server used for chain data.",
};

const BITCOIND_CHECK: CheckRowDef = {
  id: "bitcoin",
  title: "Checking Bitcoin Core",
  desc: "Bitcoin Core is running and fully synced.",
};

const TOR_CHECK: CheckRowDef = {
  id: "tor",
  title: "Checking Tor",
  desc: "Tor SOCKS and control ports, needed for maker networking.",
};

function checksForBackend(backend: MakerBackend): CheckRowDef[] {
  return [backend === "electrum" ? ELECTRUM_CHECK : BITCOIND_CHECK, TOR_CHECK];
}

function Field({
  label,
  required,
  optional,
  hint,
  className,
  children,
}: {
  label: string;
  required?: boolean;
  optional?: boolean;
  hint?: ReactNode;
  className?: string;
  children: ReactNode;
}) {
  return (
    <div className={`cs-field ${className ?? ""}`}>
      <div className="cs-field-label-row">
        <label>
          {label}
          {required && <span className="cs-required"> *</span>}
        </label>
        {optional && <span>Optional</span>}
      </div>
      {children}
      {hint && <p className="cs-hint">{hint}</p>}
    </div>
  );
}

function CheckRow({ row, state }: { row: CheckRowDef; state: CheckState }) {
  const isLoading = state.status === "loading";
  const isSuccess = state.status === "success";
  const isError = state.status === "error";

  return (
    <div
      className={`cs-add-check ${
        isSuccess ? "success" : isError ? "error" : ""
      }`}
    >
      <span className="cs-add-check-dot" aria-hidden="true">
        {isLoading && <LoaderCircle size={14} className="cs-spin" />}
        {isSuccess && <Check size={14} />}
        {isError && <X size={14} />}
      </span>
      <div className="cs-add-check-body">
        <strong>{row.title}</strong>
        <p>{row.desc}</p>
        {state.message && (
          <span className={isError ? "cs-add-result error" : "cs-add-result"}>
            {state.message}
          </span>
        )}
      </div>
    </div>
  );
}

export default function AddMaker({ firstRun = false }: { firstRun?: boolean }) {
  const navigate = useNavigate();
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [backend, setBackend] = useState<MakerBackend>("electrum");
  const [bitcoinRpc, setBitcoinRpc] = useState("127.0.0.1:38332");
  const [bitcoinUser, setBitcoinUser] = useState("user");
  const [bitcoinPassword, setBitcoinPassword] = useState("password");
  const [showRpcPassword, setShowRpcPassword] = useState(false);
  const [zmq, setZmq] = useState("tcp://127.0.0.1:28332");
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [checks, setChecks] = useState<Partial<Record<CheckId, CheckState>>>(
    {},
  );

  const activeChecks = checksForBackend(backend);
  const checksStarted = Object.values(checks).some((c) => c && c.status !== "idle");

  async function runCheck(check: CheckId): Promise<boolean> {
    setChecks((prev) => ({ ...prev, [check]: { status: "loading" } }));

    try {
      const result = await onboarding.startupCheck({
        check,
        rpc: bitcoinRpc,
        rpc_user: bitcoinUser,
        rpc_password: bitcoinPassword,
        zmq,
      });
      setChecks((prev) => ({
        ...prev,
        [check]: {
          status: result.success ? "success" : "error",
          message: result.message,
        },
      }));
      return result.success;
    } catch (err) {
      setChecks((prev) => ({
        ...prev,
        [check]: {
          status: "error",
          message: err instanceof Error ? err.message : "Check failed",
        },
      }));
      return false;
    }
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (starting) return;

    const id = name.trim();
    if (!id) {
      setError("Maker name cannot be empty.");
      return;
    }

    setError(null);
    setStarting(true);
    setChecks({});

    // Live startup checks — chain backend reachability and Tor readiness.
    const results = await Promise.all(
      activeChecks.map((row) => runCheck(row.id)),
    );
    if (results.some((ok) => !ok)) {
      setError("Startup checks failed. Fix the problem above and try again.");
      setStarting(false);
      return;
    }

    const body: CreateMakerRequest = {
      id,
      backend,
      wallet_name: id,
      password: password || undefined,
      ...(backend === "bitcoind" && {
        rpc: bitcoinRpc,
        rpc_user: bitcoinUser,
        rpc_password: bitcoinPassword,
        zmq,
      }),
    };

    try {
      await makers.create(body);
      try {
        await makers.start(id);
      } catch (startErr) {
        if (!(startErr instanceof ApiError && startErr.status === 409)) {
          throw startErr;
        }
      }
      navigate(`/makers/${id}/setup`);
    } catch (err: unknown) {
      if (err instanceof ApiError && err.status === 409) {
        setError(`A maker named "${id}" already exists.`);
      } else {
        setError(err instanceof Error ? err.message : "Failed to create maker");
      }
      setStarting(false);
    }
  }

  return (
    <div className="cs-page">
      <main className="cs-add-page">
        <header className="cs-add-head">
          <div>
            {!firstRun && (
              <Link to="/" className="cs-add-back">
                <ArrowLeft size={14} />
                Back to dashboard
              </Link>
            )}
            <h1>{firstRun ? "Create First Maker" : "Add New Maker"}</h1>
            <p>Name your maker — everything else is configured for you.</p>
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
          <section className="cs-card cs-add-basic">
            <div className="cs-card-head">
              <div>
                <h2>Maker</h2>
                <p>
                  Identifies this maker across logs, RPC calls, and dashboards.
                </p>
              </div>
            </div>
            <div className="cs-card-body cs-field-grid">
              <Field
                label="Maker name"
                required
                hint={
                  <>
                    Unique identifier. Data is stored at{" "}
                    <code>~/.openswap/&lt;name&gt;</code> automatically.
                  </>
                }
                className="cs-span-2"
              >
                <input
                  className="cs-input"
                  name="name"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="e.g. maker-1"
                  disabled={starting}
                  required
                />
              </Field>

              <Field
                label="Wallet password"
                optional
                hint="Encrypts the maker's wallet on disk."
                className="cs-span-2"
              >
                <div className="cs-input-wrap">
                  <input
                    className="cs-input"
                    type={showPassword ? "text" : "password"}
                    name="password"
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    placeholder="Optional"
                    disabled={starting}
                  />
                  <button
                    type="button"
                    className="cs-eye"
                    onClick={() => setShowPassword((v) => !v)}
                    aria-label="Toggle wallet password visibility"
                  >
                    {showPassword ? <EyeOff size={16} /> : <Eye size={16} />}
                  </button>
                </div>
              </Field>
            </div>
          </section>

          <section className="cs-card">
            <div className="cs-card-head">
              <div>
                <h2>Chain backend</h2>
                <p>Where this maker gets its Bitcoin chain data from.</p>
              </div>
            </div>
            <div className="cs-card-body cs-field-grid">
              <div className="cs-field cs-span-2">
                <div className="flex gap-2">
                  <button
                    type="button"
                    disabled={starting}
                    onClick={() => setBackend("electrum")}
                    className={`flex-1 py-2.5 rounded-lg text-sm font-semibold transition-all ${
                      backend === "electrum"
                        ? "bg-orange-600 text-white"
                        : "bg-gray-800 text-gray-400 hover:bg-gray-700"
                    }`}
                  >
                    Electrum
                  </button>
                  <button
                    type="button"
                    disabled={starting}
                    onClick={() => setBackend("bitcoind")}
                    className={`flex-1 py-2.5 rounded-lg text-sm font-semibold transition-all ${
                      backend === "bitcoind"
                        ? "bg-orange-600 text-white"
                        : "bg-gray-800 text-gray-400 hover:bg-gray-700"
                    }`}
                  >
                    Bitcoin Core
                  </button>
                </div>
                <p className="cs-hint">
                  {backend === "electrum"
                    ? "Uses a hosted Electrum server — no local node needed."
                    : "Uses your own Bitcoin Core node via RPC + ZMQ."}
                </p>
              </div>

              {backend === "bitcoind" && (
                <>
                  <Field
                    label="Bitcoin RPC endpoint"
                    required
                    hint="Format: host:port"
                    className="cs-span-2"
                  >
                    <input
                      className="cs-input"
                      name="bitcoinRpc"
                      value={bitcoinRpc}
                      onChange={(e) => setBitcoinRpc(e.target.value)}
                      placeholder="127.0.0.1:38332"
                      disabled={starting}
                      required
                    />
                  </Field>

                  <Field label="RPC username" required>
                    <input
                      className="cs-input"
                      name="bitcoinUser"
                      value={bitcoinUser}
                      onChange={(e) => setBitcoinUser(e.target.value)}
                      placeholder="user"
                      disabled={starting}
                      required
                    />
                  </Field>

                  <Field label="RPC password" required>
                    <div className="cs-input-wrap">
                      <input
                        className="cs-input"
                        type={showRpcPassword ? "text" : "password"}
                        name="bitcoinPassword"
                        value={bitcoinPassword}
                        onChange={(e) => setBitcoinPassword(e.target.value)}
                        placeholder="password"
                        disabled={starting}
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
                  </Field>

                  <Field
                    label="ZMQ endpoint"
                    required
                    hint="Subscribe to rawblock + rawtx notifications."
                    className="cs-span-2"
                  >
                    <input
                      className="cs-input"
                      name="zmq"
                      value={zmq}
                      onChange={(e) => setZmq(e.target.value)}
                      placeholder="tcp://127.0.0.1:28332"
                      disabled={starting}
                      required
                    />
                  </Field>
                </>
              )}
            </div>
          </section>

          {checksStarted && (
            <section className="cs-card cs-add-prechecks">
              <div className="cs-card-head">
                <div>
                  <h2>Startup checks</h2>
                  <p>Verifying connectivity before the maker starts.</p>
                </div>
              </div>
              <div className="cs-card-body">
                <div className="cs-add-checks">
                  {activeChecks.map((row) => (
                    <CheckRow
                      key={row.id}
                      row={row}
                      state={checks[row.id] ?? { status: "idle" }}
                    />
                  ))}
                </div>
              </div>
            </section>
          )}

          <div className="cs-add-actions">
            <Link to="/" className="cs-btn ghost">
              Cancel
            </Link>
            <button type="submit" className="cs-btn primary" disabled={starting}>
              {starting ? (
                <LoaderCircle size={18} className="cs-spin" />
              ) : (
                <Play size={18} />
              )}
              {starting ? "Starting maker..." : "Start maker"}
            </button>
          </div>
        </form>
      </main>
    </div>
  );
}
