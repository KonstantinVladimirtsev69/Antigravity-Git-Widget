const FORBIDDEN_SHORTHAND_RE = /[\s<>:"/\\|?*]/;

function isValidShorthandSegment(seg: string): boolean {
  return (
    seg.length > 0 &&
    seg !== '.' &&
    seg !== '..' &&
    !seg.startsWith('-') &&
    !FORBIDDEN_SHORTHAND_RE.test(seg)
  );
}

/** Expands `owner/repo` shorthand to GitHub HTTPS URL for UI preview. */
export function previewCloneUrl(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed || trimmed.startsWith('-') || trimmed.includes('://') || trimmed.includes('@')) {
    return trimmed;
  }
  const slashIdx = trimmed.indexOf('/');
  if (slashIdx <= 0 || trimmed.indexOf('/', slashIdx + 1) !== -1) {
    return trimmed;
  }
  const owner = trimmed.slice(0, slashIdx);
  const repo = trimmed.slice(slashIdx + 1);
  if (!isValidShorthandSegment(owner) || !isValidShorthandSegment(repo)) {
    return trimmed;
  }
  const repoClean = repo.endsWith('.git') ? repo.slice(0, -4) : repo;
  if (!repoClean) return trimmed;
  return `https://github.com/${owner}/${repoClean}.git`;
}

function safeDecodeSegment(raw: string): string {
  try {
    return decodeURIComponent(raw);
  } catch {
    return raw.replace(/%20/gi, ' ');
  }
}

/** Extracts editable target folder name from a clone URL, SCP path, or `owner/repo`. */
export function extractFolderName(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return '';
  const withoutQuery = trimmed.split(/[?#]/, 1)[0] ?? trimmed;
  const cleaned = withoutQuery.replace(/[\\/]+$/, '');
  if (!cleaned) return '';

  let lastSeg = '';
  const schemeIdx = cleaned.indexOf('://');
  if (schemeIdx !== -1) {
    const afterScheme = cleaned.slice(schemeIdx + 3);
    const sepMatch = afterScheme.match(/[\\/]/);
    if (!sepMatch || sepMatch.index === undefined) return '';
    const pathPart = afterScheme.slice(sepMatch.index + 1);
    const parts = pathPart.split(/[\\/]/);
    lastSeg = parts[parts.length - 1] ?? '';
  } else {
    const parts = cleaned.split(/[\\/]/);
    const seg = parts[parts.length - 1] ?? '';
    const colonIdx = seg.lastIndexOf(':');
    lastSeg = colonIdx >= 0 ? seg.slice(colonIdx + 1) : seg;
  }

  const withoutGit =
    lastSeg.length > 4 && lastSeg.slice(-4).toLowerCase() === '.git'
      ? lastSeg.slice(0, -4)
      : lastSeg;

  return safeDecodeSegment(withoutGit).trim();
}
