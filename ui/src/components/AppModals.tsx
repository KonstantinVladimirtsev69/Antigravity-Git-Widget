import type { useAppUpdater } from '../hooks/useAppUpdater';
import type { useClone } from '../hooks/useClone';
import { RemoteModal, SettingsPanel } from '../Modals';
import type { RepoInfo, ScannerConfig } from '../types';
import { CloneModal } from './CloneModal';
import { UpdateModal } from './UpdateModal';

interface AppModalsProps {
  updater: ReturnType<typeof useAppUpdater>;
  clone: ReturnType<typeof useClone>;
  showSettings: boolean;
  showRemoteForm: boolean;
  selectedRepo: string;
  repos: RepoInfo[];
  config: ScannerConfig;
  busy: boolean;
  onCloseSettings: () => void;
  onOpenUpdateFromSettings: () => void;
  onCloseRemote: () => void;
  onError: (msg: string) => void;
  onInitRepo: (path: string) => Promise<boolean>;
  onSaveConfig: (cfg: ScannerConfig) => Promise<void>;
  onAuthChanged: () => Promise<void>;
  onAddExistingRemote: (name: string, url: string) => Promise<void>;
  onCreateGithubRemote: (payload: {
    name: string;
    description: string;
    private: boolean;
    use_ssh: boolean;
    token?: string;
  }) => Promise<void>;
}

export function AppModals({
  updater,
  clone,
  showSettings,
  showRemoteForm,
  selectedRepo,
  repos,
  config,
  busy,
  onCloseSettings,
  onOpenUpdateFromSettings,
  onCloseRemote,
  onError,
  onInitRepo,
  onSaveConfig,
  onAuthChanged,
  onAddExistingRemote,
  onCreateGithubRemote,
}: AppModalsProps) {
  const defaultCloneDir = config.last_clone_dir || config.roots[0] || 'C:\\Projects';

  return (
    <>
      {updater.modalOpen && updater.checkInfo && (
        <UpdateModal
          info={updater.checkInfo}
          status={updater.status}
          installing={updater.isInstalling}
          restartTimedOut={updater.restartTimedOut}
          reloadSuggested={updater.reloadSuggested}
          onInstall={() => void updater.startInstall()}
          onClose={updater.closeModal}
        />
      )}

      {clone.modalOpen && (
        <CloneModal
          status={clone.status}
          starting={clone.starting}
          cancelling={clone.cancelling}
          formError={clone.formError}
          defaultParentDir={defaultCloneDir}
          onStartClone={clone.startClone}
          onCancelClone={() => void clone.cancelClone()}
          onReset={clone.reset}
          onClose={clone.closeModal}
          onClearError={() => clone.setFormError('')}
        />
      )}

      {showSettings && (
        <SettingsPanel
          config={config}
          busy={busy}
          currentVersion={updater.currentVersion}
          updateInfo={updater.checkInfo}
          checkingUpdates={updater.checking}
          updateCheckError={updater.manualError}
          onCheckUpdates={() => void updater.checkForUpdates()}
          onOpenUpdateModal={onOpenUpdateFromSettings}
          onClose={onCloseSettings}
          onInitRepo={onInitRepo}
          onSave={(cfg) => void onSaveConfig(cfg)}
        />
      )}

      {showRemoteForm && selectedRepo && (
        <RemoteModal
          defaultRepoName={
            repos.find((r) => r.path.toLowerCase() === selectedRepo.toLowerCase())?.name ??
            selectedRepo.split(/[\\/]/).pop() ??
            'new-repo'
          }
          hasSavedToken={Boolean(config.has_github_token)}
          busy={busy}
          onClose={onCloseRemote}
          onAuthChanged={onAuthChanged}
          onError={onError}
          onAddExisting={onAddExistingRemote}
          onCreateGithub={onCreateGithubRemote}
        />
      )}
    </>
  );
}
