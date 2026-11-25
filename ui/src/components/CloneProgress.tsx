import { useState } from 'react';
import type { CloneStatusResponse } from '../types';

interface CloneProgressProps {
  status: CloneStatusResponse;
  cancelling: boolean;
  onCancel: () => void;
}

export function CloneProgress({ status, cancelling, onCancel }: CloneProgressProps) {
  const [showLog, setShowLog] = useState(false);

  if (status.stage === 'idle') return null;

  const pct = Math.max(0, Math.min(100, status.percent));
  const barColor =
    status.stage === 'error'
      ? 'var(--gw-danger)'
      : status.stage === 'cancelled'
        ? 'var(--gw-warn)'
        : status.stage === 'done'
          ? 'var(--gw-ok)'
          : 'var(--gw-accent)';

  return (
    <div className="gw-stack" style={{ gap: 5 }}>
      <div className="gw-between">
        <span style={{ fontSize: 11.5, fontWeight: 500 }}>{status.phase_label}</span>
        <span className="gw-muted">{pct}%</span>
      </div>

      <div style={{ height: 6, width: '100%', borderRadius: 999, background: 'rgba(255, 255, 255, 0.08)', overflow: 'hidden' }}>
        <div style={{ height: '100%', width: `${pct}%`, background: barColor, transition: 'width 150ms ease' }} />
      </div>

      {status.error && (
        <div style={{ color: '#fca5a5', fontSize: 11, wordBreak: 'break-word' }}>{status.error}</div>
      )}

      <div className="gw-between">
        {status.log_tail.length > 0 ? (
          <button type="button" className="gw-btn gw-btn-xs" onClick={() => setShowLog((v) => !v)}>
            {showLog ? '▾' : '▸'} Лог ({status.log_tail.length})
          </button>
        ) : (
          <span />
        )}

        {status.in_progress && (
          <button type="button" className="gw-btn gw-btn-xs gw-btn-danger" disabled={cancelling} onClick={onCancel}>
            {cancelling ? 'Cancelling...' : 'Cancel'}
          </button>
        )}
      </div>

      {showLog && status.log_tail.length > 0 && (
        <pre
          style={{
            margin: 0,
            padding: '6px 8px',
            maxHeight: 130,
            overflowY: 'auto',
            borderRadius: 6,
            background: 'rgba(0, 0, 0, 0.32)',
            border: '1px solid var(--gw-border)',
            fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
            fontSize: 10.5,
            whiteSpace: 'pre-wrap',
            wordBreak: 'break-word',
          }}
        >
          {status.log_tail.join('\n')}
        </pre>
      )}
    </div>
  );
}
