import { useEffect, useState } from 'react';
import { CONVERSATION_ID, api } from './api';
import { DiffAskAiBox } from './components/DiffAskAiBox';
import { DiffModeBar } from './components/DiffModeBar';
import type { CommitInfo, DiffFile, DiffHunk } from './types';
import { deliverPromptToHost } from './utils/hostBridge';

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

  useEffect(() => {
    if (!focusFile) return;
    setExpanded((prev) => ({
      ...prev,
      [`S:${focusFile}`]: true,
      [`U:${focusFile}`]: true,
    }));
  }, [focusFile]);

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
      const msg = await deliverPromptToHost(
        res.prompt,
        res.sent,
        'Diff snippet and question sent to AI chat',
      );
      onNotify(msg);
      setAskTarget(null);
      setQuestion('');
    } catch (e) {
      onError(e instanceof Error ? e.message : String(e));
    } finally {
      setSending(false);
    }
  };

  const modeBar = (
    <DiffModeBar
      repo={repo}
      mode={mode}
      commits={commits}
      selectedCommit={selectedCommit}
      busy={busy}
      onModeChange={onModeChange}
      onCommitSelect={onCommitSelect}
      onNotify={onNotify}
      onCherryPick={(fn, okMsg) => void runAction(fn, okMsg)}
    />
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
        <DiffAskAiBox
          file={askTarget.file}
          snippet={askTarget.snippet}
          question={question}
          sending={sending}
          onQuestionChange={setQuestion}
          onSend={() => void sendAsk()}
          onClose={() => setAskTarget(null)}
        />
      )}

      {files.map((df) => {
        const key = `${df.staged ? 'S' : 'U'}:${df.path}`;
        const isFocused = focusFile === df.path;
        const isOpen = expanded[key] ?? true;

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
