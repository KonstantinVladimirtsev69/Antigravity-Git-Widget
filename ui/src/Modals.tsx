import { useState } from 'react';
import type { ActionPreview, DualBranchSync, GuardedOpKind } from './types';

export { RemoteModal } from './RemoteModal';
export { SettingsPanel } from './components/SettingsModal';

interface DirtyModalProps {
  kind: GuardedOpKind;
  actionLabel?: string;
  target: string | null;
  files: string[];
  busy: boolean;
  onStashPop: () => void;
  onQuickCommit: (msg: string) => void;
  onCancel: () => void;
}

export function DirtyModal({
  kind,
  actionLabel,
  target,
  files,
  busy,
  onStashPop,
  onQuickCommit,
  onCancel,
}: DirtyModalProps) {
  const label = actionLabel ?? kind;
  const [msg, setMsg] = useState('wip: save before ' + label);

  return (
    <div className="gw-card" style={{ borderColor: 'rgba(245, 158, 11, 0.45)' }}>
      <div style={{ fontWeight: 600 }}>
        Uncommitted changes before {label}
        {target ? ` (${target})` : ''}
      </div>
      <ul className="gw-list">
        {files.slice(0, 8).map((f) => (
          <li key={f}>{f}</li>
        ))}
        {files.length > 8 && <li>...and {files.length - 8} more</li>}
      </ul>
      <div className="gw-stack" style={{ marginTop: 8 }}>
        <button className="gw-btn gw-btn-primary" disabled={busy} onClick={onStashPop}>
          Stash → {label} → Stash pop
        </button>
        <div className="gw-row">
          <input
            className="gw-input"
            value={msg}
            onChange={(e) => setMsg(e.target.value)}
            placeholder="Commit message"
          />
          <button
            className="gw-btn"
            disabled={busy || !msg.trim()}
            onClick={() => onQuickCommit(msg.trim())}
          >
            Commit
          </button>
        </div>
        <button className="gw-btn" disabled={busy} onClick={onCancel}>
          Cancel
        </button>
      </div>
    </div>
  );
}

interface PreviewModalProps {
  actionLabel: string;
  preview: ActionPreview;
  busy: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function PreviewModal({
  actionLabel,
  preview,
  busy,
  onConfirm,
  onCancel,
}: PreviewModalProps) {
  return (
    <div className="gw-card" style={{ borderColor: 'rgba(245, 158, 11, 0.45)' }}>
      <div style={{ fontWeight: 600 }}>
        Confirm {actionLabel}: {preview.source} → {preview.target}
      </div>
      <div className="gw-muted" style={{ marginTop: 2 }}>
        Protected branch · commits: {preview.commit_count}
      </div>
      {preview.commits.length > 0 && (
        <ul className="gw-list">
          {preview.commits.slice(0, 10).map((c) => (
            <li key={c}>{c}</li>
          ))}
        </ul>
      )}
      <div className="gw-row" style={{ marginTop: 8 }}>
        <button className="gw-btn gw-btn-primary" disabled={busy} onClick={onConfirm}>
          Confirm {actionLabel}
        </button>
        <button className="gw-btn" disabled={busy} onClick={onCancel}>
          Cancel
        </button>
      </div>
    </div>
  );
}

interface UnpulledModalProps {
  modeLabel: string;
  sync: DualBranchSync;
  busy: boolean;
  onPullAndContinue: () => void;
  onContinueWithoutPull: () => void;
  onCancel: () => void;
}

export function UnpulledModal({
  modeLabel,
  sync,
  busy,
  onPullAndContinue,
  onContinueWithoutPull,
  onCancel,
}: UnpulledModalProps) {
  return (
    <div className="gw-card" style={{ borderColor: 'rgba(245, 158, 11, 0.45)' }}>
      <div style={{ fontWeight: 600 }}>Unpulled commits detected before {modeLabel}</div>
      <div className="gw-muted" style={{ marginTop: 2 }}>
        Pulling both branches before integrating helps avoid hidden conflicts:
      </div>
      <ul className="gw-list">
        <li>
          <strong>{sync.source.branch}</strong>:{' '}
          {sync.source.behind > 0 ? `↓${sync.source.behind} behind` : 'up to date'}
        </li>
        <li>
          <strong>{sync.target.branch}</strong>:{' '}
          {sync.target.behind > 0 ? `↓${sync.target.behind} behind` : 'up to date'}
        </li>
      </ul>
      <div className="gw-stack" style={{ marginTop: 8 }}>
        <button className="gw-btn gw-btn-primary" disabled={busy} onClick={onPullAndContinue}>
          Pull both &amp; {modeLabel}
        </button>
        <div className="gw-row">
          <button className="gw-btn" style={{ flex: 1 }} disabled={busy} onClick={onContinueWithoutPull}>
            {modeLabel} anyway
          </button>
          <button className="gw-btn" disabled={busy} onClick={onCancel}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
