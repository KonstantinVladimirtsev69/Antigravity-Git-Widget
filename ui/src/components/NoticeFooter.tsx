import { useEffect } from 'react';

interface NoticeFooterProps {
  notice: string;
  error: string;
  fetchWarning?: string | null;
  onClearNotice: () => void;
  onRetryFetch?: () => void;
  onDismissFetchWarning?: () => void;
}

export function NoticeFooter({
  notice,
  error,
  fetchWarning,
  onClearNotice,
  onRetryFetch,
  onDismissFetchWarning,
}: NoticeFooterProps) {
  useEffect(() => {
    if (!fetchWarning || !onDismissFetchWarning) return;
    const id = window.setTimeout(() => onDismissFetchWarning(), 10000);
    return () => window.clearTimeout(id);
  }, [fetchWarning, onDismissFetchWarning]);

  return (
    <>
      {fetchWarning && (
        <div
          className="gw-card gw-between"
          style={{ borderColor: 'rgba(245, 158, 11, 0.45)', color: '#fde68a', flexWrap: 'wrap', alignItems: 'center' }}
        >
          <span style={{ flex: '1 1 160px', minWidth: 0, wordBreak: 'break-word' }}>
            Remote fetch warning: {fetchWarning}
          </span>
          <div className="gw-row" style={{ gap: 4 }}>
            {onRetryFetch && (
              <button type="button" className="gw-btn gw-btn-xs" onClick={onRetryFetch}>
                Retry
              </button>
            )}
            {onDismissFetchWarning && (
              <button type="button" className="gw-btn gw-btn-xs" onClick={onDismissFetchWarning}>
                ✕
              </button>
            )}
          </div>
        </div>
      )}
      {notice && (
        <div className="gw-card gw-muted gw-between" style={{ alignItems: 'flex-start' }}>
          <div style={{ maxHeight: 72, overflowY: 'auto', whiteSpace: 'pre-wrap', flex: 1 }}>
            {notice}
          </div>
          <button
            type="button"
            className="gw-btn gw-btn-xs"
            style={{ marginLeft: 6 }}
            onClick={onClearNotice}
          >
            ✕
          </button>
        </div>
      )}
      {error && (
        <div className="gw-card" style={{ borderColor: 'rgba(239, 68, 68, 0.45)', color: '#fca5a5' }}>
          {error}
        </div>
      )}
    </>
  );
}
