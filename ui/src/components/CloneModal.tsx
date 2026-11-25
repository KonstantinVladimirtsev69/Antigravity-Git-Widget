import { useEffect, useState } from 'react';
import { api } from '../api';
import type { AddProjectResult, CloneStatusResponse } from '../types';
import { extractFolderName, previewCloneUrl } from '../utils/cloneUrl';
import { IS_T3_HOST, addProjectToT3Host } from '../utils/hostBridge';
import { CloneProgress } from './CloneProgress';

interface CloneModalProps {
  status: CloneStatusResponse | null;
  starting: boolean;
  cancelling: boolean;
  formError: string;
  defaultParentDir: string;
  onStartClone: (url: string, parentDir: string, folderName: string) => Promise<boolean>;
  onCancelClone: () => void;
  onReset: () => void;
  onClose: () => void;
  onClearError: () => void;
}

function joinPreviewPath(parentDir: string, folderName: string): string {
  const base = parentDir.trim().replace(/[\\/]+$/, '');
  const folder = folderName.trim();
  if (!base) return folder;
  if (!folder) return base;
  const sep = base.includes('/') && !base.includes('\\') ? '/' : '\\';
  return `${base}${sep}${folder}`;
}

export function CloneModal({
  status, starting, cancelling, formError, defaultParentDir,
  onStartClone, onCancelClone, onReset, onClose, onClearError,
}: CloneModalProps) {
  const [url, setUrl] = useState('');
  const [parentDir, setParentDir] = useState(() => defaultParentDir || 'C:\\Projects');
  const [parentEdited, setParentEdited] = useState(false);
  const [folderName, setFolderName] = useState('');
  const [folderEditedManually, setFolderEditedManually] = useState(false);
  const [pickingFolder, setPickingFolder] = useState(false);
  const [actionError, setActionError] = useState('');
  const [copied, setCopied] = useState(false);
  const [addingToT3, setAddingToT3] = useState(false);
  const [t3Result, setT3Result] = useState<AddProjectResult | null>(null);

  const inProgress = Boolean(status?.in_progress) || starting;
  const donePath = status?.stage === 'done' ? status.target_path : null;
  const trimmedUrl = url.trim();
  const expandedPreview = previewCloneUrl(url);
  const showUrlPreview = trimmedUrl.length > 0 && expandedPreview !== trimmedUrl;
  const canStart = !inProgress && trimmedUrl.length > 0 && parentDir.trim().length > 0 && folderName.trim().length > 0;

  useEffect(() => {
    if (!parentEdited && defaultParentDir) setParentDir(defaultParentDir);
  }, [defaultParentDir, parentEdited]);

  useEffect(() => {
    setCopied(false);
    setT3Result(null);
    setActionError('');
  }, [donePath]);

  useEffect(() => {
    if (inProgress) return;
    const onKeyDown = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [inProgress, onClose]);

  const handleUrlChange = (nextUrl: string) => {
    setUrl(nextUrl);
    if (formError) onClearError();
    if (!folderEditedManually) setFolderName(extractFolderName(nextUrl));
  };

  const handleFolderChange = (nextFolder: string) => {
    setFolderName(nextFolder);
    if (formError) onClearError();
    setFolderEditedManually(nextFolder.length > 0);
  };

  const handlePickFolder = async () => {
    if (pickingFolder || inProgress) return;
    setPickingFolder(true);
    setActionError('');
    try {
      const res = await api.pickFolder(parentDir.trim() || null);
      if (!res.cancelled && res.path) {
        setParentDir(res.path);
        setParentEdited(true);
      }
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setPickingFolder(false);
    }
  };

  const handleCopyPath = async (targetPath: string) => {
    setActionError('');
    try {
      await navigator.clipboard.writeText(targetPath);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleOpenExplorer = async (targetPath: string) => {
    setActionError('');
    try {
      await api.openExplorer(targetPath);
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleAddToT3 = async (targetPath: string) => {
    if (addingToT3) return;
    setAddingToT3(true);
    setActionError('');
    try {
      const res = await addProjectToT3Host(targetPath);
      setT3Result(res);
    } finally {
      setAddingToT3(false);
    }
  };

  const handleReset = () => {
    setUrl('');
    setFolderName('');
    setFolderEditedManually(false);
    setParentEdited(false);
    setActionError('');
    setCopied(false);
    setT3Result(null);
    onReset();
  };

  const visibleError = formError && formError !== status?.error ? formError : actionError;
  const t3Color = t3Result?.status === 'error' ? '#fca5a5' : t3Result?.status === 'fallback' ? '#fcd34d' : '#86efac';

  return (
    <div className="gw-card gw-stack" role="dialog" aria-modal="true" aria-labelledby="gw-clone-modal-title" style={{ borderColor: 'rgba(59, 130, 246, 0.45)' }}>
      <div className="gw-between">
        <div id="gw-clone-modal-title" style={{ fontWeight: 600 }}>Clone Repository</div>
        <button type="button" className="gw-btn gw-btn-xs" aria-label="Close clone dialog" onClick={onClose}>✕</button>
      </div>

      {!donePath && (
        <>
          <div className="gw-stack" style={{ gap: 4 }}>
            <label className="gw-muted" htmlFor="gw-clone-url">URL или owner/repo</label>
            <input id="gw-clone-url" className="gw-input" value={url} disabled={inProgress} onChange={(e) => handleUrlChange(e.target.value)} placeholder="https://github.com/owner/repo.git или owner/repo" />
            {showUrlPreview && <div className="gw-muted" style={{ wordBreak: 'break-all' }}>→ {expandedPreview}</div>}
          </div>
          <div className="gw-stack" style={{ gap: 4 }}>
            <label className="gw-muted" htmlFor="gw-clone-parent">Родительская папка</label>
            <div className="gw-row">
              <input id="gw-clone-parent" className="gw-input" value={parentDir} disabled={inProgress} onChange={(e) => { setParentDir(e.target.value); setParentEdited(true); if (formError) onClearError(); }} placeholder="C:\Projects" />
              <button type="button" className="gw-btn" disabled={inProgress || pickingFolder} onClick={() => void handlePickFolder()}>{pickingFolder ? '...' : 'Обзор…'}</button>
            </div>
          </div>
          <div className="gw-stack" style={{ gap: 4 }}>
            <label className="gw-muted" htmlFor="gw-clone-folder">Имя подпапки</label>
            <input id="gw-clone-folder" className="gw-input" value={folderName} disabled={inProgress} onChange={(e) => handleFolderChange(e.target.value)} placeholder="my-repo" />
            {parentDir.trim() && folderName.trim() && <div className="gw-muted" style={{ wordBreak: 'break-all' }}>Путь: {joinPreviewPath(parentDir, folderName)}</div>}
          </div>
        </>
      )}

      {status && status.stage !== 'idle' && (status.in_progress || !formError) && <CloneProgress status={status} cancelling={cancelling} onCancel={onCancelClone} />}
      {visibleError && <div style={{ color: '#fca5a5', fontSize: 11, wordBreak: 'break-word' }}>{visibleError}</div>}

      {donePath ? (
        <div className="gw-stack" style={{ gap: 6 }}>
          <div className="gw-muted" style={{ wordBreak: 'break-all' }}>Склонировано: {donePath}</div>
          <div className="gw-row" style={{ flexWrap: 'wrap' }}>
            <button type="button" className="gw-btn" onClick={() => void handleOpenExplorer(donePath)}>Открыть в Проводнике</button>
            <button type="button" className="gw-btn" onClick={() => void handleCopyPath(donePath)}>{copied ? 'Скопировано' : 'Скопировать путь'}</button>
            {IS_T3_HOST && (
              <button type="button" className="gw-btn gw-btn-primary" disabled={addingToT3} onClick={() => void handleAddToT3(donePath)}>
                {addingToT3 ? 'Добавление...' : 'Добавить проект в T3 Code'}
              </button>
            )}
          </div>
          {t3Result && <div style={{ color: t3Color, fontSize: 11, wordBreak: 'break-word' }}>{t3Result.message || t3Result.status}</div>}
          <div className="gw-row">
            <button type="button" className="gw-btn" style={{ flex: 1 }} onClick={handleReset}>Клонировать ещё</button>
            <button type="button" className="gw-btn" onClick={onClose}>Закрыть</button>
          </div>
        </div>
      ) : (
        <div className="gw-row">
          <button type="button" className="gw-btn gw-btn-primary" style={{ flex: 1 }} disabled={!canStart} onClick={() => { if (canStart) { setActionError(''); void onStartClone(trimmedUrl, parentDir.trim(), folderName.trim()); } }}>
            {inProgress ? 'Cloning...' : 'Clone'}
          </button>
          {status && status.stage !== 'idle' && !status.in_progress && <button type="button" className="gw-btn" onClick={handleReset}>Reset</button>}
          <button type="button" className="gw-btn" onClick={onClose}>Close</button>
        </div>
      )}
    </div>
  );
}
