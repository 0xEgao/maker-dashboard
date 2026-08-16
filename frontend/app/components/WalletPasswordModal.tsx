import { useState } from "react";
import { createPortal } from "react-dom";
import { Eye, EyeOff, LoaderCircle } from "lucide-react";

interface WalletPasswordModalProps {
  submitting: boolean;
  onSubmit: (password: string) => void;
  onCancel: () => void;
}

/**
 * Shown when a maker's wallet cannot be opened without a password.
 * The password is passed to POST /makers/{id}/start, used once to open the
 * wallet, and never stored.
 */
export function WalletPasswordModal({
  submitting,
  onSubmit,
  onCancel,
}: WalletPasswordModalProps) {
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (submitting) return;
    onSubmit(password);
  }

  return createPortal(
    <div className="cs-modal-backdrop">
      <div className="cs-modal">
        <h2 className="mb-4 text-xl font-bold">Wallet Password Required</h2>
        <form onSubmit={handleSubmit}>
          <div className="cs-field mb-6">
            <div className="cs-input-wrap">
              <input
                className="cs-input"
                type={showPassword ? "text" : "password"}
                name="walletPassword"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="Wallet password"
                disabled={submitting}
                autoFocus
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
          </div>
          <div className="flex gap-3">
            <button
              type="button"
              onClick={onCancel}
              disabled={submitting}
              className="cs-btn ghost flex-1"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={submitting}
              className="cs-btn primary flex-1"
            >
              {submitting ? (
                <LoaderCircle size={16} className="cs-spin" />
              ) : null}
              {submitting ? "Starting..." : "Start"}
            </button>
          </div>
        </form>
      </div>
    </div>,
    document.body,
  );
}
