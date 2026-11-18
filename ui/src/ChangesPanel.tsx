import { useState } from 'react';
import type { FileChange } from './types';

interface ChangesPanelProps {
  staged: FileChange[];
  unstaged: FileChange[];
  busy: boolean;
  onCommit: (msg: string, amend: boolean) => Promise<void>;
  onSuggestCommit: () => Promise<void>;
  onUndoCommit: () => Promise<void>;
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
  onDiscardFile: (path: string) => void;
  onStageAll: () => void;
  onUnstageAll: () => void;
  onDiscardAll: () => void;
  onOpenDiff: (path: string) => void;
}

function splitPath(fullPath: string): { name: string; dir: string } {
  const norm = fullPath.replace(/\\/g, '/');
  const idx = norm.lastIndexOf('/');
  if (idx === -1) return { name: norm, dir: '' };
  return { name: norm.slice(idx + 1), dir: norm.slice(0, idx) };
}

function statusClass(code: string): string {
  const ch = code.charAt(0).toUpperCase();
  if (ch === 'A' || ch === 'U' || ch === '?') return 'gw-st-add';
  if (ch === 'D') return 'gw-st-del';
  return 'gw-st-mod';
}

export function ChangesPanel({
  staged,
  unstaged,
  busy,
  onCommit,
  onSuggestCommit,
  onUndoCommit,
  onStageFile,
  onUnstageFile,
  onDiscardFile,
  onStageAll,
  onUnstageAll,
  onDiscardAll,
  onOpenDiff,
}: ChangesPanelProps) {
  const [open, setOpen] = useState(true);
  const [msg, setMsg] = useState('');
  const [amend, setAmend] = useState(false);
  const [confirmDiscardAll, setConfirmDiscardAll] = useState(false);

  const totalCount = staged.length + unstaged.length;
  const canCommit = !busy && msg.trim().length > 0 && (totalCount > 0 || amend);

  const submitCommit = () => {
    if (!canCommit) return;
    const text = msg.trim();
    const isAmend = amend;
    setMsg('');
    setAmend(false);
    void onCommit(text, isAmend);
  };

  return (
    <div className="gw-card gw-stack" style={{ padding: '7px 8px' }}>
      <div className="gw-between">
        <button
          type="button"
          className="gw-btn gw-btn-xs"
          style={{ border: 'none', background: 'transparent', padding: 0, fontWeight: 600 }}
          onClick={() => setOpen((v) => !v)}
        >
          {open ? '▾' : '▸'} Changes
        </button>
        <div className="gw-row" style={{ gap: 4 }}>
          {totalCount > 0 && <span className="gw-count-pill">{totalCount}</span>}
          <button
            type="button"
            className="gw-btn gw-btn-xs"
            disabled={busy}
            title="Undo Last Commit (git reset --soft HEAD~1)"
            onClick={() => void onUndoCommit()}
          >
            Undo
          </button>
        </div>
      </div>

      {open && (
        <>
          <div className="gw-row" style={{ gap: 4 }}>
            <input
              className="gw-input"
              placeholder="Message (Ctrl+Enter to commit)"
              value={msg}
              onChange={(e) => setMsg(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) submitCommit();
              }}
            />
            <button
              type="button"
              className="gw-btn"
              disabled={busy || totalCount === 0}
              title="Ask AI chat to generate commit message"
              onClick={() => void onSuggestCommit()}
            >
              ✨
            </button>
          </div>

          <div className="gw-row" style={{ gap: 6 }}>
            <button
              type="button"
              className="gw-btn gw-btn-primary"
              style={{ flex: 1 }}
              disabled={!canCommit}
              onClick={submitCommit}
            >
              ✓ {amend ? 'Amend' : staged.length > 0 ? `Commit (${staged.length} staged)` : 'Commit'}
            </button>
            <label className="gw-row gw-muted" style={{ cursor: 'pointer', gap: 4 }}>
              <input type="checkbox" checked={amend} onChange={(e) => setAmend(e.target.checked)} />
              Amend
            </label>
          </div>

          {staged.length > 0 && (
            <div className="gw-stack" style={{ gap: 2 }}>
              <div className="gw-between">
                <span className="gw-muted" style={{ fontWeight: 600 }}>
                  Staged Changes
                </span>
                <div className="gw-row" style={{ gap: 4 }}>
                  <span className="gw-count-pill">{staged.length}</span>
                  <button
                    type="button"
                    className="gw-btn gw-btn-xs"
                    disabled={busy}
                    title="Unstage all"
                    onClick={onUnstageAll}
                  >
                    −
                  </button>
                </div>
              </div>
              <div className="gw-stack" style={{ gap: 1, maxHeight: 110, overflowY: 'auto' }}>
                {staged.map((c) => {
                  const { name, dir } = splitPath(c.path);
                  return (
                    <div key={`S:${c.path}`} className="gw-row" style={{ gap: 3 }}>
                      <button
                        type="button"
                        className="gw-file-row"
                        style={{ flex: 1, minWidth: 0 }}
                        onClick={() => onOpenDiff(c.path)}
                      >
                        <span className="gw-file-name">{name}</span>
                        {dir && <span className="gw-file-dir">{dir}</span>}
                        <span className={`gw-file-st ${statusClass(c.code)}`}>{c.code}</span>
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
                  );
                })}
              </div>
            </div>
          )}

          {unstaged.length > 0 && (
            <div className="gw-stack" style={{ gap: 2 }}>
              <div className="gw-between">
                <span className="gw-muted" style={{ fontWeight: 600 }}>
                  Changes
                </span>
                <div className="gw-row" style={{ gap: 4 }}>
                  <span className="gw-count-pill">{unstaged.length}</span>
                  <button
                    type="button"
                    className="gw-btn gw-btn-xs"
                    disabled={busy}
                    title="Stage all"
                    onClick={onStageAll}
                  >
                    +
                  </button>
                  {!confirmDiscardAll ? (
                    <button
                      type="button"
                      className="gw-btn gw-btn-xs gw-btn-danger"
                      disabled={busy}
                      title="Discard all unstaged changes"
                      onClick={() => setConfirmDiscardAll(true)}
                    >
                      ↺
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
                        Discard?
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
              <div className="gw-stack" style={{ gap: 1, maxHeight: 140, overflowY: 'auto' }}>
                {unstaged.map((c) => {
                  const { name, dir } = splitPath(c.path);
                  return (
                    <div key={`U:${c.path}`} className="gw-row" style={{ gap: 3 }}>
                      <button
                        type="button"
                        className="gw-file-row"
                        style={{ flex: 1, minWidth: 0 }}
                        onClick={() => onOpenDiff(c.path)}
                      >
                        <span className="gw-file-name">{name}</span>
                        {dir && <span className="gw-file-dir">{dir}</span>}
                        <span className={`gw-file-st ${statusClass(c.code)}`}>{c.code}</span>
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
                        title="Discard file"
                        onClick={() => onDiscardFile(c.path)}
                      >
                        ↺
                      </button>
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </>
      )}
    </div>
  );
}
