import { useState } from 'react';

interface RemoteModalProps {
  defaultRepoName: string;
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
  onClose: () => void;
}

export function RemoteModal({
  defaultRepoName,
  hasSavedToken,
  busy,
  onAddExisting,
  onCreateGithub,
  onClose,
}: RemoteModalProps) {
  const [mode, setMode] = useState<'github' | 'existing'>('github');
  const [ghName, setGhName] = useState(defaultRepoName);
  const [ghDesc, setGhDesc] = useState('');
  const [isPrivate, setIsPrivate] = useState(true);
  const [useSsh, setUseSsh] = useState(false);
  const [token, setToken] = useState('');
  const [remoteName, setRemoteName] = useState('origin');
  const [remoteUrl, setRemoteUrl] = useState('');

  return (
    <div className="gw-card gw-stack">
      <div className="gw-between">
        <div className="gw-tabs" style={{ flex: 1 }}>
          <button
            type="button"
            className={`gw-tab ${mode === 'github' ? 'active' : ''}`}
            onClick={() => setMode('github')}
          >
            Create on GitHub
          </button>
          <button
            type="button"
            className={`gw-tab ${mode === 'existing' ? 'active' : ''}`}
            onClick={() => setMode('existing')}
          >
            Link Existing URL
          </button>
        </div>
        <button className="gw-btn gw-btn-xs" onClick={onClose}>
          ✕
        </button>
      </div>

      {mode === 'github' ? (
        <div className="gw-stack" style={{ gap: 6 }}>
          <input
            className="gw-input"
            placeholder="Repository name (e.g. git-widget)"
            value={ghName}
            onChange={(e) => setGhName(e.target.value)}
          />
          <input
            className="gw-input"
            placeholder="Description (optional)"
            value={ghDesc}
            onChange={(e) => setGhDesc(e.target.value)}
          />
          <div className="gw-between">
            <div className="gw-tabs" style={{ width: 150 }}>
              <button
                type="button"
                className={`gw-tab ${isPrivate ? 'active' : ''}`}
                onClick={() => setIsPrivate(true)}
              >
                🔒 Private
              </button>
              <button
                type="button"
                className={`gw-tab ${!isPrivate ? 'active' : ''}`}
                onClick={() => setIsPrivate(false)}
              >
                🌐 Public
              </button>
            </div>
            <label className="gw-row gw-muted" style={{ cursor: 'pointer', gap: 4 }}>
              <input
                type="checkbox"
                checked={useSsh}
                onChange={(e) => setUseSsh(e.target.checked)}
              />
              SSH remote
            </label>
          </div>
          <input
            className="gw-input"
            type="password"
            placeholder={
              hasSavedToken
                ? 'GitHub Token saved ✓ (leave blank or enter new)'
                : 'GitHub PAT token (optional if GCM / gh is signed in)'
            }
            value={token}
            onChange={(e) => setToken(e.target.value)}
          />
          <button
            type="button"
            className="gw-btn gw-btn-primary"
            disabled={busy || !ghName.trim()}
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
            <input
              className="gw-input"
              style={{ width: 84 }}
              value={remoteName}
              onChange={(e) => setRemoteName(e.target.value)}
              placeholder="origin"
            />
            <input
              className="gw-input"
              value={remoteUrl}
              onChange={(e) => setRemoteUrl(e.target.value)}
              placeholder="https://github.com/user/repo.git"
            />
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
