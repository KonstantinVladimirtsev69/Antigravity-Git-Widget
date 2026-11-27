import { useEffect, useRef, useState } from 'react';
import type { MutableRefObject } from 'react';
import { CONVERSATION_ID, api } from '../api';
import { deliverPromptToHost } from '../utils/hostBridge';
import { getModeLabel } from '../IntegrateSplitButton';
import type {
  ActionPreview,
  DirtyStrategy,
  DualBranchSync,
  GuardedOpKind,
  IntegrationMode,
  RepoStatus,
} from '../types';
import { isProtectedBranch } from '../utils/fileHelpers';
import { MODE_STORAGE_KEY, VALID_MODES } from './useRepoData';
import type { RefreshOptions } from './useRepoData';

function loadStoredMode(): IntegrationMode {
  try {
    const raw = window.localStorage.getItem(MODE_STORAGE_KEY);
    if (raw && VALID_MODES.includes(raw as IntegrationMode)) return raw as IntegrationMode;
  } catch { /* ignore */ }
  return 'merge';
}

interface FlowParams {
  selectedRepo: string;
  status: RepoStatus | null;
  selectedRemote: string;
  mergeTarget: string;
  busy: boolean;
  setBusy: (v: boolean) => void;
  setNotice: (v: string) => void;
  setError: (v: string) => void;
  activeRepoRef: MutableRefObject<string>;
  lastFetchAtRef: MutableRefObject<number>;
  refreshRepoData: (repo: string, opts?: RefreshOptions) => Promise<void>;
}

export function useIntegrationFlow(params: FlowParams) {
  const { selectedRepo, status, selectedRemote, mergeTarget, busy, setBusy, setNotice, setError, activeRepoRef, lastFetchAtRef, refreshRepoData } = params;
  const [mergeDir, setMergeDir] = useState<'into_current' | 'current_into_target'>('current_into_target');
  const [integrationMode, setIntegrationMode] = useState<IntegrationMode>(loadStoredMode);
  const [dualSync, setDualSync] = useState<DualBranchSync | null>(null);
  const [scanningSync, setScanningSync] = useState<boolean>(false);
  const [dirtyPrompt, setDirtyPrompt] = useState<{
    kind: GuardedOpKind;
    target: string | null;
    files: string[];
    integratePair?: { source: string; target: string };
  } | null>(null);
  const [unpulledPrompt, setUnpulledPrompt] = useState<{
    source: string;
    target: string;
    sync: DualBranchSync;
  } | null>(null);
  const [protectedPrompt, setProtectedPrompt] = useState<{
    action: 'merge' | 'push';
    preview: ActionPreview;
    source?: string;
    target?: string;
  } | null>(null);

  const activeRemoteRef = useRef(selectedRemote);
  activeRemoteRef.current = selectedRemote;

  useEffect(() => {
    setDualSync(null);
  }, [selectedRemote]);

  useEffect(() => {
    if (!selectedRepo || !status || !mergeTarget || mergeTarget === status.branch || busy) {
      if (!mergeTarget || (status && mergeTarget === status.branch)) setDualSync(null);
      return;
    }
    let cancelled = false;
    const repo = selectedRepo;
    const remote = selectedRemote;
    const source = mergeDir === 'current_into_target' ? status.branch : mergeTarget;
    const target = mergeDir === 'current_into_target' ? mergeTarget : status.branch;
    const shouldFetch = status.remotes.length > 0 && Date.now() - lastFetchAtRef.current > 20000;
    setScanningSync(shouldFetch);

    void api
      .scanSync(repo, source, target, remote || undefined, shouldFetch)
      .then((res) => {
        if (cancelled || activeRepoRef.current !== repo || activeRemoteRef.current !== remote) return;
        if (shouldFetch) lastFetchAtRef.current = Date.now();
        setDualSync(res);
        if (shouldFetch) void refreshRepoData(repo, { lightweight: true });
      })
      .catch(() => {})
      .finally(() => { if (!cancelled) setScanningSync(false); });
    return () => { cancelled = true; };
  }, [selectedRepo, selectedRemote, mergeTarget, status?.branch, mergeDir]); // eslint-disable-line react-hooks/exhaustive-deps

  const runSimple = async (fn: () => Promise<{ message?: string }>, fallback = 'Done'): Promise<boolean> => {
    setBusy(true);
    setError('');
    setNotice('');
    try {
      const res = await fn();
      setNotice(res.message || fallback);
      await refreshRepoData(selectedRepo);
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      return false;
    } finally {
      setBusy(false);
    }
  };

  const executeGuarded = async (kind: GuardedOpKind, target: string | null, strategy: DirtyStrategy) => {
    if (!selectedRepo) return;
    setBusy(true);
    setError('');
    setNotice('');
    try {
      const res = await api.runGuarded(
        selectedRepo,
        kind,
        target,
        strategy,
        kind === 'checkout' ? selectedRemote || undefined : undefined,
      );
      if (res.blocked_dirty) {
        setDirtyPrompt({ kind, target, files: res.dirty_files });
      } else {
        setDirtyPrompt(null);
        if (kind === 'pull') lastFetchAtRef.current = Date.now();
        if (res.op_result) setNotice(res.op_result.message || `${kind} completed`);
        await refreshRepoData(selectedRepo);
      }
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); } finally { setBusy(false); }
  };

  const handleIntegrationModeChange = (mode: IntegrationMode) => {
    setIntegrationMode(mode);
    try { window.localStorage.setItem(MODE_STORAGE_KEY, mode); } catch { /* ignore */ }
    void api.saveIntegrationMode(mode).catch(() => {});
  };

  const getPair = (): { source: string; target: string } | null => {
    if (!status || !mergeTarget || mergeTarget === status.branch) return null;
    return mergeDir === 'current_into_target'
      ? { source: status.branch, target: mergeTarget }
      : { source: mergeTarget, target: status.branch };
  };

  const executeIntegrate = async (source: string, target: string, strategy: DirtyStrategy) => {
    if (!selectedRepo) return;
    setBusy(true);
    setError('');
    setNotice('');
    try {
      const res = await api.integrate(selectedRepo, source, target, integrationMode, strategy);
      if (res.blocked_dirty) {
        setDirtyPrompt({ kind: 'integrate', target: `${source} → ${target}`, files: res.dirty_files, integratePair: { source, target } });
      } else {
        setDirtyPrompt(null);
        lastFetchAtRef.current = Date.now();
        setNotice(res.op_result?.message || `${getModeLabel(integrationMode)}: ${source} → ${target}`);
        await refreshRepoData(selectedRepo);
      }
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); } finally { setBusy(false); }
  };

  const proceedIntegration = async (source: string, target: string) => {
    if (!selectedRepo) return;
    if (!isProtectedBranch(target)) {
      await executeIntegrate(source, target, { mode: 'fail_if_dirty' });
      return;
    }
    setBusy(true);
    setError('');
    try {
      const preview = await api.previewIntegrate(selectedRepo, source, target, integrationMode);
      setProtectedPrompt({ action: 'merge', preview, source, target });
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); } finally { setBusy(false); }
  };

  const handleIntegrateClick = async () => {
    const pair = getPair();
    if (!selectedRepo || !pair || !status) return;
    setBusy(true);
    setError('');
    try {
      const hasRemote = status.remotes.length > 0;
      const sync = await api.scanSync(selectedRepo, pair.source, pair.target, selectedRemote || undefined, hasRemote);
      if (hasRemote) lastFetchAtRef.current = Date.now();
      setDualSync(sync);
      if (sync.any_behind) { setUnpulledPrompt({ source: pair.source, target: pair.target, sync }); return; }
      await proceedIntegration(pair.source, pair.target);
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); } finally { setBusy(false); }
  };

  const handlePullBoth = async () => {
    const pair = getPair();
    if (!selectedRepo || !pair) return;
    lastFetchAtRef.current = Date.now();
    await runSimple(() => api.pullBoth(selectedRepo, pair.source, pair.target, selectedRemote || undefined), 'Branches pulled');
    setDualSync(null);
  };

  const handlePushClick = async (onMissingRemote: () => void) => {
    if (!selectedRepo || !status) return;
    if (status.remotes.length === 0) { onMissingRemote(); setError('No git remote configured. Add remote below.'); return; }
    if (!status.upstream) { await runSimple(() => api.publish(selectedRepo, selectedRemote || status.remotes[0])); return; }
    if (!isProtectedBranch(status.branch)) { await runSimple(() => api.push(selectedRepo, selectedRemote || undefined), 'Pushed'); return; }
    setBusy(true);
    setError('');
    try {
      setProtectedPrompt({
        action: 'push',
        preview: await api.previewPush(selectedRepo, selectedRemote || undefined),
      });
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); } finally { setBusy(false); }
  };

  const confirmPullAndIntegrate = async () => {
    if (!unpulledPrompt) return;
    const { source, target } = unpulledPrompt;
    setUnpulledPrompt(null);
    setBusy(true);
    setError('');
    try {
      const res = await api.pullBoth(selectedRepo, source, target, selectedRemote || undefined);
      lastFetchAtRef.current = Date.now();
      await refreshRepoData(selectedRepo);
      if (res.has_conflicts || !res.success) { setError(res.message || 'Pull resulted in conflicts'); return; }
      setDualSync(null);
      await proceedIntegration(source, target);
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); } finally { setBusy(false); }
  };

  const skipPullAndIntegrate = async () => {
    if (!unpulledPrompt) return;
    const { source, target } = unpulledPrompt;
    setUnpulledPrompt(null);
    await proceedIntegration(source, target);
  };

  const resolveDirtyPrompt = (strategy: DirtyStrategy) => {
    if (!dirtyPrompt) return;
    const dp = dirtyPrompt;
    setDirtyPrompt(null);
    if (dp.kind === 'integrate' && dp.integratePair) void executeIntegrate(dp.integratePair.source, dp.integratePair.target, strategy);
    else void executeGuarded(dp.kind, dp.target, strategy);
  };

  const confirmProtectedPrompt = async () => {
    if (!protectedPrompt) return;
    const pp = protectedPrompt;
    setProtectedPrompt(null);
    if (pp.action === 'merge') await executeIntegrate(pp.source ?? pp.preview.source, pp.target ?? pp.preview.target, { mode: 'fail_if_dirty' });
    else await runSimple(() => api.push(selectedRepo, selectedRemote || undefined), 'Pushed');
  };

  const sendConflict = async () => {
    if (!selectedRepo) return;
    setBusy(true);
    setError('');
    try {
      const res = await api.sendConflictToChat(selectedRepo, CONVERSATION_ID);
      const msg = await deliverPromptToHost(
        res.prompt,
        res.sent,
        'Conflict context sent to AI chat',
      );
      setNotice(msg);
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); } finally { setBusy(false); }
  };

  return {
    mergeDir, setMergeDir, integrationMode, setIntegrationMode, dualSync, scanningSync,
    dirtyPrompt, setDirtyPrompt, unpulledPrompt, setUnpulledPrompt, protectedPrompt, setProtectedPrompt,
    runSimple, executeGuarded, handleIntegrationModeChange, handleIntegrateClick, handlePullBoth,
    handlePushClick, confirmPullAndIntegrate, skipPullAndIntegrate, resolveDirtyPrompt, confirmProtectedPrompt, sendConflict,
  };
}
