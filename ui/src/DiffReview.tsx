import { useState } from 'react';
import { CONVERSATION_ID, api } from './api';
import { Picker } from './Picker';
import type { CommitInfo, DiffFile, DiffHunk } from './types';

interface DiffReviewProps {
  repo: string;
  files: DiffFile[];
  commits: CommitInfo[];
  selectedCommit: string;
  mode: 'working' | 'branch' | 'commit';
  focusFile?: string | null;
  busy: boolean;
  onModeChange: (mode: 'working' | 'branch' | 'commit') => void;
  onCommitSelect: (hash: string) => void;
  onRefresh: () => Promise<void>;
  onNotify: (msg: string) => void;
  onError: (err: string) => void;
}

export function DiffReview({
  repo,
  files,
  commits,
  selectedCommit,
  mode,
  focusFile,
  busy,
  onModeChange,
  onCommitSelect,
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
        onNotify('Diff snippet and question sent to AI chat');
      } else {
        await navigator.clipboard.writeText(res.prompt);
        onNotify('Copied to clipboard (direct chat send unavailable)');
      }
      setAskTarget(null);
      setQuestion('');
    } catch (e) {
      onError(e instanceof Error ? e.message : String(e));
    } finally {
      setSending(false);
    }
  };

  const modeBar = (
    <div className="gw-stack">
      <div className="gw-tabs">
        <button
          type="button"
          className={`gw-tab ${mode === 'working' ? 'active' : ''}`}
          onClick={() => onModeChange('working')}
        >
          Working
        </button>
        <button
          type="button"
          className={`gw-tab ${mode === 'branch' ? 'active' : ''}`}
          onClick={() => onModeChange('branch')}
        >
          Branch
        </button>
        <button
          type="button"
          className={`gw-tab ${mode === 'commit' ? 'active' : ''}`}
          onClick={() => onModeChange('commit')}
        >
          Commits
        </button>
      </div>
      {mode === 'commit' && (
        <div className="gw-stack" style={{ gap: 5 }}>
          <Picker
            value={selectedCommit}
            searchable
            placeholder="Select commit..."
            options={commits.map((c) => ({
              value: c.hash,
              label: `${c.short_hash} · ${c.subject}`,
              sub: `${c.author} · ${c.date_rel}`,
            }))}
            onChange={onCommitSelect}
          />
          {selectedCommit && (
            <div className="gw-row">
              <button
                type="button"
                className="gw-btn gw-btn-xs"
                onClick={() => {
                  void navigator.clipboard.writeText(selectedCommit);
                  onNotify(`Copied ${selectedCommit.slice(0, 8)}`);
                }}
              >
                Copy Hash
              </button>
              <button
                type="button"
                className="gw-btn gw-btn-xs"
                disabled={busy}
                onClick={() =>
                  void runAction(
                    () => api.cherryPick(repo, selectedCommit),
                    `Cherry-picked ${selectedCommit.slice(0, 8)}`,
                  )
                }
              >
                Cherry-Pick
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );

  if (files.length === 0) {
    return (
      <div className="gw-stack">
        {modeBar}
        <div className="gw-card gw-muted" style={{ textAlign: 'center', padding: 14 }}>
          {mode === 'working'
            ? 'Working tree is clean. Check "Branch" or "Commits".'
            : mode === 'branch'
              ? 'No diff against base branch.'
              : 'Select a commit to inspect its diff.'}
        </div>
      </div>
    );
  }

  return (
    <div className="gw-stack">
      {modeBar}
      {askTarget && (
        <div className="gw-card gw-stack" style={{ borderColor: 'rgba(59, 130, 246, 0.45)' }}>
          <div className="gw-between">
            <strong style={{ fontSize: 11.5 }}>Ask AI about {askTarget.file}</strong>
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
            placeholder="Comment or question about this snippet..."
            value={question}
            onChange={(e) => setQuestion(e.target.value)}
          />
          <button
            className="gw-btn gw-btn-primary"
            disabled={sending || !question.trim()}
            onClick={() => void sendAsk()}
          >
            {sending ? 'Sending...' : 'Send to AI Chat'}
          </button>
        </div>
      )}

      {files.map((df) => {
        const key = `${df.staged ? 'S' : 'U'}:${df.path}`;
        const isFocused = focusFile === df.path;
        const isOpen = isFocused ? true : (expanded[key] ?? true);

        return (
          <div
            key={key}
            className="gw-card gw-stack"
            style={{
              padding: 7,
              borderColor: isFocused ? 'rgba(59, 130, 246, 0.5)' : undefined,
            }}
          >
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
                  {df.staged ? 'Staged' : df.status}
                </span>
              </div>
            </div>

            {mode === 'working' && (
              <div className="gw-row">
                {df.staged ? (
                  <button
                    className="gw-btn gw-btn-xs"
                    disabled={busy}
                    onClick={() => void runAction(() => api.unstageFile(repo, df.path), 'Unstaged')}
                  >
                    Unstage
                  </button>
                ) : (
                  <>
                    <button
                      className="gw-btn gw-btn-xs gw-btn-ok"
                      disabled={busy}
                      onClick={() => void runAction(() => api.stageFile(repo, df.path), 'Accepted (staged)')}
                    >
                      Accept file
                    </button>
                    <button
                      className="gw-btn gw-btn-xs gw-btn-danger"
                      disabled={busy}
                      onClick={() => void runAction(() => api.discardFile(repo, df.path), 'Reverted file')}
                    >
                      Reject file
                    </button>
                  </>
                )}
              </div>
            )}

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
                        title="Select lines or click to ask AI about this hunk"
                        onClick={() => openAskForHunk(df.path, hunk)}
                      >
                        💬 AI
                      </button>
                      {mode === 'working' && !df.staged && (
                        <>
                          <button
                            className="gw-btn gw-btn-xs gw-btn-ok"
                            disabled={busy}
                            onClick={() =>
                              void runAction(
                                () => api.acceptHunk(repo, df.path, hunk.index),
                                'Hunk accepted',
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
                                'Hunk rejected',
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
