import { useState } from 'react';
import type { PrListResponse } from './types';

interface PrPanelProps {
  prData: PrListResponse | null;
  currentBranch: string;
  baseBranch: string;
}

export function PrPanel({ prData, currentBranch, baseBranch }: PrPanelProps) {
  const [open, setOpen] = useState(false);

  if (!prData?.github_url && (!prData || prData.prs.length === 0)) {
    return null;
  }

  const ghUrl = prData.github_url;
  const targetBase = baseBranch && baseBranch !== currentBranch ? baseBranch : 'main';
  const compareUrl = ghUrl
    ? `${ghUrl}/compare/${encodeURIComponent(targetBase)}...${encodeURIComponent(currentBranch)}?expand=1`
    : null;

  return (
    <div className="gw-card gw-stack" style={{ padding: '7px 8px' }}>
      <div className="gw-between">
        <button
          type="button"
          className="gw-btn gw-btn-xs"
          style={{ border: 'none', background: 'transparent', padding: 0, fontWeight: 600 }}
          onClick={() => setOpen((v) => !v)}
        >
          {open ? '▾' : '▸'} Pull Requests {prData.gh_available ? `(${prData.prs.length})` : ''}
        </button>
        <div className="gw-row" style={{ gap: 4 }}>
          {compareUrl && currentBranch !== targetBase && (
            <a
              className="gw-btn gw-btn-xs gw-btn-primary"
              style={{ textDecoration: 'none' }}
              href={compareUrl}
              target="_blank"
              rel="noreferrer"
            >
              + Create PR ↗
            </a>
          )}
          {ghUrl && (
            <a
              className="gw-btn gw-btn-xs"
              style={{ textDecoration: 'none' }}
              href={`${ghUrl}/pulls`}
              target="_blank"
              rel="noreferrer"
            >
              PRs ↗
            </a>
          )}
        </div>
      </div>

      {open && (
        <div className="gw-stack" style={{ gap: 4 }}>
          {!prData.gh_available ? (
            <div className="gw-muted">
              Sign in with GitHub (via `+` button) or install `gh` CLI to list private PRs inline.
            </div>
          ) : prData.prs.length === 0 ? (
            <div className="gw-muted">No open pull requests.</div>
          ) : (
            prData.prs.map((pr) => (
              <a
                key={pr.number}
                href={pr.url}
                target="_blank"
                rel="noreferrer"
                className="gw-file-row"
                style={{ textDecoration: 'none', padding: '4px 5px' }}
              >
                <span className="gw-badge gw-badge-ok">#{pr.number}</span>
                <span className="gw-file-name">{pr.title}</span>
                <span className="gw-muted" style={{ marginLeft: 'auto', flexShrink: 0 }}>
                  @{pr.author}
                </span>
              </a>
            ))
          )}
        </div>
      )}
    </div>
  );
}
