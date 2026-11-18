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
  const [msg, setMsg] = useState('wip: save before ' + kind);

  return (
    <div className="gw-card" style={{ borderColor: 'rgba(245, 158, 11, 0.45)' }}>
      <div style={{ fontWeight: 600 }}>
        Uncommitted changes before {kind}
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

interface SettingsPanelProps {
  config: ScannerConfig;
  busy: boolean;
  onSave: (cfg: ScannerConfig) => void;
  onInitRepo: (path: string) => void;
  onClose: () => void;
}

export function SettingsPanel({
  config,
  busy,
  onSave,
  onInitRepo,
  onClose,
}: SettingsPanelProps) {
  const [rootsText, setRootsText] = useState(config.roots.join('\n'));
  const [depth, setDepth] = useState(String(config.max_depth));
  const [newRepoPath, setNewRepoPath] = useState('');

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
      <div style={{ fontWeight: 600 }}>Scanner Roots</div>
      <textarea
        className="gw-textarea"
        rows={2}
        value={rootsText}
        onChange={(e) => setRootsText(e.target.value)}
      />
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
        <button className="gw-btn gw-btn-primary" disabled={busy} onClick={submit}>
          Save
        </button>
        <button className="gw-btn" onClick={onClose}>
          Close
        </button>
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
            className="gw-btn"
            disabled={busy || !newRepoPath.trim()}
            onClick={() => {
              onInitRepo(newRepoPath.trim());
              setNewRepoPath('');
            }}
          >
            Init
          </button>
        </div>
      </div>
    </div>
  );
}

interface RemoteModalProps {
  defaultRepoName: string;
  hasSavedToken: boolean;
  busy: boolean;
  onAddExisting: (name: string, url: string) => Promise<void>;
  onCreateGithub: (payload: {
    name: string;
    description: string;
    private: boolean;
    use_ssh: boolean;
    token?: string;
  }) => Promise<void>;
  onClose: () => void;
}

export function RemoteModal({
  defaultRepoName,
  hasSavedToken,
  busy,
  onAddExisting,
  onCreateGithub,
  onClose,
}: RemoteModalProps) {
  const [mode, setMode] = useState<'github' | 'existing'>('github');
  const [ghName, setGhName] = useState(defaultRepoName);
  const [ghDesc, setGhDesc] = useState('');
  const [isPrivate, setIsPrivate] = useState(true);
  const [useSsh, setUseSsh] = useState(false);
  const [token, setToken] = useState('');
  const [remoteName, setRemoteName] = useState('origin');
  const [remoteUrl, setRemoteUrl] = useState('');

  return (
    <div className="gw-card gw-stack">
      <div className="gw-between">
        <div className="gw-tabs" style={{ flex: 1 }}>
          <button
            type="button"
            className={`gw-tab ${mode === 'github' ? 'active' : ''}`}
            onClick={() => setMode('github')}
          >
            Create on GitHub
          </button>
          <button
            type="button"
            className={`gw-tab ${mode === 'existing' ? 'active' : ''}`}
            onClick={() => setMode('existing')}
          >
            Link Existing URL
          </button>
        </div>
        <button className="gw-btn gw-btn-xs" onClick={onClose}>
          ✕
        </button>
      </div>

      {mode === 'github' ? (
        <div className="gw-stack" style={{ gap: 6 }}>
          <input
            className="gw-input"
            placeholder="Repository name (e.g. git-widget)"
            value={ghName}
            onChange={(e) => setGhName(e.target.value)}
          />
          <input
            className="gw-input"
            placeholder="Description (optional)"
            value={ghDesc}
            onChange={(e) => setGhDesc(e.target.value)}
          />
          <div className="gw-between">
            <div className="gw-tabs" style={{ width: 150 }}>
              <button
                type="button"
                className={`gw-tab ${isPrivate ? 'active' : ''}`}
                onClick={() => setIsPrivate(true)}
              >
                🔒 Private
              </button>
              <button
                type="button"
                className={`gw-tab ${!isPrivate ? 'active' : ''}`}
                onClick={() => setIsPrivate(false)}
              >
                🌐 Public
              </button>
            </div>
            <label className="gw-row gw-muted" style={{ cursor: 'pointer', gap: 4 }}>
              <input
                type="checkbox"
                checked={useSsh}
                onChange={(e) => setUseSsh(e.target.checked)}
              />
              SSH remote
            </label>
          </div>
          <input
            className="gw-input"
            type="password"
            placeholder={
              hasSavedToken
                ? 'GitHub Token saved ✓ (leave blank or enter new)'
                : 'GitHub PAT token (optional if GCM / gh is signed in)'
            }
            value={token}
            onChange={(e) => setToken(e.target.value)}
          />
          <button
            type="button"
            className="gw-btn gw-btn-primary"
            disabled={busy || !ghName.trim()}
            onClick={() =>
              void onCreateGithub({
                name: ghName.trim(),
                description: ghDesc.trim(),
                private: isPrivate,
                use_ssh: useSsh,
                token: token.trim() || undefined,
              })
            }
          >
            Create &amp; Publish to GitHub
          </button>
        </div>
      ) : (
        <div className="gw-stack" style={{ gap: 6 }}>
          <div className="gw-row">
            <input
              className="gw-input"
              style={{ width: 84 }}
              value={remoteName}
              onChange={(e) => setRemoteName(e.target.value)}
              placeholder="origin"
            />
            <input
              className="gw-input"
              value={remoteUrl}
              onChange={(e) => setRemoteUrl(e.target.value)}
              placeholder="https://github.com/user/repo.git"
            />
          </div>
          <button
            type="button"
            className="gw-btn gw-btn-primary"
            disabled={busy || !remoteName.trim() || !remoteUrl.trim()}
            onClick={() => void onAddExisting(remoteName.trim(), remoteUrl.trim())}
          >
            Save Remote
          </button>
        </div>
      )}
    </div>
  );
}
