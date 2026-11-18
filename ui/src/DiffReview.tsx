import { useState } from 'react';
import { CONVERSATION_ID, api } from './api';
import type { DiffFile, DiffHunk } from './types';

interface DiffReviewProps {
  repo: string;
  files: DiffFile[];
  busy: boolean;
  onRefresh: () => Promise<void>;
  onNotify: (msg: string) => void;
  onError: (err: string) => void;
}

export function DiffReview({
  repo,
  files,
  busy,
  onRefresh,
  onNotify,
  onError,
}: DiffReviewProps) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [askTarget, setAskTarget] = useState<{ file: string; snippet: string } | null>(null);
  const [question, setQuestion] = useState('');
  const [sending, setSending] = useState(false);

  const toggle = (key: string) =>
    setExpanded((prev) => ({ ...prev, [key]: !(prev[key] ?? true) }));

  const runAction = async (fn: () => Promise<unknown>, okMsg: string) => {
    try {
      onError('');
      await fn();
      onNotify(okMsg);
      await onRefresh();
    } catch (e) {
      onError(e instanceof Error ? e.message : String(e));
      await onRefresh();
    }
  };

  const openAskForHunk = (file: string, hunk: DiffHunk) => {
    const sel = window.getSelection()?.toString().trim();
    const snippet = sel || [hunk.header, ...hunk.lines].join('\n');
    setAskTarget({ file, snippet });
    setQuestion('');
  };

  const sendAsk = async () => {
    if (!askTarget || !question.trim()) return;
    setSending(true);
    try {
      const res = await api.askAi(
        repo,
        askTarget.file,
        askTarget.snippet,
        question.trim(),
        CONVERSATION_ID,
      );
      if (res.sent) {
        onNotify('Фрагмент и вопрос отправлены в чат ИИ');
      } else {
        await navigator.clipboard.writeText(res.prompt);
        onNotify('Скопировано в буфер обмена (чат недоступен напрямую)');
      }
      setAskTarget(null);
      setQuestion('');
    } catch (e) {
      onError(e instanceof Error ? e.message : String(e));
    } finally {
      setSending(false);
    }
  };

  if (files.length === 0) {
    return (
      <div className="gw-card gw-muted" style={{ textAlign: 'center', padding: 16 }}>
        Рабочее дерево чистое — нет изменений для ревью.
      </div>
    );
  }

  return (
    <div className="gw-stack">
      {askTarget && (
        <div className="gw-card gw-stack" style={{ borderColor: 'var(--gw-accent)' }}>
          <div className="gw-between">
            <strong style={{ fontSize: 11.5 }}>Вопрос ИИ по {askTarget.file}</strong>
            <button className="gw-btn gw-btn-xs" onClick={() => setAskTarget(null)}>
              ✕
            </button>
          </div>
          <pre
            className="gw-diff-box"
            style={{ margin: 0, padding: 6, maxHeight: 90, overflow: 'auto' }}
          >
            {askTarget.snippet}
          </pre>
          <textarea
            className="gw-textarea"
            rows={2}
            placeholder="Комментарий или вопрос по этому куску..."
            value={question}
            onChange={(e) => setQuestion(e.target.value)}
          />
          <button
            className="gw-btn gw-btn-primary"
            disabled={sending || !question.trim()}
            onClick={() => void sendAsk()}
          >
            {sending ? 'Отправка...' : 'Отправить в чат ИИ'}
          </button>
        </div>
      )}

      {files.map((df) => {
        const key = `${df.staged ? 'S' : 'U'}:${df.path}`;
        const isOpen = expanded[key] ?? true;

        return (
          <div key={key} className="gw-card gw-stack" style={{ padding: 7 }}>
            <div className="gw-between">
              <button
                type="button"
                className="gw-btn gw-btn-xs"
                style={{
                  border: 'none',
                  background: 'transparent',
                  padding: 0,
                  textAlign: 'left',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                }}
                onClick={() => toggle(key)}
              >
                {isOpen ? '▾' : '▸'} <strong>{df.path}</strong>
              </button>
              <div className="gw-row">
                <span className="gw-badge gw-badge-ok">+{df.additions}</span>
                {df.deletions > 0 && <span className="gw-badge gw-badge-danger">-{df.deletions}</span>}
                <span className={`gw-badge ${df.staged ? 'gw-badge-ok' : 'gw-badge-warn'}`}>
                  {df.staged ? 'Accepted' : df.status}
                </span>
              </div>
            </div>

            <div className="gw-row">
              {df.staged ? (
                <button
                  className="gw-btn gw-btn-xs"
                  disabled={busy}
                  onClick={() => void runAction(() => api.unstageFile(repo, df.path), 'Возвращено в Pending')}
                >
                  Unstage
                </button>
              ) : (
                <>
                  <button
                    className="gw-btn gw-btn-xs gw-btn-ok"
                    disabled={busy}
                    onClick={() => void runAction(() => api.stageFile(repo, df.path), 'Файл принят (staged)')}
                  >
                    Accept file
                  </button>
                  <button
                    className="gw-btn gw-btn-xs gw-btn-danger"
                    disabled={busy}
                    onClick={() => void runAction(() => api.discardFile(repo, df.path), 'Изменения файла откачены')}
                  >
                    Reject file
                  </button>
                </>
              )}
            </div>

            {isOpen &&
              df.hunks.map((hunk) => (
                <div key={hunk.index} className="gw-diff-box">
                  <div className="gw-diff-hunk gw-between">
                    <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                      {hunk.header}
                    </span>
                    <div className="gw-row">
                      <button
                        className="gw-btn gw-btn-xs"
                        title="Выделите строки выше или нажмите для отправки всего hunk в чат"
                        onClick={() => openAskForHunk(df.path, hunk)}
                      >
                        💬 ИИ
                      </button>
                      {!df.staged && (
                        <>
                          <button
                            className="gw-btn gw-btn-xs gw-btn-ok"
                            disabled={busy}
                            onClick={() =>
                              void runAction(
                                () => api.acceptHunk(repo, df.path, hunk.index),
                                'Hunk принят',
                              )
                            }
                          >
                            ✓
                          </button>
                          <button
                            className="gw-btn gw-btn-xs gw-btn-danger"
                            disabled={busy}
                            onClick={() =>
                              void runAction(
                                () => api.rejectHunk(repo, df.path, hunk.index),
                                'Hunk откачен',
                              )
                            }
                          >
                            ✕
                          </button>
                        </>
                      )}
                    </div>
                  </div>
                  {hunk.lines.map((line, idx) => {
                    const cls = line.startsWith('+')
                      ? 'gw-diff-line gw-diff-add'
                      : line.startsWith('-')
                        ? 'gw-diff-line gw-diff-del'
                        : 'gw-diff-line';
                    return (
                      <div key={idx} className={cls}>
                        {line || ' '}
                      </div>
                    );
                  })}
                </div>
              ))}
          </div>
        );
      })}
    </div>
  );
}
