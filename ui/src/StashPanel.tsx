import { useState } from 'react';
import type { StashEntry } from './types';

interface StashPanelProps {
  stashes: StashEntry[];
  isDirty: boolean;
  busy: boolean;
  onSave: (message?: string) => void;
  onApply: (index: number) => void;
  onPop: (index: number) => void;
  onDrop: (index: number) => void;
}

export function StashPanel({
  stashes,
  isDirty,
  busy,
  onSave,
  onApply,
  onPop,
  onDrop,
}: StashPanelProps) {
  const [open, setOpen] = useState(false);
  const [msg, setMsg] = useState('');

  return (
    <div className="gw-card gw-stack">
      <div className="gw-between">
        <button
          type="button"
          className="gw-btn gw-btn-xs"
          style={{ border: 'none', background: 'transparent', padding: 0 }}
          onClick={() => setOpen((v) => !v)}
        >
          {open ? '▾' : '▸'} Stashes ({stashes.length})
        </button>
        <div className="gw-row" style={{ gap: 4 }}>
          <button
            type="button"
            className="gw-btn gw-btn-xs"
            disabled={busy || !isDirty}
            onClick={() => {
              onSave(msg.trim() || undefined);
              setMsg('');
            }}
          >
            Stash
          </button>
          <button
            type="button"
            className="gw-btn gw-btn-xs"
            disabled={busy || stashes.length === 0}
            onClick={() => onPop(0)}
          >
            Pop
          </button>
        </div>
      </div>

      {open && (
        <div className="gw-stack" style={{ gap: 5 }}>
          <input
            className="gw-input"
            placeholder="Optional stash message..."
            value={msg}
            onChange={(e) => setMsg(e.target.value)}
          />
          {stashes.length === 0 ? (
            <div className="gw-muted">No stashes saved.</div>
          ) : (
            <div className="gw-stack" style={{ gap: 4, maxHeight: 130, overflowY: 'auto' }}>
              {stashes.map((s) => (
                <div
                  key={s.reference}
                  className="gw-between"
                  style={{ padding: '3px 4px', borderTop: '1px solid var(--gw-border)' }}
                >
                  <div style={{ minWidth: 0, flex: 1 }}>
                    <div
                      style={{
                        fontSize: 11.5,
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                        whiteSpace: 'nowrap',
                      }}
                    >
                      {s.message}
                    </div>
                    <div className="gw-muted">
                      {s.reference} · {s.date_rel}
                    </div>
                  </div>
                  <div className="gw-row" style={{ gap: 3 }}>
                    <button
                      type="button"
                      className="gw-btn gw-btn-xs"
                      disabled={busy}
                      onClick={() => onApply(s.index)}
                    >
                      Apply
                    </button>
                    <button
                      type="button"
                      className="gw-btn gw-btn-xs"
                      disabled={busy}
                      onClick={() => onPop(s.index)}
                    >
                      Pop
                    </button>
                    <button
                      type="button"
                      className="gw-btn gw-btn-xs gw-btn-danger"
                      disabled={busy}
                      onClick={() => onDrop(s.index)}
                    >
                      ✕
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
