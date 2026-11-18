import { useState } from 'react';
import type { FileChange } from './types';

interface ChangesPanelProps {
  staged: FileChange[];
  unstaged: FileChange[];
  busy: boolean;
  onCommit: (msg: string, amend: boolean) => Promise<void>;
  onUndoCommit: () => Promise<void>;
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
  onDiscardFile: (path: string) => void;
  onStageAll: () => void;
  onUnstageAll: () => void;
  onDiscardAll: () => void;
  onOpenDiff: (path: string) => void;
}

export function ChangesPanel({
  staged,
  unstaged,
  busy,
  onCommit,
  onUndoCommit,
  onStageFile,
  onUnstageFile,
  onDiscardFile,
  onStageAll,
  onUnstageAll,
  onDiscardAll,
  onOpenDiff,
}: ChangesPanelProps) {
  const [msg, setMsg] = useState('');
  const [amend, setAmend] = useState(false);
  const [confirmDiscardAll, setConfirmDiscardAll] = useState(false);

  const canCommit = !busy && msg.trim().length > 0 && (staged.length > 0 || unstaged.length > 0 || amend);

  const submitCommit = () => {
    if (!canCommit) return;
    const text = msg.trim();
    const isAmend = amend;
    setMsg('');
    setAmend(false);
    void onCommit(text, isAmend);
  };

  return (
    <div className="gw-card gw-stack">
      <div className="gw-row">
        <input
          className="gw-input"
          placeholder={staged.length > 0 ? `Commit ${staged.length} staged...` : 'Commit message...'}
          value={msg}
          onChange={(e) => setMsg(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) submitCommit();
          }}
        />
        <button className="gw-btn gw-btn-primary" disabled={!canCommit} onClick={submitCommit}>
          {amend ? 'Amend' : 'Commit'}
        </button>
      </div>

      <div className="gw-between">
        <label className="gw-row gw-muted" style={{ cursor: 'pointer' }}>
          <input type="checkbox" checked={amend} onChange={(e) => setAmend(e.target.checked)} />
          Amend
        </label>
        <button
          type="button"
          className="gw-btn gw-btn-xs"
          disabled={busy}
          title="git reset --soft HEAD~1 (keeps changes staged)"
          onClick={() => void onUndoCommit()}
        >
          Undo Last Commit
        </button>
      </div>

      {staged.length > 0 && (
        <div className="gw-stack" style={{ gap: 2 }}>
          <div className="gw-between">
            <span className="gw-muted" style={{ fontWeight: 600 }}>
              Staged Changes ({staged.length})
            </span>
            <button
              type="button"
              className="gw-btn gw-btn-xs"
              disabled={busy}
              title="Unstage all"
              onClick={onUnstageAll}
            >
              − All
            </button>
          </div>
          <div className="gw-stack" style={{ gap: 1, maxHeight: 110, overflowY: 'auto' }}>
            {staged.map((c) => (
              <div key={`S:${c.path}`} className="gw-row" style={{ gap: 4 }}>
                <button
                  type="button"
                  className="gw-picker-item gw-between"
                  style={{ padding: '2px 5px', flex: 1, minWidth: 0 }}
                  onClick={() => onOpenDiff(c.path)}
                >
                  <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {c.path}
                  </span>
                  <span className="gw-badge gw-badge-ok" style={{ marginLeft: 4 }}>
                    {c.code}
                  </span>
                </button>
                <button
                  type="button"
                  className="gw-btn gw-btn-xs"
                  disabled={busy}
                  title="Unstage file"
                  onClick={() => onUnstageFile(c.path)}
                >
                  −
                </button>
              </div>
            ))}
          </div>
        </div>
      )}

      {unstaged.length > 0 && (
        <div className="gw-stack" style={{ gap: 2 }}>
          <div className="gw-between">
            <span className="gw-muted" style={{ fontWeight: 600 }}>
              Changes ({unstaged.length})
            </span>
            <div className="gw-row" style={{ gap: 4 }}>
              <button
                type="button"
                className="gw-btn gw-btn-xs"
                disabled={busy}
                title="Stage all"
                onClick={onStageAll}
              >
                + All
              </button>
              {!confirmDiscardAll ? (
                <button
                  type="button"
                  className="gw-btn gw-btn-xs gw-btn-danger"
                  disabled={busy}
                  title="Discard all unstaged changes"
                  onClick={() => setConfirmDiscardAll(true)}
                >
                  ↺ All
                </button>
              ) : (
                <>
                  <button
                    type="button"
                    className="gw-btn gw-btn-xs gw-btn-danger"
                    disabled={busy}
                    onClick={() => {
                      setConfirmDiscardAll(false);
                      onDiscardAll();
                    }}
                  >
                    Confirm?
                  </button>
                  <button
                    type="button"
                    className="gw-btn gw-btn-xs"
                    onClick={() => setConfirmDiscardAll(false)}
                  >
                    ✕
                  </button>
                </>
              )}
            </div>
          </div>
          <div className="gw-stack" style={{ gap: 1, maxHeight: 130, overflowY: 'auto' }}>
            {unstaged.map((c) => (
              <div key={`U:${c.path}`} className="gw-row" style={{ gap: 4 }}>
                <button
                  type="button"
                  className="gw-picker-item gw-between"
                  style={{ padding: '2px 5px', flex: 1, minWidth: 0 }}
                  onClick={() => onOpenDiff(c.path)}
                >
                  <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {c.path}
                  </span>
                  <span className="gw-badge gw-badge-warn" style={{ marginLeft: 4 }}>
                    {c.code}
                  </span>
                </button>
                <button
                  type="button"
                  className="gw-btn gw-btn-xs"
                  disabled={busy}
                  title="Stage file"
                  onClick={() => onStageFile(c.path)}
                >
                  +
                </button>
                <button
                  type="button"
                  className="gw-btn gw-btn-xs gw-btn-danger"
                  disabled={busy}
                  title="Discard file changes"
                  onClick={() => onDiscardFile(c.path)}
                >
                  ↺
                </button>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
