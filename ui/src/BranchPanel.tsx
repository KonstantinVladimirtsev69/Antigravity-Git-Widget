import { useMemo, useState } from 'react';
import { Picker } from './Picker';
import type { RepoStatus } from './types';

interface BranchPanelProps {
  status: RepoStatus;
  selectedRemote: string;
  mergeTarget: string;
  mergeDir: 'into_current' | 'current_into_target';
  busy: boolean;
  onCheckoutLocal: (branch: string) => void;
  onCheckoutRemote: (remoteBranch: string) => void;
  onCreateBranch: (name: string) => Promise<void>;
  onDeleteBranch: (name: string, force: boolean) => Promise<void>;
  onFetch: () => void;
  onPull: () => void;
  onPush: () => void;
  onMergeDirToggle: () => void;
  onMergeTargetChange: (branch: string) => void;
  onMerge: () => void;
}

export function BranchPanel({
  status,
  selectedRemote,
  mergeTarget,
  mergeDir,
  busy,
  onCheckoutLocal,
  onCheckoutRemote,
  onCreateBranch,
  onDeleteBranch,
  onFetch,
  onPull,
  onPush,
  onMergeDirToggle,
  onMergeTargetChange,
  onMerge,
}: BranchPanelProps) {
  const [branchMode, setBranchMode] = useState<'none' | 'create' | 'delete'>('none');
  const [newBranch, setNewBranch] = useState('');
  const [delBranch, setDelBranch] = useState('');
  const [forceDel, setForceDel] = useState(false);

  const hasRemote = status.remotes.length > 0;
  const isPublished = Boolean(status.upstream);

  const branchOptions = useMemo(() => {
    const localSet = new Set(status.local_branches);
    const locals = status.local_branches.map((b) => ({ value: `local:${b}`, label: b }));
    const remotes = status.remote_branches
      .filter((rb) => {
        const shortName = rb.includes('/') ? rb.slice(rb.indexOf('/') + 1) : rb;
        return !localSet.has(shortName);
      })
      .map((rb) => ({ value: `remote:${rb}`, label: rb, sub: 'remote' }));
    return [...locals, ...remotes];
  }, [status.local_branches, status.remote_branches]);

  const otherLocals = useMemo(
    () => status.local_branches.filter((b) => b !== status.branch),
    [status.local_branches, status.branch],
  );

  return (
    <div className="gw-card gw-stack">
      <div className="gw-row">
        <div style={{ flex: 1, minWidth: 0 }}>
          <Picker
            value={`local:${status.branch}`}
            searchable
            options={branchOptions}
            disabled={busy}
            onChange={(val) => {
              if (val.startsWith('local:')) {
                const name = val.slice(6);
                if (name !== status.branch) onCheckoutLocal(name);
              } else if (val.startsWith('remote:')) {
                onCheckoutRemote(val.slice(7));
              }
            }}
          />
        </div>
        <button
          type="button"
          className="gw-btn"
          title="New branch"
          disabled={busy}
          onClick={() => setBranchMode((m) => (m === 'create' ? 'none' : 'create'))}
        >
          +
        </button>
        <button
          type="button"
          className="gw-btn"
          title="Delete branch"
          disabled={busy || otherLocals.length === 0}
          onClick={() => {
            setDelBranch((prev) => (prev && otherLocals.includes(prev) ? prev : otherLocals[0] ?? ''));
            setForceDel(false);
            setBranchMode((m) => (m === 'delete' ? 'none' : 'delete'));
          }}
        >
          ⌫
        </button>
      </div>

      {branchMode === 'create' && (
        <div className="gw-row">
          <input
            className="gw-input"
            placeholder="feat/new-branch"
            value={newBranch}
            onChange={(e) => setNewBranch(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && newBranch.trim()) {
                const name = newBranch.trim();
                setNewBranch('');
                setBranchMode('none');
                void onCreateBranch(name);
              }
            }}
          />
          <button
            type="button"
            className="gw-btn gw-btn-primary"
            disabled={busy || !newBranch.trim()}
            onClick={() => {
              const name = newBranch.trim();
              setNewBranch('');
              setBranchMode('none');
              void onCreateBranch(name);
            }}
          >
            Create
          </button>
        </div>
      )}

      {branchMode === 'delete' && (
        <div className="gw-stack" style={{ gap: 5 }}>
          <div className="gw-row">
            <div style={{ flex: 1, minWidth: 0 }}>
              <Picker
                value={delBranch}
                placeholder="Branch to delete..."
                options={otherLocals.map((b) => ({ value: b, label: b }))}
                onChange={setDelBranch}
              />
            </div>
            <button
              type="button"
              className="gw-btn gw-btn-danger"
              disabled={busy || !delBranch}
              onClick={() => {
                const name = delBranch;
                setBranchMode('none');
                void onDeleteBranch(name, forceDel);
              }}
            >
              Delete
            </button>
          </div>
          <label className="gw-row gw-muted" style={{ cursor: 'pointer' }}>
            <input
              type="checkbox"
              checked={forceDel}
              onChange={(e) => setForceDel(e.target.checked)}
            />
            Force delete (-D)
          </label>
        </div>
      )}

      <div className="gw-row">
        <button className="gw-btn" style={{ flex: 1 }} disabled={busy || !hasRemote} onClick={onFetch}>
          Fetch
        </button>
        <button
          className={`gw-btn ${status.behind > 0 ? 'gw-btn-primary' : ''}`}
          style={{ flex: 1 }}
          disabled={busy || !hasRemote}
          onClick={onPull}
        >
          {status.behind > 0 ? `Pull ↓${status.behind}` : 'Pull'}
        </button>
        <button
          className={`gw-btn ${!isPublished || status.ahead > 0 ? 'gw-btn-primary' : ''}`}
          style={{ flex: 1 }}
          disabled={busy || (hasRemote && isPublished && status.ahead === 0 && !selectedRemote)}
          onClick={onPush}
        >
          {!hasRemote
            ? 'Set Remote'
            : !isPublished
              ? 'Publish Branch'
              : status.ahead > 0
                ? `Push ↑${status.ahead}`
                : 'Push'}
        </button>
      </div>

      <div className="gw-row">
        <button
          type="button"
          className="gw-btn"
          title={
            mergeDir === 'current_into_target'
              ? `Merge ${status.branch} into selected branch`
              : `Merge selected branch into ${status.branch}`
          }
          onClick={onMergeDirToggle}
        >
          {mergeDir === 'current_into_target' ? '→' : '←'}
        </button>
        <div style={{ flex: 1, minWidth: 0 }}>
          <Picker
            value={mergeTarget}
            placeholder="Target branch..."
            options={otherLocals.map((b) => ({ value: b, label: b }))}
            disabled={busy}
            onChange={onMergeTargetChange}
          />
        </div>
        <button className="gw-btn" disabled={busy || !mergeTarget} onClick={onMerge}>
          Merge
        </button>
      </div>
    </div>
  );
}
