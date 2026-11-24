import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiError, api } from '../api';
import type { UpdateCheckResponse, UpdateStatusResponse } from '../types';

const POLL_INTERVAL_MS = 400;
const RESTART_TIMEOUT_MS = 20_000;

export function normalizeVersion(version?: string | null): string {
  return (version ?? '').trim().replace(/^v/i, '');
}

function isStatusResponse(val: unknown): val is UpdateStatusResponse {
  return typeof val === 'object' && val !== null && 'stage' in val && 'in_progress' in val;
}

export function useAppUpdater() {
  const [checkInfo, setCheckInfo] = useState<UpdateCheckResponse | null>(null);
  const [status, setStatus] = useState<UpdateStatusResponse | null>(null);
  const [checking, setChecking] = useState(false);
  const [manualError, setManualError] = useState('');
  const [modalOpen, setModalOpen] = useState(false);
  const [polling, setPolling] = useState(false);
  const [restartTimedOut, setRestartTimedOut] = useState(false);
  const [reloadSuggested, setReloadSuggested] = useState(false);
  const inFlightRef = useRef(false);
  const initialVersionRef = useRef('');

  useEffect(() => {
    let cancelled = false;
    void api.checkUpdate(false).then((res) => { if (!cancelled) setCheckInfo(res); }).catch(() => {});
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    if (!polling) return;
    let cancelled = false;
    let timer: number | null = null;
    let restartStartedAt: number | null = null;
    let sawDisconnect = false;
    const stop = () => { inFlightRef.current = false; if (!cancelled) setPolling(false); };
    const schedule = () => { if (!cancelled) timer = window.setTimeout(() => void tick(), POLL_INTERVAL_MS); };
    const checkTimeout = () => {
      if (restartStartedAt === null) restartStartedAt = Date.now();
      if (Date.now() - restartStartedAt > RESTART_TIMEOUT_MS) { setRestartTimedOut(true); stop(); return true; }
      return false;
    };
    const tick = async () => {
      if (cancelled) return;
      try {
        const next = await api.getUpdateStatus();
        if (cancelled) return;
        setStatus(next);
        const initVer = initialVersionRef.current;
        const nextVer = normalizeVersion(next.current_version);
        const versionChanged = Boolean(nextVer && initVer && nextVer !== initVer);
        const recovered = sawDisconnect && next.stage === 'idle' && !next.in_progress;
        if (versionChanged || recovered) { stop(); window.location.reload(); return; }
        if (next.stage === 'error') { stop(); return; }
        if (next.stage === 'restarting' && checkTimeout()) return;
        schedule();
      } catch (err) {
        if (cancelled) return;
        if (err instanceof ApiError && err.status === 401) { setReloadSuggested(true); stop(); return; }
        sawDisconnect = true;
        if (!checkTimeout()) schedule();
      }
    };
    schedule();
    return () => { cancelled = true; if (timer !== null) window.clearTimeout(timer); };
  }, [polling]);

  const checkForUpdates = useCallback(async () => {
    setChecking(true);
    setManualError('');
    try {
      const res = await api.checkUpdate(true);
      setCheckInfo(res);
      if (res.error) setManualError(res.error);
    } catch (err) {
      setManualError(err instanceof Error ? err.message : String(err));
    } finally {
      setChecking(false);
    }
  }, []);

  const currentVersion = normalizeVersion(status?.current_version ?? checkInfo?.current_version);
  const availableVersion =
    checkInfo?.update_available && checkInfo.latest_version ? normalizeVersion(checkInfo.latest_version) : null;
  const isInstalling = Boolean(
    status?.in_progress || (status && status.stage !== 'idle' && status.stage !== 'error'),
  );

  const startInstall = useCallback(async () => {
    if (inFlightRef.current || status?.in_progress) return;
    inFlightRef.current = true;
    setRestartTimedOut(false);
    setReloadSuggested(false);
    initialVersionRef.current = currentVersion;
    try {
      const next = await api.installUpdate();
      setStatus(next);
      if (next.stage === 'error') inFlightRef.current = false;
      else setPolling(true);
    } catch (err) {
      if (err instanceof ApiError && err.status === 409) {
        const snap = isStatusResponse(err.body) ? err.body : await api.getUpdateStatus().catch(() => null);
        if (snap) setStatus(snap);
        if (!snap || snap.in_progress || snap.stage !== 'error') setPolling(true);
        else inFlightRef.current = false;
        return;
      }
      setStatus({
        stage: 'error',
        current_version: currentVersion,
        target_version: checkInfo?.latest_version ?? null,
        message: null,
        error: err instanceof Error ? err.message : String(err),
        in_progress: false,
      });
      inFlightRef.current = false;
    }
  }, [checkInfo?.latest_version, currentVersion, status?.in_progress]);

  const openModal = useCallback(() => setModalOpen(true), []);
  const closeModal = useCallback(() => setModalOpen(false), []);

  return {
    checkInfo, status, checking, manualError, modalOpen, restartTimedOut, reloadSuggested,
    currentVersion, availableVersion, isInstalling, openModal, closeModal, checkForUpdates, startInstall,
  };
}
