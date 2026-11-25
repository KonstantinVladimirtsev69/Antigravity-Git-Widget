import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiError, api } from '../api';
import type { CloneStatusResponse } from '../types';

const CLONE_POLL_INTERVAL_MS = 450;
const MAX_TRANSIENT_POLL_ERRORS = 6;

function isCloneStatusResponse(val: unknown): val is CloneStatusResponse {
  return (
    typeof val === 'object' &&
    val !== null &&
    'stage' in val &&
    'in_progress' in val &&
    'phase_label' in val
  );
}

export function useClone(onCloneSuccess?: (targetPath: string) => void) {
  const [modalOpen, setModalOpen] = useState(false);
  const [status, setStatus] = useState<CloneStatusResponse | null>(null);
  const [polling, setPolling] = useState(false);
  const [starting, setStarting] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [formError, setFormError] = useState('');

  const completedNotifiedRef = useRef(false);
  const onSuccessRef = useRef(onCloneSuccess);
  onSuccessRef.current = onCloneSuccess;

  useEffect(() => {
    let cancelled = false;
    void api
      .getCloneStatus()
      .then((snap) => {
        if (cancelled) return;
        if (snap.in_progress) {
          completedNotifiedRef.current = false;
          setStatus(snap);
          setPolling(true);
        } else if (snap.stage !== 'idle') {
          completedNotifiedRef.current = snap.stage === 'done';
          setStatus(snap);
        }
      })
      .catch(() => null);
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!polling) return;
    let cancelled = false;
    let timer: number | null = null;
    let transientErrors = 0;

    const schedule = () => {
      if (!cancelled) {
        timer = window.setTimeout(() => void tick(), CLONE_POLL_INTERVAL_MS);
      }
    };

    const tick = async () => {
      if (cancelled) return;
      try {
        const next = await api.getCloneStatus();
        if (cancelled) return;
        transientErrors = 0;
        setStatus(next);

        if (next.stage === 'done') {
          setPolling(false);
          setCancelling(false);
          if (!completedNotifiedRef.current) {
            completedNotifiedRef.current = true;
            if (next.target_path) {
              onSuccessRef.current?.(next.target_path);
            }
          }
          return;
        }

        if (next.stage === 'error' || next.stage === 'cancelled' || next.stage === 'idle') {
          setPolling(false);
          setCancelling(false);
          return;
        }

        schedule();
      } catch (err) {
        if (cancelled) return;
        if (err instanceof ApiError && err.status === 401) {
          setFormError(err.message);
          setPolling(false);
          setCancelling(false);
          return;
        }
        transientErrors += 1;
        if (transientErrors >= MAX_TRANSIENT_POLL_ERRORS) {
          setFormError(err instanceof Error ? err.message : String(err));
          setPolling(false);
          setCancelling(false);
          return;
        }
        schedule();
      }
    };

    schedule();
    return () => {
      cancelled = true;
      if (timer !== null) window.clearTimeout(timer);
    };
  }, [polling]);

  const startClone = useCallback(
    async (url: string, parentDir: string, folderName: string): Promise<boolean> => {
      if (starting || status?.in_progress) return false;
      setStarting(true);
      setFormError('');
      completedNotifiedRef.current = false;
      try {
        const snap = await api.startClone({
          url,
          parent_dir: parentDir,
          folder_name: folderName,
        });
        setStatus(snap);
        if (snap.in_progress) {
          setPolling(true);
        }
        return true;
      } catch (err) {
        if (err instanceof ApiError && err.status === 409) {
          const snap = isCloneStatusResponse(err.body)
            ? err.body
            : await api.getCloneStatus().catch(() => null);
          if (snap) {
            setStatus(snap);
            if (snap.in_progress) setPolling(true);
          }
          setFormError('Another clone is already in progress.');
          return false;
        }
        setFormError(err instanceof Error ? err.message : String(err));
        return false;
      } finally {
        setStarting(false);
      }
    },
    [starting, status?.in_progress],
  );

  const cancelClone = useCallback(async () => {
    if (cancelling) return;
    setCancelling(true);
    try {
      const snap = await api.cancelClone();
      setStatus(snap);
      if (!snap.in_progress) {
        setPolling(false);
        setCancelling(false);
      }
    } catch (err) {
      setFormError(err instanceof Error ? err.message : String(err));
      setCancelling(false);
    }
  }, [cancelling]);

  const openModal = useCallback(() => {
    setFormError('');
    setModalOpen(true);
  }, []);

  const closeModal = useCallback(() => {
    setModalOpen(false);
  }, []);

  const reset = useCallback(() => {
    if (status?.in_progress) return;
    setStatus(null);
    setFormError('');
    setCancelling(false);
    completedNotifiedRef.current = false;
  }, [status?.in_progress]);

  return {
    modalOpen,
    status,
    polling,
    starting,
    cancelling,
    formError,
    setFormError,
    openModal,
    closeModal,
    startClone,
    cancelClone,
    reset,
  };
}
