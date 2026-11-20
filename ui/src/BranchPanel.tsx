import { useMemo, useState } from 'react';
import { IntegrateSplitButton, getModeLabel } from './IntegrateSplitButton';
import { Picker } from './Picker';
import type { DualBranchSync, IntegrationMode, RepoStatus } from './types';

interface BranchPanelProps {
  status: RepoStatus;
  selectedRemote: string;
  mergeTarget: string;
  mergeDir: 'into_current' | 'current_into_target';
  integrationMode: IntegrationMode;
  dualSync: DualBranchSync | null;
  scanningSync: boolean;
  busy: boolean;
  onCheckoutLocal: (branch: string) => void;
  onCheckoutRemote: (remoteBranch: string) => void;
  onCreateBranch: (name: string) => Promise<void>;
  onDeleteBranch: (name: string, force: boolean) => Promise<void>;
  onFetch: () => void;
  onPull: () => void;
  onPullBoth: () => void;
  onPush: () => void;
  onMergeDirToggle: () => void;
  onMergeTargetChange: (branch: string) => void;
  onIntegrationModeChange: (mode: IntegrationMode) => void;
  onIntegrate: () => void;
}

export function BranchPanel({
  status,
  selectedRemote,
  mergeTarget,
  mergeDir,
  integrationMode,
  dualSync,
  scanningSync,
  busy,
  onCheckoutLocal,
  onCheckoutRemote,
  onCreateBranch,
  onDeleteBranch,
  onFetch,
  onPull,
  onPullBoth,
  onPush,
  onMergeDirToggle,
  onMergeTargetChange,
  onIntegrationModeChange,
  onIntegrate,
}: BranchPanelProps) {
  const [branchMode, setBranchMode] = useState<'none' | 'create' | 'delete'>('none');
  const [newBranch, setNewBranch] = useState('');
  const [delBranch, setDelBranch] = useState('');
  const [forceDel, setForceDel] = useState(false);

  const hasRemote = status.remotes.length > 0;
  const isPublished = Boolean(status.upstream);

  const formatTracking = (branch: string) => {
    const tr = status.branch_tracking?.[branch];
    if (!tr) return undefined;
    const parts: string[] = [];
    if (tr.behind > 0) parts.push(`↓${tr.behind}`);
    if (tr.ahead > 0) parts.push(`↑${tr.ahead}`);
    if (tr.gone) parts.push('gone');
    return parts.length > 0 ? parts.join(' ') : undefined;
  };

  const branchOptions = useMemo(() => {
    const localSet = new Set(status.local_branches);
    const locals = status.local_branches.map((b) => ({
      value: `local:${b}`,
      label: b,
      sub: formatTracking(b),
    }));
    const remotes = status.remote_branches
      .filter((rb) => {
        const shortName = rb.includes('/') ? rb.slice(rb.indexOf('/') + 1) : rb;
        return !localSet.has(shortName);
      })
      .map((rb) => ({ value: `remote:${rb}`, label: rb, sub: 'remote' }));
    return [...locals, ...remotes];
  }, [status.local_branches, status.remote_branches, status.branch_tracking]);

  const otherLocals = useMemo(
    () => status.local_branches.filter((b) => b !== status.branch),
    [status.local_branches, status.branch],
  );

  const currentBehind = Math.max(
    status.behind,
    status.branch_tracking?.[status.branch]?.behind ?? 0,
  );
  const targetBehind = mergeTarget
    ? (dualSync &&
      (dualSync.source.branch === mergeTarget || dualSync.target.branch === mergeTarget)
        ? dualSync.source.branch === mergeTarget
          ? dualSync.source.behind
          : dualSync.target.behind
        : status.branch_tracking?.[mergeTarget]?.behind ?? 0)
    : 0;

  const unpulledItems: string[] = [];
  if (currentBehind > 0) unpulledItems.push(`${status.branch} ↓${currentBehind}`);
  if (mergeTarget && targetBehind > 0) unpulledItems.push(`${mergeTarget} ↓${targetBehind}`);
  const modeLabel = getModeLabel(integrationMode);

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
              ? `${modeLabel}: ${status.branch} → ${mergeTarget || 'target'}`
              : `${modeLabel}: ${mergeTarget || 'source'} → ${status.branch}`
          }
          onClick={onMergeDirToggle}
        >
          {mergeDir === 'current_into_target' ? '→' : '←'}
        </button>
        <div style={{ flex: 1, minWidth: 0 }}>
          <Picker
            value={mergeTarget}
            placeholder="Target branch..."
            options={otherLocals.map((b) => ({
              value: b,
              label: b,
              sub: formatTracking(b),
            }))}
            disabled={busy}
            onChange={onMergeTargetChange}
          />
        </div>
        <IntegrateSplitButton
          mode={integrationMode}
          disabled={busy || !mergeTarget}
          hasUnpulled={unpulledItems.length > 0}
          onModeChange={onIntegrationModeChange}
          onExecute={onIntegrate}
        />
      </div>

      {(scanningSync || unpulledItems.length > 0) && mergeTarget && (
        <div className="gw-sync-banner">
          <span className="gw-muted" style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {scanningSync
              ? `Scanning ${status.branch} & ${mergeTarget}...`
              : `Unpulled: ${unpulledItems.join(' · ')}`}
          </span>
          {unpulledItems.length > 0 && (
            <button
              type="button"
              className="gw-btn gw-btn-xs gw-btn-primary"
              disabled={busy}
              onClick={onPullBoth}
            >
              {unpulledItems.length > 1 ? 'Pull both' : 'Pull'}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
