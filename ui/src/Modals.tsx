import { useState } from 'react';
import type { ActionPreview, GuardedOpKind, ScannerConfig } from './types';

interface DirtyModalProps {
  kind: GuardedOpKind;
  target: string | null;
  files: string[];
  busy: boolean;
  onStashPop: () => void;
  onQuickCommit: (msg: string) => void;
  onCancel: () => void;
}

export function DirtyModal({
  kind,
  target,
  files,
  busy,
  onStashPop,
  onQuickCommit,
  onCancel,
}: DirtyModalProps) {
  const [msg, setMsg] = useState('wip: quick save before ' + kind);

  return (
    <div className="gw-card" style={{ borderColor: 'var(--gw-warn)' }}>
      <div style={{ fontWeight: 600 }}>
        Незакоммиченные изменения перед {kind}
        {target ? ` (${target})` : ''}
      </div>
      <ul className="gw-list">
        {files.slice(0, 8).map((f) => (
          <li key={f}>{f}</li>
        ))}
        {files.length > 8 && <li>...и ещё {files.length - 8}</li>}
      </ul>
      <div className="gw-stack" style={{ marginTop: 8 }}>
        <button className="gw-btn gw-btn-primary" disabled={busy} onClick={onStashPop}>
          Stash → {kind} → Stash pop
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
          Отмена
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
    <div className="gw-card" style={{ borderColor: 'var(--gw-warn)' }}>
      <div style={{ fontWeight: 600 }}>
        Подтверждение: {actionLabel} ({preview.source} → {preview.target})
      </div>
      <div className="gw-muted" style={{ marginTop: 2 }}>
        Защищённая ветка · коммитов: {preview.commit_count}
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
          Подтвердить {actionLabel}
        </button>
        <button className="gw-btn" disabled={busy} onClick={onCancel}>
          Отмена
        </button>
      </div>
    </div>
  );
}

interface SettingsPanelProps {
  config: ScannerConfig;
  busy: boolean;
  onSave: (cfg: ScannerConfig) => void;
  onClose: () => void;
}

export function SettingsPanel({ config, busy, onSave, onClose }: SettingsPanelProps) {
  const [rootsText, setRootsText] = useState(config.roots.join('\n'));
  const [depth, setDepth] = useState(String(config.max_depth));

  const submit = () => {
    const roots = rootsText
      .split(/\r?\n/)
      .map((s) => s.trim())
      .filter(Boolean);
    const max_depth = Math.max(1, Math.min(6, Number(depth) || 3));
    onSave({ roots, max_depth });
  };

  return (
    <div className="gw-card gw-stack">
      <div style={{ fontWeight: 600 }}>Папки сканирования</div>
      <textarea
        className="gw-textarea"
        rows={3}
        value={rootsText}
        onChange={(e) => setRootsText(e.target.value)}
      />
      <div className="gw-row">
        <span className="gw-muted">Глубина:</span>
        <input
          className="gw-input"
          style={{ width: 64 }}
          type="number"
          min={1}
          max={6}
          value={depth}
          onChange={(e) => setDepth(e.target.value)}
        />
        <button className="gw-btn gw-btn-primary" disabled={busy} onClick={submit}>
          Сохранить
        </button>
        <button className="gw-btn" onClick={onClose}>
          Закрыть
        </button>
      </div>
    </div>
  );
}
