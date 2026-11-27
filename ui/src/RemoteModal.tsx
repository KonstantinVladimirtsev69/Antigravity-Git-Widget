import { useEffect, useRef, useState } from 'react';
import { api } from './api';
import type { GithubDeviceFlowStart } from './types';

interface RemoteModalProps {
  defaultRepoName: string;
  existingRemotes?: string[];
  hasSavedToken: boolean;
  busy: boolean;
  onAddExisting: (name: string, url: string) => Promise<void>;
  onCreateGithub: (payload: {
    name: string;
    description: string;
    private: boolean;
    use_ssh: boolean;
    token?: string;
  }) => Promise<void>;
  onAuthChanged: () => Promise<void>;
  onError: (msg: string) => void;
  onClose: () => void;
}

export function RemoteModal({
  defaultRepoName,
  existingRemotes = [],
  hasSavedToken,
  busy,
  onAddExisting,
  onCreateGithub,
  onAuthChanged,
  onError,
  onClose,
}: RemoteModalProps) {
  const hasExistingRemotes = existingRemotes.length > 0;
  const [mode, setMode] = useState<'github' | 'existing'>(() =>
    hasExistingRemotes ? 'existing' : 'github',
  );
  const [ghName, setGhName] = useState(defaultRepoName);
  const [ghDesc, setGhDesc] = useState('');
  const [isPrivate, setIsPrivate] = useState(true);
  const [useSsh, setUseSsh] = useState(false);
  const [showPat, setShowPat] = useState(false);
  const [token, setToken] = useState('');
  const [oauthFlow, setOauthFlow] = useState<GithubDeviceFlowStart | null>(null);
  const [oauthBusy, setOauthBusy] = useState(false);
  const [copiedCode, setCopiedCode] = useState(false);
  const [remoteName, setRemoteName] = useState(() =>
    existingRemotes.includes('origin') ? '' : 'origin',
  );
  const [remoteUrl, setRemoteUrl] = useState('');
  const pollTimerRef = useRef<number | null>(null);

  useEffect(() => {
    if (hasExistingRemotes) setMode('existing');
  }, [hasExistingRemotes]);

  useEffect(() => {
    return () => {
      if (pollTimerRef.current !== null) window.clearTimeout(pollTimerRef.current);
    };
  }, []);

  useEffect(() => {
    if (!oauthFlow) return;
    let cancelled = false;
    let delayMs = Math.max(5, oauthFlow.interval) * 1000;

    const tick = async () => {
      if (cancelled) return;
      try {
        const res = await api.pollGithubOAuth(oauthFlow.device_code);
        if (cancelled) return;
        if (res.status === 'authorized') {
          setOauthFlow(null);
          await onAuthChanged();
          return;
        }
        if (res.status === 'expired' || res.status === 'denied' || res.status === 'error') {
          setOauthFlow(null);
          onError(
            res.status === 'expired'
              ? 'GitHub authorization code expired. Try again.'
              : res.status === 'denied'
                ? 'GitHub authorization was cancelled.'
                : res.message || 'GitHub OAuth failed.',
          );
          return;
        }
        if (res.interval && res.interval > 0) delayMs = res.interval * 1000;
      } catch (e) {
        if (!cancelled) {
          setOauthFlow(null);
          onError(e instanceof Error ? e.message : String(e));
          return;
        }
      }
      if (!cancelled) {
        pollTimerRef.current = window.setTimeout(() => void tick(), delayMs);
      }
    };

    pollTimerRef.current = window.setTimeout(() => void tick(), delayMs);
    return () => {
      cancelled = true;
      if (pollTimerRef.current !== null) window.clearTimeout(pollTimerRef.current);
    };
  }, [oauthFlow, onAuthChanged, onError]);

  const startOAuth = async () => {
    setOauthBusy(true);
    onError('');
    try {
      const flow = await api.startGithubOAuth();
      setOauthFlow(flow);
      setCopiedCode(false);
      try {
        await navigator.clipboard.writeText(flow.user_code);
        setCopiedCode(true);
      } catch {
        // clipboard may be blocked outside user gesture in some contexts
      }
      window.open(flow.verification_uri, '_blank', 'noopener,noreferrer');
    } catch (e) {
      onError(e instanceof Error ? e.message : String(e));
    } finally {
      setOauthBusy(false);
    }
  };

  return (
    <div className="gw-card gw-stack">
      <div className="gw-between">
        <div className="gw-tabs" style={{ flex: 1 }}>
          {!hasExistingRemotes && (
            <button type="button" className={`gw-tab ${mode === 'github' ? 'active' : ''}`} onClick={() => setMode('github')}>
              Create on GitHub
            </button>
          )}
          <button type="button" className={`gw-tab ${mode === 'existing' ? 'active' : ''}`} onClick={() => setMode('existing')}>
            Link Existing URL
          </button>
        </div>
        <button className="gw-btn gw-btn-xs" onClick={onClose}>✕</button>
      </div>

      {!hasExistingRemotes && mode === 'github' ? (
        <div className="gw-stack" style={{ gap: 6 }}>
          <input className="gw-input" placeholder="Repository name (e.g. git-widget)" value={ghName} onChange={(e) => setGhName(e.target.value)} />
          <input className="gw-input" placeholder="Description (optional)" value={ghDesc} onChange={(e) => setGhDesc(e.target.value)} />
          <div className="gw-between">
            <div className="gw-tabs" style={{ width: 150 }}>
              <button type="button" className={`gw-tab ${isPrivate ? 'active' : ''}`} onClick={() => setIsPrivate(true)}>🔒 Private</button>
              <button type="button" className={`gw-tab ${!isPrivate ? 'active' : ''}`} onClick={() => setIsPrivate(false)}>🌐 Public</button>
            </div>
            <label className="gw-row gw-muted" style={{ cursor: 'pointer', gap: 4 }}>
              <input type="checkbox" checked={useSsh} onChange={(e) => setUseSsh(e.target.checked)} />
              SSH remote
            </label>
          </div>

          {hasSavedToken ? (
            <div className="gw-between" style={{ padding: '4px 6px', borderRadius: 6, background: 'rgba(34, 197, 94, 0.08)', border: '1px solid rgba(34, 197, 94, 0.28)' }}>
              <span style={{ color: '#86efac', fontSize: 11 }}>✓ GitHub connected</span>
              <button
                type="button"
                className="gw-btn gw-btn-xs"
                disabled={busy}
                onClick={() => void api.logoutGithubOAuth().then(onAuthChanged).catch((e) => onError(String(e)))}
              >
                Disconnect
              </button>
            </div>
          ) : oauthFlow ? (
            <div className="gw-card gw-stack" style={{ gap: 5, borderColor: 'rgba(59, 130, 246, 0.45)' }}>
              <div className="gw-between">
                <span className="gw-muted">Enter code on GitHub:</span>
                <button type="button" className="gw-btn gw-btn-xs" onClick={() => setOauthFlow(null)}>Cancel</button>
              </div>
              <div className="gw-between">
                <code style={{ fontSize: 14, fontWeight: 700, letterSpacing: 1.5, color: '#93c5fd' }}>{oauthFlow.user_code}</code>
                <button
                  type="button"
                  className="gw-btn gw-btn-xs"
                  onClick={() => {
                    void navigator.clipboard.writeText(oauthFlow.user_code);
                    setCopiedCode(true);
                  }}
                >
                  {copiedCode ? 'Copied ✓' : 'Copy'}
                </button>
              </div>
              <div className="gw-between">
                <span className="gw-muted">Waiting for authorization...</span>
                <a className="gw-btn gw-btn-xs gw-btn-primary" style={{ textDecoration: 'none' }} href={oauthFlow.verification_uri} target="_blank" rel="noreferrer">
                  Open GitHub ↗
                </a>
              </div>
            </div>
          ) : (
            <div className="gw-stack" style={{ gap: 4 }}>
              <button type="button" className="gw-btn" disabled={busy || oauthBusy} onClick={() => void startOAuth()}>
                {oauthBusy ? 'Starting GitHub login...' : '🔑 Sign in with GitHub'}
              </button>
              <button
                type="button"
                className="gw-btn gw-btn-xs"
                style={{ border: 'none', background: 'transparent', alignSelf: 'flex-start', padding: 0 }}
                onClick={() => setShowPat((v) => !v)}
              >
                <span className="gw-muted">{showPat ? '▾ Hide PAT input' : '▸ Or paste a PAT token manually'}</span>
              </button>
              {showPat && (
                <input className="gw-input" type="password" placeholder="GitHub PAT token (repo scope)" value={token} onChange={(e) => setToken(e.target.value)} />
              )}
            </div>
          )}

          <button
            type="button"
            className="gw-btn gw-btn-primary"
            disabled={busy || !ghName.trim() || (!hasSavedToken && !token.trim() && Boolean(oauthFlow))}
            onClick={() =>
              void onCreateGithub({
                name: ghName.trim(),
                description: ghDesc.trim(),
                private: isPrivate,
                use_ssh: useSsh,
                token: token.trim() || undefined,
              })
            }
          >
            Create &amp; Publish to GitHub
          </button>
        </div>
      ) : (
        <div className="gw-stack" style={{ gap: 6 }}>
          <div className="gw-row">
            <input className="gw-input" style={{ width: 84 }} value={remoteName} onChange={(e) => setRemoteName(e.target.value)} placeholder="origin" />
            <input className="gw-input" value={remoteUrl} onChange={(e) => setRemoteUrl(e.target.value)} placeholder="https://github.com/user/repo.git" />
          </div>
          <button
            type="button"
            className="gw-btn gw-btn-primary"
            disabled={busy || !remoteName.trim() || !remoteUrl.trim()}
            onClick={() => void onAddExisting(remoteName.trim(), remoteUrl.trim())}
          >
            Save Remote
          </button>
        </div>
      )}
    </div>
  );
}
