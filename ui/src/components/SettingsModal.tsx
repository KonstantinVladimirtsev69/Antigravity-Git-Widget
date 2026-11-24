import { useState } from 'react';
import { normalizeVersion } from '../hooks/useAppUpdater';
import type { ScannerConfig, UpdateCheckResponse } from '../types';

interface SettingsPanelProps {
  config: ScannerConfig;
  busy: boolean;
  currentVersion?: string;
  updateInfo?: UpdateCheckResponse | null;
  checkingUpdates?: boolean;
  updateCheckError?: string;
  onCheckUpdates?: () => void;
  onOpenUpdateModal?: () => void;
  onSave: (cfg: ScannerConfig) => void;
  onInitRepo: (path: string) => Promise<boolean>;
  onClose: () => void;
}

export function SettingsPanel({
  config,
  busy,
  currentVersion,
  updateInfo,
  checkingUpdates = false,
  updateCheckError = '',
  onCheckUpdates,
  onOpenUpdateModal,
  onSave,
  onInitRepo,
  onClose,
}: SettingsPanelProps) {
  const [rootsText, setRootsText] = useState(config.roots.join('\n'));
  const [depth, setDepth] = useState(String(config.max_depth));
  const [updateRepo, setUpdateRepo] = useState(config.update_repo ?? '');
  const [newRepoPath, setNewRepoPath] = useState('');

  const versionLabel = normalizeVersion(currentVersion || updateInfo?.current_version) || '0.1.0';
  const latestLabel = normalizeVersion(updateInfo?.latest_version);

  const submit = () => {
    const roots = rootsText
      .split(/\r?\n/)
      .map((s) => s.trim())
      .filter(Boolean);
    const max_depth = Math.max(1, Math.min(6, Number(depth) || 3));
    onSave({
      ...config,
      roots,
      max_depth,
      update_repo: updateRepo.trim() || null,
    });
  };

  return (
    <div className="gw-card gw-stack">
      <div style={{ fontWeight: 600 }}>Scanner Roots</div>
      <textarea
        className="gw-textarea"
        rows={2}
        value={rootsText}
        onChange={(e) => setRootsText(e.target.value)}
      />
      <div className="gw-stack" style={{ gap: 4 }}>
        <span className="gw-muted">Update repository:</span>
        <input
          className="gw-input"
          placeholder="owner/repo (optional)"
          value={updateRepo}
          onChange={(e) => setUpdateRepo(e.target.value)}
        />
      </div>
      <div className="gw-row">
        <span className="gw-muted">Depth:</span>
        <input
          className="gw-input"
          style={{ width: 56 }}
          type="number"
          min={1}
          max={6}
          value={depth}
          onChange={(e) => setDepth(e.target.value)}
        />
        <button type="button" className="gw-btn gw-btn-primary" disabled={busy} onClick={submit}>
          Save
        </button>
        <button type="button" className="gw-btn" onClick={onClose}>
          Close
        </button>
      </div>

      <div style={{ borderTop: '1px solid var(--gw-border)', paddingTop: 6 }} className="gw-stack">
        <div className="gw-between">
          <span style={{ fontWeight: 600 }}>Widget Version &amp; Updates</span>
          <span className="gw-badge">v{versionLabel}</span>
        </div>
        <div className="gw-between" style={{ gap: 6 }}>
          <div style={{ fontSize: 11, minWidth: 0 }}>
            {updateCheckError ? (
              <span style={{ color: '#fca5a5' }}>{updateCheckError}</span>
            ) : updateInfo?.update_available && latestLabel ? (
              <span style={{ color: '#86efac' }}>Update v{latestLabel} available</span>
            ) : updateInfo && !updateInfo.error ? (
              <span className="gw-muted">Up to date</span>
            ) : (
              <span className="gw-muted">Check GitHub Releases for updates</span>
            )}
          </div>
          <div className="gw-row" style={{ flexShrink: 0 }}>
            {updateInfo?.update_available && latestLabel && onOpenUpdateModal && (
              <button
                type="button"
                className="gw-btn gw-btn-xs gw-btn-ok"
                onClick={onOpenUpdateModal}
              >
                View v{latestLabel}
              </button>
            )}
            {onCheckUpdates && (
              <button
                type="button"
                className="gw-btn gw-btn-xs"
                disabled={busy || checkingUpdates}
                onClick={onCheckUpdates}
              >
                {checkingUpdates ? 'Checking...' : 'Check for updates'}
              </button>
            )}
          </div>
        </div>
      </div>

      <div style={{ borderTop: '1px solid var(--gw-border)', paddingTop: 6 }} className="gw-stack">
        <span className="gw-muted">Init new Git repository:</span>
        <div className="gw-row">
          <input
            className="gw-input"
            placeholder="C:\Projects\my-new-app"
            value={newRepoPath}
            onChange={(e) => setNewRepoPath(e.target.value)}
          />
          <button
            type="button"
            className="gw-btn"
            disabled={busy || !newRepoPath.trim()}
            onClick={() => {
              const p = newRepoPath.trim();
              void onInitRepo(p).then((ok) => {
                if (ok) setNewRepoPath('');
              });
            }}
          >
            Init
          </button>
        </div>
      </div>
    </div>
  );
}
