import type { RepoStatus } from '../types';

interface ConflictBannerProps {
  status: RepoStatus;
  busy: boolean;
  onSendConflict: () => void;
  onContinueRebase: () => void;
  onAbortRebase: () => void;
  onAbortMerge: () => void;
}

export function ConflictBanner({
  status,
  busy,
  onSendConflict,
  onContinueRebase,
  onAbortRebase,
  onAbortMerge,
}: ConflictBannerProps) {
  if (!status.is_merging && !status.is_rebasing && status.conflicts.length === 0) {
    return null;
  }
  return (
    <div className="gw-card gw-stack" style={{ borderColor: 'rgba(239, 68, 68, 0.45)' }}>
      <strong style={{ color: '#fca5a5' }}>
        {status.is_rebasing ? 'Rebase Conflict' : 'Merge Conflict'}
      </strong>
      <ul className="gw-list">
        {status.conflicts.map((c) => (
          <li key={c}>{c}</li>
        ))}
      </ul>
      <div className="gw-row" style={{ flexWrap: 'wrap' }}>
        <button className="gw-btn gw-btn-primary" disabled={busy} onClick={onSendConflict}>
          Send to AI Chat
        </button>
        {status.is_rebasing ? (
          <>
            <button
              className="gw-btn gw-btn-ok"
              disabled={busy || status.conflicts.length > 0}
              onClick={onContinueRebase}
            >
              Continue Rebase
            </button>
            <button className="gw-btn gw-btn-danger" disabled={busy} onClick={onAbortRebase}>
              Abort Rebase
            </button>
          </>
        ) : (
          <button className="gw-btn gw-btn-danger" disabled={busy} onClick={onAbortMerge}>
            Abort Merge
          </button>
        )}
      </div>
    </div>
  );
}
