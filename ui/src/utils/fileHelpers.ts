export function splitPath(fullPath: string): { name: string; dir: string } {
  const norm = fullPath.replace(/\\/g, '/');
  const idx = norm.lastIndexOf('/');
  if (idx === -1) return { name: norm, dir: '' };
  return { name: norm.slice(idx + 1), dir: norm.slice(0, idx) };
}

export function statusClass(code: string): string {
  const ch = code.charAt(0).toUpperCase();
  if (ch === 'A' || ch === 'U' || ch === '?') return 'gw-st-add';
  if (ch === 'D') return 'gw-st-del';
  return 'gw-st-mod';
}

export function isProtectedBranch(branch: string): boolean {
  const b = branch.trim();
  return b === 'main' || b === 'master';
}
