interface DiffAskAiBoxProps {
  file: string;
  snippet: string;
  question: string;
  sending: boolean;
  onQuestionChange: (v: string) => void;
  onSend: () => void;
  onClose: () => void;
}

export function DiffAskAiBox({
  file,
  snippet,
  question,
  sending,
  onQuestionChange,
  onSend,
  onClose,
}: DiffAskAiBoxProps) {
  return (
    <div className="gw-card gw-stack" style={{ borderColor: 'rgba(59, 130, 246, 0.45)' }}>
      <div className="gw-between">
        <strong style={{ fontSize: 11.5 }}>Ask AI about {file}</strong>
        <button className="gw-btn gw-btn-xs" onClick={onClose}>
          ✕
        </button>
      </div>
      <pre className="gw-diff-box" style={{ margin: 0, padding: 6, maxHeight: 90, overflow: 'auto' }}>
        {snippet}
      </pre>
      <textarea
        className="gw-textarea"
        rows={2}
        placeholder="Comment or question about this snippet..."
        value={question}
        onChange={(e) => onQuestionChange(e.target.value)}
      />
      <button
        className="gw-btn gw-btn-primary"
        disabled={sending || !question.trim()}
        onClick={onSend}
      >
        {sending ? 'Sending...' : 'Send to AI Chat'}
      </button>
    </div>
  );
}
