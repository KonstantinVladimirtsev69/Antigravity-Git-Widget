import { api } from '../api';
import { Picker } from '../Picker';
import type { CommitInfo } from '../types';

interface DiffModeBarProps {
  repo: string;
  mode: 'working' | 'branch' | 'commit';
  commits: CommitInfo[];
  selectedCommit: string;
  busy: boolean;
  onModeChange: (mode: 'working' | 'branch' | 'commit') => void;
  onCommitSelect: (hash: string) => void;
  onNotify: (msg: string) => void;
  onCherryPick: (fn: () => Promise<unknown>, okMsg: string) => void;
}

export function DiffModeBar({
  repo,
  mode,
  commits,
  selectedCommit,
  busy,
  onModeChange,
  onCommitSelect,
  onNotify,
  onCherryPick,
}: DiffModeBarProps) {
  return (
    <div className="gw-stack">
      <div className="gw-tabs">
        {(['working', 'branch', 'commit'] as const).map((m) => (
          <button
            key={m}
            type="button"
            className={`gw-tab ${mode === m ? 'active' : ''}`}
            onClick={() => onModeChange(m)}
          >
            {m === 'working' ? 'Working' : m === 'branch' ? 'Branch' : 'Commits'}
          </button>
        ))}
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
                  onCherryPick(
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
}
