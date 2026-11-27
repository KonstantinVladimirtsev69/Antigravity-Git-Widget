import { useState } from 'react';
import type { ReactNode } from 'react';
import { Picker } from '../Picker';

interface BranchCreateDeleteProps {
  otherLocals: string[];
  busy: boolean;
  onCreateBranch: (name: string) => Promise<void>;
  onDeleteBranch: (name: string, force: boolean) => Promise<void>;
  children: ReactNode;
}

export function BranchCreateDelete({
  otherLocals,
  busy,
  onCreateBranch,
  onDeleteBranch,
  children,
}: BranchCreateDeleteProps) {
  const [branchMode, setBranchMode] = useState<'none' | 'create' | 'delete'>('none');
  const [newBranch, setNewBranch] = useState('');
  const [delBranch, setDelBranch] = useState('');
  const [forceDel, setForceDel] = useState(false);

  const submitCreate = () => {
    const name = newBranch.trim();
    if (!name) return;
    setNewBranch('');
    setBranchMode('none');
    void onCreateBranch(name);
  };

  return (
    <>
      <div className="gw-row">
        <div style={{ flex: 1, minWidth: 0 }}>{children}</div>
        <button
          type="button"
          className="gw-btn"
          title="New branch"
          disabled={busy}
          onClick={() => setBranchMode((m) => (m === 'create' ? 'none' : 'create'))}
        >
          +
        </button>
        <button
          type="button"
          className="gw-btn"
          title="Delete branch"
          disabled={busy || otherLocals.length === 0}
          onClick={() => {
            setDelBranch((prev) => (prev && otherLocals.includes(prev) ? prev : otherLocals[0] ?? ''));
            setForceDel(false);
            setBranchMode((m) => (m === 'delete' ? 'none' : 'delete'));
          }}
        >
          ⌫
        </button>
      </div>

      {branchMode === 'create' && (
        <div className="gw-row">
          <input
            className="gw-input"
            placeholder="feat/new-branch"
            value={newBranch}
            onChange={(e) => setNewBranch(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && newBranch.trim()) submitCreate();
            }}
          />
          <button
            type="button"
            className="gw-btn gw-btn-primary"
            disabled={busy || !newBranch.trim()}
            onClick={submitCreate}
          >
            Create
          </button>
        </div>
      )}

      {branchMode === 'delete' && (
        <div className="gw-stack" style={{ gap: 5 }}>
          <div className="gw-row">
            <div style={{ flex: 1, minWidth: 0 }}>
              <Picker
                value={delBranch}
                placeholder="Branch to delete..."
                options={otherLocals.map((b) => ({ value: b, label: b }))}
                onChange={setDelBranch}
              />
            </div>
            <button
              type="button"
              className="gw-btn gw-btn-danger"
              disabled={busy || !delBranch}
              onClick={() => {
                const name = delBranch;
                setBranchMode('none');
                void onDeleteBranch(name, forceDel);
              }}
            >
              Delete
            </button>
          </div>
          <label className="gw-row gw-muted" style={{ cursor: 'pointer' }}>
            <input
              type="checkbox"
              checked={forceDel}
              onChange={(e) => setForceDel(e.target.checked)}
            />
            Force delete (-D)
          </label>
        </div>
      )}
    </>
  );
}
