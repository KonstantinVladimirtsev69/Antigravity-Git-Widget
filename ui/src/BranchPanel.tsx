import { useMemo } from 'react';
import { BranchCreateDelete } from './components/BranchCreateDelete';
import { IntegrateSplitButton, getModeLabel } from './IntegrateSplitButton';
import { Picker } from './Picker';
import type { DualBranchSync, IntegrationMode, RepoStatus } from './types';
import { filterRemoteBranchesForRemote, isDiverged } from './utils/remoteFocus';

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
  const hasRemote = status.remotes.length > 0;
  const isPublished = status.upstream != null;
  const diverged = isDiverged(status);

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
    const locals = status.local_branches.map((b) => ({
      value: `local:${b}`,
      label: b,
      sub: formatTracking(b),
    }));
    const remotes = filterRemoteBranchesForRemote(
      status.remote_branches,
      status.local_branches,
      selectedRemote,
      status.remotes,
    ).map((rb) => ({ value: `remote:${rb}`, label: rb, sub: 'remote' }));
    return [...locals, ...remotes];
  }, [status.local_branches, status.remote_branches, status.branch_tracking, status.remotes, selectedRemote]);

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
      <BranchCreateDelete
        otherLocals={otherLocals}
        busy={busy}
        onCreateBranch={onCreateBranch}
        onDeleteBranch={onDeleteBranch}
      >
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
      </BranchCreateDelete>

      <div className="gw-row" style={{ flexWrap: 'wrap' }}>
        <button className="gw-btn" style={{ flex: '1 1 70px' }} disabled={busy || !hasRemote} onClick={onFetch}>
          Fetch
        </button>
        <button
          className={`gw-btn ${status.behind > 0 ? 'gw-btn-primary' : ''}`}
          style={{ flex: '1 1 70px' }}
          disabled={busy || !hasRemote}
          onClick={onPull}
        >
          {status.behind > 0 ? `Pull ↓${status.behind}` : 'Pull'}
        </button>
        <button
          className={`gw-btn ${!isPublished || status.ahead > 0 ? 'gw-btn-primary' : ''}`}
          style={{ flex: '1 1 90px' }}
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
        {diverged && (
          <span
            className="gw-badge gw-badge-warn"
            title="Branch has diverged from selected remote; Pull to merge or review conflicts"
          >
            Diverged ↑{status.ahead} ↓{status.behind}
          </span>
        )}
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
