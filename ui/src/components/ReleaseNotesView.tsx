import { Fragment, type ReactNode } from 'react';
import { parseInlineMarkdown, parseReleaseNotes } from '../utils/releaseNotes';

function renderInline(text: string, allowBold = true): ReactNode[] {
  return parseInlineMarkdown(text).map((tok, idx) => {
    switch (tok.type) {
      case 'bold':
        return allowBold ? (
          <strong key={idx} style={{ color: 'var(--gw-text, #f3f4f6)', fontWeight: 600 }}>
            {renderInline(tok.value, false)}
          </strong>
        ) : (
          <Fragment key={idx}>{tok.value}</Fragment>
        );
      case 'code':
        return (
          <code
            key={idx}
            style={{
              fontFamily: 'var(--gw-mono, ui-monospace, monospace)',
              fontSize: '0.92em',
              padding: '1px 4px',
              borderRadius: 4,
              background: 'rgba(255, 255, 255, 0.08)',
              color: '#93c5fd',
            }}
          >
            {tok.value}
          </code>
        );
      case 'link':
        return (
          <a
            key={idx}
            href={tok.href}
            target="_blank"
            rel="noreferrer"
            style={{ color: '#60a5fa', textDecoration: 'underline' }}
          >
            {tok.label}
          </a>
        );
      case 'text':
      default:
        return <Fragment key={idx}>{tok.value}</Fragment>;
    }
  });
}

export function ReleaseNotesView({ markdown }: { markdown: string }) {
  const blocks = parseReleaseNotes(markdown);

  if (blocks.length === 0) {
    return <span className="gw-muted">No release notes provided.</span>;
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 6, lineHeight: 1.45 }}>
      {blocks.map((block, idx) => {
        if (block.type === 'heading') {
          const fontSize = block.level <= 2 ? 12.5 : 11.5;
          return (
            <div
              key={idx}
              style={{
                fontWeight: 600,
                fontSize,
                color: block.level <= 2 ? '#e5e7eb' : '#93c5fd',
                marginTop: idx === 0 ? 0 : 4,
              }}
            >
              {renderInline(block.text)}
            </div>
          );
        }

        if (block.type === 'code') {
          return (
            <pre
              key={idx}
              style={{
                margin: 0,
                padding: '6px 8px',
                borderRadius: 4,
                background: 'rgba(0, 0, 0, 0.35)',
                fontFamily: 'var(--gw-mono, ui-monospace, monospace)',
                fontSize: 11,
                overflowX: 'auto',
              }}
            >
              {block.text}
            </pre>
          );
        }

        if (block.type === 'list') {
          const Tag = block.ordered ? 'ol' : 'ul';
          return (
            <Tag
              key={idx}
              style={{
                margin: 0,
                paddingLeft: 16,
                display: 'flex',
                flexDirection: 'column',
                gap: 3,
              }}
            >
              {block.items.map((item, itemIdx) => (
                <li
                  key={itemIdx}
                  style={{
                    marginLeft: item.indent * 12,
                    listStyleType: block.ordered ? 'decimal' : item.indent > 0 ? 'circle' : 'disc',
                  }}
                >
                  {renderInline(item.text)}
                </li>
              ))}
            </Tag>
          );
        }

        return (
          <p key={idx} style={{ margin: 0 }}>
            {renderInline(block.text)}
          </p>
        );
      })}
    </div>
  );
}
