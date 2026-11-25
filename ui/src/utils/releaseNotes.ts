export interface ListItem {
  indent: number;
  text: string;
}

export type ReleaseNoteBlock =
  | { type: 'heading'; level: 1 | 2 | 3 | 4; text: string }
  | { type: 'list'; ordered: boolean; items: ListItem[] }
  | { type: 'code'; text: string }
  | { type: 'paragraph'; text: string };

export type InlineToken =
  | { type: 'text'; value: string }
  | { type: 'bold'; value: string }
  | { type: 'code'; value: string }
  | { type: 'link'; label: string; href: string };

function isSafeHttpUrl(url: string): boolean {
  return /^https?:\/\/[^\s)]+$/i.test(url);
}

/**
 * Parses inline markdown tokens (`**bold**`, `` `code` ``, `[label](url)`, and bare `https://` URLs).
 */
export function parseInlineMarkdown(input: string): InlineToken[] {
  const tokens: InlineToken[] = [];
  const pattern = /(`[^`]+`|\*\*[^*]+\*\*|\[[^\]]+\]\((?:[^()\s]+|\([^()]*\))+\)|https?:\/\/[^\s<>()]+[^\s<>().,;!?])/g;
  let lastIndex = 0;

  for (const match of input.matchAll(pattern)) {
    const idx = match.index ?? 0;
    if (idx > lastIndex) {
      tokens.push({ type: 'text', value: input.slice(lastIndex, idx) });
    }

    const raw = match[0];
    if (raw.startsWith('`') && raw.endsWith('`') && raw.length >= 2) {
      tokens.push({ type: 'code', value: raw.slice(1, -1) });
    } else if (raw.startsWith('**') && raw.endsWith('**') && raw.length >= 4) {
      tokens.push({ type: 'bold', value: raw.slice(2, -2) });
    } else if (raw.startsWith('[')) {
      const sep = raw.indexOf('](');
      const label = raw.slice(1, sep);
      const href = raw.slice(sep + 2, -1).trim();
      if (isSafeHttpUrl(href)) {
        tokens.push({ type: 'link', label, href });
      } else {
        tokens.push({ type: 'text', value: label });
      }
    } else if (isSafeHttpUrl(raw)) {
      tokens.push({ type: 'link', label: raw, href: raw });
    } else {
      tokens.push({ type: 'text', value: raw });
    }

    lastIndex = idx + raw.length;
  }

  if (lastIndex < input.length) {
    tokens.push({ type: 'text', value: input.slice(lastIndex) });
  }

  return tokens;
}

/**
 * Parses Keep-a-Changelog / GitHub Release markdown into structured blocks.
 */
export function parseReleaseNotes(markdown: string): ReleaseNoteBlock[] {
  const lines = markdown.replace(/\r\n?/g, '\n').split('\n');
  const blocks: ReleaseNoteBlock[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];
    const trimmed = line.trim();

    if (!trimmed) {
      i += 1;
      continue;
    }

    if (trimmed.startsWith('```')) {
      const codeLines: string[] = [];
      i += 1;
      while (i < lines.length && !lines[i].trim().startsWith('```')) {
        codeLines.push(lines[i]);
        i += 1;
      }
      if (i < lines.length) i += 1;
      blocks.push({ type: 'code', text: codeLines.join('\n') });
      continue;
    }

    const headingMatch = trimmed.match(/^(#{1,4})\s+(.+)$/);
    if (headingMatch) {
      const level = headingMatch[1].length as 1 | 2 | 3 | 4;
      blocks.push({ type: 'heading', level, text: headingMatch[2].trim() });
      i += 1;
      continue;
    }

    const bulletMatch = line.match(/^(\s*)[-*]\s+(.+)$/);
    const orderedMatch = line.match(/^(\s*)\d+\.\s+(.+)$/);
    if (bulletMatch || orderedMatch) {
      const ordered = Boolean(orderedMatch);
      const items: ListItem[] = [];

      while (i < lines.length) {
        const cur = lines[i];
        const m = ordered ? cur.match(/^(\s*)\d+\.\s+(.+)$/) : cur.match(/^(\s*)[-*]\s+(.+)$/);
        if (!m) break;
        items.push({
          indent: Math.min(3, Math.floor(m[1].length / 2)),
          text: m[2].trim(),
        });
        i += 1;
      }

      blocks.push({ type: 'list', ordered, items });
      continue;
    }

    const paraLines: string[] = [];
    while (i < lines.length) {
      const cur = lines[i];
      const curTrimmed = cur.trim();
      if (
        !curTrimmed ||
        curTrimmed.startsWith('```') ||
        /^(#{1,4})\s+/.test(curTrimmed) ||
        /^(\s*)[-*]\s+/.test(cur) ||
        /^(\s*)\d+\.\s+/.test(cur)
      ) {
        break;
      }
      paraLines.push(curTrimmed);
      i += 1;
    }

    if (paraLines.length > 0) {
      blocks.push({ type: 'paragraph', text: paraLines.join(' ') });
    }
  }

  return blocks;
}
