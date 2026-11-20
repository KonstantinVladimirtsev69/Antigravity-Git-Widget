import { useMemo, useState } from 'react';
import { api } from './api';
import { layoutGraph } from './graphLayout';
import type { AuthorStat, CommitDetails, GraphCommit } from './types';
import { splitPath, statusClass } from './utils/fileHelpers';

interface GraphPanelProps {
  repo: string;
  commits: GraphCommit[];
  authors: AuthorStat[];
  githubUrl: string | null;
  allBranches: boolean;
  authorFilter: string;
  busy: boolean;
  onToggleAll: (all: boolean) => void;
  onAuthorFilter: (author: string) => void;
  onCherryPick: (hash: string) => void;
  onInspectCommit: (hash: string, focusFile?: string) => void;
  onNotify: (msg: string) => void;
}

export function GraphPanel({
  repo,
  commits,
  authors,
  githubUrl,
  allBranches,
  authorFilter,
  busy,
  onToggleAll,
  onAuthorFilter,
  onCherryPick,
  onInspectCommit,
  onNotify,
}: GraphPanelProps) {
  const [open, setOpen] = useState(true);
  const [expandedHash, setExpandedHash] = useState<string | null>(null);
  const [detailsCache, setDetailsCache] = useState<Record<string, CommitDetails>>({});
  const [loadingHash, setLoadingHash] = useState<string | null>(null);

  const rows = useMemo(() => layoutGraph(commits), [commits]);

  const displayedAuthors = useMemo(() => {
    const top = authors.slice(0, 6);
    if (authorFilter && !top.some((a) => a.name === authorFilter)) {
      const matched = authors.find((a) => a.name === authorFilter);
      top.push(matched ?? { name: authorFilter, count: commits.length, last_date: '' });
    }
    return top;
  }, [authors, authorFilter, commits.length]);

  const toggleCommit = async (hash: string) => {
    if (expandedHash === hash) {
      setExpandedHash(null);
      return;
    }
    setExpandedHash(hash);
    if (!detailsCache[hash]) {
      setLoadingHash(hash);
      try {
        const d = await api.getCommitDetails(repo, hash);
        setDetailsCache((prev) => ({ ...prev, [hash]: d }));
      } catch {
        // ignore
      } finally {
        setLoadingHash(null);
      }
    }
  };

  const LANE_W = 12;
  const ROW_H = 26;

  return (
    <div className="gw-card gw-stack" style={{ padding: '7px 8px' }}>
      <div className="gw-between">
        <button
          type="button"
          className="gw-btn gw-btn-xs"
          style={{ border: 'none', background: 'transparent', padding: 0, fontWeight: 600 }}
          onClick={() => setOpen((v) => !v)}
        >
          {open ? '▾' : '▸'} Graph ({commits.length})
        </button>
        <div className="gw-row" style={{ gap: 4 }}>
          <button
            type="button"
            className={`gw-btn gw-btn-xs ${allBranches ? 'gw-btn-primary' : ''}`}
            onClick={() => onToggleAll(!allBranches)}
            title="Show all branches or current branch only"
          >
            {allBranches ? 'All' : 'Branch'}
          </button>
        </div>
      </div>

      {open && (
        <>
          {(authors.length > 1 || Boolean(authorFilter)) && (
            <div className="gw-row" style={{ flexWrap: 'wrap', gap: 4 }}>
              <button
                type="button"
                className={`gw-chip ${!authorFilter ? 'active' : ''}`}
                onClick={() => onAuthorFilter('')}
              >
                All devs
              </button>
              {displayedAuthors.map((a) => (
                <button
                  key={a.name}
                  type="button"
                  className={`gw-chip ${authorFilter === a.name ? 'active' : ''}`}
                  title={`${a.name} · ${a.count} commits${a.last_date ? ` · last ${a.last_date}` : ''}`}
                  onClick={() => onAuthorFilter(authorFilter === a.name ? '' : a.name)}
                >
                  {a.name.split(' ')[0]} <span className="gw-muted">{a.count}</span>
                </button>
              ))}
            </div>
          )}

          <div className="gw-graph-list">
            {rows.length === 0 && <div className="gw-muted" style={{ padding: 6 }}>No commits found.</div>}
            {rows.map((row) => {
              const c = row.commit;
              const isExpanded = expandedHash === c.hash;
              const details = detailsCache[c.hash];
              const svgW = Math.max(18, row.maxLanes * LANE_W + 6);
              const cx = row.lane * LANE_W + 8;
              const cy = ROW_H / 2;

              return (
                <div key={c.hash} className="gw-graph-entry">
                  <button
                    type="button"
                    className={`gw-graph-row ${isExpanded ? 'active' : ''}`}
                    title={`${c.author_name} · ${c.date_rel} (${c.date_exact})\n${c.subject}`}
                    onClick={() => void toggleCommit(c.hash)}
                  >
                    <svg width={svgW} height={ROW_H} style={{ flexShrink: 0, display: 'block' }}>
                      {row.incoming.map((e, i) => {
                        const x1 = e.fromLane * LANE_W + 8;
                        const x2 = e.toLane * LANE_W + 8;
                        return (
                          <path
                            key={`in-${i}`}
                            d={`M ${x1} 0 C ${x1} ${cy * 0.6}, ${x2} ${cy * 0.4}, ${x2} ${cy}`}
                            stroke={e.color}
                            strokeWidth="1.6"
                            fill="none"
                          />
                        );
                      })}
                      {row.outgoing.map((e, i) => {
                        const x1 = e.fromLane * LANE_W + 8;
                        const x2 = e.toLane * LANE_W + 8;
                        return (
                          <path
                            key={`out-${i}`}
                            d={`M ${x1} ${cy} C ${x1} ${cy + (ROW_H - cy) * 0.6}, ${x2} ${cy + (ROW_H - cy) * 0.4}, ${x2} ${ROW_H}`}
                            stroke={e.color}
                            strokeWidth="1.6"
                            fill="none"
                          />
                        );
                      })}
                      <circle
                        cx={cx}
                        cy={cy}
                        r={c.is_head ? 4 : 3.2}
                        fill={c.is_head ? '#121215' : row.color}
                        stroke={row.color}
                        strokeWidth={c.is_head ? 2 : 1}
                      />
                    </svg>

                    <div className="gw-graph-main">
                      <span className="gw-graph-subject">{c.subject}</span>
                      {c.refs.slice(0, 2).map((ref) => (
                        <span key={`${ref.kind}:${ref.name}`} className={`gw-ref gw-ref-${ref.kind}`}>
                          {ref.kind === 'remote' ? '☁ ' : ''}
                          {ref.name}
                        </span>
                      ))}
                    </div>

                    <span className="gw-graph-meta">{c.author_name.split(' ')[0]}</span>
                  </button>

                  {isExpanded && (
                    <div className="gw-commit-card">
                      <div className="gw-between">
                        <span style={{ color: '#93c5fd', fontWeight: 500 }}>@{c.author_name}</span>
                        <span className="gw-muted">
                          {c.date_rel} · {c.date_exact}
                        </span>
                      </div>
                      <div style={{ margin: '4px 0', color: 'var(--gw-fg-strong)' }}>{c.subject}</div>

                      {loadingHash === c.hash && <div className="gw-muted">Loading files...</div>}

                      {details && (
                        <>
                          <div className="gw-muted" style={{ marginBottom: 4 }}>
                            {details.files.length} file(s) changed,{' '}
                            <span className="gw-st-add">+{details.total_additions}</span>{' '}
                            <span className="gw-st-del">-{details.total_deletions}</span>
                          </div>
                          <div className="gw-stack" style={{ gap: 1, maxHeight: 120, overflowY: 'auto' }}>
                            {details.files.map((f) => {
                              const { name, dir } = splitPath(f.path);
                              return (
                                <button
                                  key={f.path}
                                  type="button"
                                  className="gw-file-row"
                                  onClick={() => onInspectCommit(c.hash, f.path)}
                                >
                                  <span className="gw-file-name">{name}</span>
                                  {dir && <span className="gw-file-dir">{dir}</span>}
                                  <span className={`gw-file-st ${statusClass(f.status)}`}>{f.status}</span>
                                </button>
                              );
                            })}
                          </div>
                        </>
                      )}

                      <div className="gw-row" style={{ marginTop: 6, flexWrap: 'wrap' }}>
                        <button
                          type="button"
                          className="gw-btn gw-btn-xs"
                          onClick={() => {
                            void navigator.clipboard.writeText(c.hash);
                            onNotify(`Copied ${c.short_hash}`);
                          }}
                        >
                          {c.short_hash} ⎘
                        </button>
                        <button
                          type="button"
                          className="gw-btn gw-btn-xs"
                          onClick={() => onInspectCommit(c.hash)}
                        >
                          Diff
                        </button>
                        <button
                          type="button"
                          className="gw-btn gw-btn-xs"
                          disabled={busy}
                          onClick={() => onCherryPick(c.hash)}
                        >
                          Cherry-Pick
                        </button>
                        {githubUrl && (
                          <a
                            className="gw-btn gw-btn-xs"
                            style={{ textDecoration: 'none' }}
                            href={`${githubUrl}/commit/${c.hash}`}
                            target="_blank"
                            rel="noreferrer"
                          >
                            GitHub ↗
                          </a>
                        )}
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}
