import { useEffect } from 'react';
import { normalizeVersion } from '../hooks/useAppUpdater';
import type { UpdateCheckResponse, UpdateStage, UpdateStatusResponse } from '../types';

const STEPS: { key: UpdateStage; label: string }[] = [
  { key: 'downloading', label: 'Downloading' },
  { key: 'verifying', label: 'Verifying SHA-256' },
  { key: 'installing', label: 'Installing' },
  { key: 'restarting', label: 'Restarting' },
];

interface UpdateModalProps {
  info: UpdateCheckResponse;
  status: UpdateStatusResponse | null;
  installing: boolean;
  restartTimedOut: boolean;
  reloadSuggested: boolean;
  onInstall: () => void;
  onClose: () => void;
}

export function UpdateModal({
  info,
  status,
  installing,
  restartTimedOut,
  reloadSuggested,
  onInstall,
  onClose,
}: UpdateModalProps) {
  const currentVer = normalizeVersion(status?.current_version ?? info.current_version) || '0.1.0';
  const latestVer = normalizeVersion(status?.target_version ?? info.latest_version) || currentVer;
  const stage = status?.stage ?? 'idle';
  const isError = stage === 'error';
  const canClose = !installing || isError || restartTimedOut || reloadSuggested;
  const safeHtmlUrl = info.html_url && info.html_url.startsWith('https://') ? info.html_url : null;
  const activeIdx = STEPS.findIndex((s) => s.key === stage);
  const statusMsg = status?.message?.trim() ?? '';

  useEffect(() => {
    if (!canClose) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [canClose, onClose]);

  return (
    <div
      className="gw-card gw-stack"
      role="dialog"
      aria-modal="true"
      aria-labelledby="gw-update-modal-title"
      style={{ borderColor: 'rgba(59, 130, 246, 0.45)' }}
    >
      <div className="gw-between">
        <div id="gw-update-modal-title" style={{ fontWeight: 600 }}>
          Update available: v{currentVer} → v{latestVer}
        </div>
        <button
          type="button"
          className="gw-btn gw-btn-xs"
          aria-label="Close update dialog"
          disabled={!canClose}
          onClick={onClose}
        >
          ✕
        </button>
      </div>

      <div className="gw-between">
        <span className="gw-muted">{info.repo}</span>
        {safeHtmlUrl && (
          <a
            className="gw-btn gw-btn-xs"
            style={{ textDecoration: 'none' }}
            href={safeHtmlUrl}
            target="_blank"
            rel="noreferrer"
          >
            Open on GitHub ↗
          </a>
        )}
      </div>

      <div
        style={{
          maxHeight: 240,
          overflowY: 'auto',
          whiteSpace: 'pre-wrap',
          wordBreak: 'break-word',
          padding: '6px 8px',
          borderRadius: 6,
          background: 'rgba(0, 0, 0, 0.25)',
          border: '1px solid var(--gw-border)',
          fontSize: 11.5,
        }}
      >
        {info.release_notes.trim() || 'No release notes provided.'}
      </div>

      {status && stage !== 'idle' && (
        <div className="gw-stack" style={{ gap: 5 }}>
          <div className="gw-row" style={{ flexWrap: 'wrap', gap: 4 }}>
            {STEPS.map((step, idx) => {
              const done = activeIdx > idx;
              const active = activeIdx === idx;
              const cls = done
                ? 'gw-badge gw-badge-ok'
                : active
                  ? isError
                    ? 'gw-badge gw-badge-danger'
                    : 'gw-badge gw-badge-warn'
                  : 'gw-badge';
              return (
                <span key={step.key} className={cls}>
                  {done ? '✓ ' : active ? '● ' : ''}
                  {step.label}
                </span>
              );
            })}
          </div>
          {statusMsg && !isError && <div className="gw-muted">{statusMsg}</div>}
          {isError && status.error && (
            <div style={{ color: '#fca5a5', fontSize: 11 }}>{status.error}</div>
          )}
        </div>
      )}

      {(restartTimedOut || reloadSuggested) && (
        <div
          className="gw-between"
          style={{
            padding: '6px 8px',
            borderRadius: 6,
            background: 'rgba(245, 158, 11, 0.1)',
            border: '1px solid rgba(245, 158, 11, 0.35)',
          }}
        >
          <span style={{ color: '#fcd34d', fontSize: 11 }}>
            {reloadSuggested
              ? 'Update installed (session token rotated). Reload the panel to connect.'
              : 'Restart is taking longer than expected. Reload the panel manually.'}
          </span>
          <button
            type="button"
            className="gw-btn gw-btn-xs gw-btn-primary"
            onClick={() => window.location.reload()}
          >
            Reload panel
          </button>
        </div>
      )}

      <div className="gw-row">
        <button
          type="button"
          className="gw-btn gw-btn-primary"
          style={{ flex: 1 }}
          disabled={installing || (!info.update_available && !isError)}
          onClick={onInstall}
        >
          {installing ? 'Installing...' : 'Install & Restart'}
        </button>
        <button type="button" className="gw-btn" disabled={!canClose} onClick={onClose}>
          Close
        </button>
      </div>
    </div>
  );
}
