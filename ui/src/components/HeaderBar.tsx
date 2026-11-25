import { Picker } from '../Picker';
import type { PickerOption } from '../Picker';
import { normalizeVersion } from '../hooks/useAppUpdater';
import type { RepoStatus } from '../types';

interface HeaderBarProps {
  selectedRepo: string;
  repoOptions: PickerOption[];
  status: RepoStatus | null;
  selectedRemote: string;
  autoMatched: boolean;
  tab: 'flow' | 'diff';
  diffCount: number;
  busy: boolean;
  updateVersion?: string | null;
  updateInProgress?: boolean;
  cloneInProgress?: boolean;
  onOpenUpdateModal?: () => void;
  onToggleCloneModal?: () => void;
  onSelectRepo: (val: string) => void;
  onRefresh: () => void;
  onToggleSettings: () => void;
  onSelectRemote: (val: string) => void;
  onToggleRemoteForm: () => void;
  onSwitchTab: (tab: 'flow' | 'diff') => void;
}

export function HeaderBar({
  selectedRepo,
  repoOptions,
  status,
  selectedRemote,
  autoMatched,
  tab,
  diffCount,
  busy,
  updateVersion,
  updateInProgress = false,
  cloneInProgress = false,
  onOpenUpdateModal,
  onToggleCloneModal,
  onSelectRepo,
  onRefresh,
  onToggleSettings,
  onSelectRemote,
  onToggleRemoteForm,
  onSwitchTab,
}: HeaderBarProps) {
  const badgeVersion = normalizeVersion(updateVersion);

  return (
    <>
      <div className="gw-row">
        <Picker
          value={selectedRepo}
          options={repoOptions}
          searchable
          placeholder="Select repository..."
          onChange={onSelectRepo}
        />
        {(badgeVersion || updateInProgress) && onOpenUpdateModal && (
          <button
            type="button"
            className="gw-badge gw-badge-ok"
            style={{ cursor: 'pointer', whiteSpace: 'nowrap', flexShrink: 0 }}
            onClick={onOpenUpdateModal}
            title={badgeVersion ? `Update to v${badgeVersion}` : 'Updating widget'}
          >
            {updateInProgress ? 'Updating...' : `Update v${badgeVersion}`}
          </button>
        )}
        {onToggleCloneModal && (
          <button
            type="button"
            className={`gw-btn${cloneInProgress ? ' gw-btn-primary' : ''}`}
            onClick={onToggleCloneModal}
            title="Clone repository"
          >
            {cloneInProgress ? 'Cloning...' : 'Clone'}
          </button>
        )}
        <button className="gw-btn" onClick={onRefresh} disabled={busy}>
          ↻
        </button>
        <button className="gw-btn" onClick={onToggleSettings}>
          ⚙
        </button>
      </div>

      {status && (
        <div className="gw-row">
          <div style={{ flex: 1, minWidth: 0 }}>
            <Picker
              value={selectedRemote}
              placeholder="No remote configured..."
              options={status.remotes.map((r) => ({ value: r, label: `remote: ${r}` }))}
              disabled={busy || status.remotes.length === 0}
              onChange={onSelectRemote}
            />
          </div>
          <button
            type="button"
            className="gw-btn"
            title="Add remote"
            onClick={onToggleRemoteForm}
          >
            +
          </button>
        </div>
      )}

      {status &&
        (status.ahead > 0 || status.behind > 0 || status.is_dirty || status.is_merging || autoMatched) && (
          <div className="gw-between">
            <div className="gw-row" style={{ flexWrap: 'wrap' }}>
              {status.ahead > 0 && <span className="gw-badge gw-badge-ok">↑{status.ahead}</span>}
              {status.behind > 0 && <span className="gw-badge gw-badge-warn">↓{status.behind}</span>}
              {status.is_dirty && (
                <span className="gw-badge gw-badge-warn">dirty ({status.changes.length})</span>
              )}
              {status.is_merging && <span className="gw-badge gw-badge-danger">CONFLICT</span>}
            </div>
            {autoMatched && <span className="gw-badge">Auto</span>}
          </div>
        )}

      <div className="gw-tabs">
        <button
          type="button"
          className={`gw-tab ${tab === 'flow' ? 'active' : ''}`}
          onClick={() => onSwitchTab('flow')}
        >
          Git Flow
        </button>
        <button
          type="button"
          className={`gw-tab ${tab === 'diff' ? 'active' : ''}`}
          onClick={() => onSwitchTab('diff')}
        >
          Diff Review {diffCount > 0 ? `(${diffCount})` : ''}
        </button>
      </div>
    </>
  );
}
