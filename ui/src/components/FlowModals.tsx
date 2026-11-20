import { getModeLabel } from '../IntegrateSplitButton';
import { DirtyModal, PreviewModal, UnpulledModal } from '../Modals';
import type { ActionPreview, DirtyStrategy, DualBranchSync, GuardedOpKind, IntegrationMode } from '../types';

interface FlowModalsProps {
  integrationMode: IntegrationMode;
  busy: boolean;
  unpulledPrompt: { source: string; target: string; sync: DualBranchSync } | null;
  dirtyPrompt: { kind: GuardedOpKind; target: string | null; files: string[] } | null;
  protectedPrompt: { action: 'merge' | 'push'; preview: ActionPreview } | null;
  onPullAndIntegrate: () => void;
  onSkipPullAndIntegrate: () => void;
  onCancelUnpulled: () => void;
  onResolveDirty: (strategy: DirtyStrategy) => void;
  onCancelDirty: () => void;
  onConfirmProtected: () => void;
  onCancelProtected: () => void;
}

export function FlowModals({
  integrationMode,
  busy,
  unpulledPrompt,
  dirtyPrompt,
  protectedPrompt,
  onPullAndIntegrate,
  onSkipPullAndIntegrate,
  onCancelUnpulled,
  onResolveDirty,
  onCancelDirty,
  onConfirmProtected,
  onCancelProtected,
}: FlowModalsProps) {
  return (
    <>
      {unpulledPrompt && (
        <UnpulledModal
          modeLabel={getModeLabel(integrationMode)}
          sync={unpulledPrompt.sync}
          busy={busy}
          onPullAndContinue={onPullAndIntegrate}
          onContinueWithoutPull={onSkipPullAndIntegrate}
          onCancel={onCancelUnpulled}
        />
      )}
      {dirtyPrompt && (
        <DirtyModal
          kind={dirtyPrompt.kind}
          actionLabel={dirtyPrompt.kind === 'integrate' ? getModeLabel(integrationMode) : undefined}
          target={dirtyPrompt.target}
          files={dirtyPrompt.files}
          busy={busy}
          onStashPop={() => onResolveDirty({ mode: 'stash_and_pop' })}
          onQuickCommit={(message) => onResolveDirty({ mode: 'quick_commit', message })}
          onCancel={onCancelDirty}
        />
      )}
      {protectedPrompt && (
        <PreviewModal
          actionLabel={protectedPrompt.action === 'merge' ? getModeLabel(integrationMode) : 'Push'}
          preview={protectedPrompt.preview}
          busy={busy}
          onConfirm={onConfirmProtected}
          onCancel={onCancelProtected}
        />
      )}
    </>
  );
}
