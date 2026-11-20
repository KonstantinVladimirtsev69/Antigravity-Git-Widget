interface NoticeFooterProps {
  notice: string;
  error: string;
  onClearNotice: () => void;
}

export function NoticeFooter({ notice, error, onClearNotice }: NoticeFooterProps) {
  return (
    <>
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
