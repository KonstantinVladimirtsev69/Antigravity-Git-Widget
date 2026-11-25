import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import { parseInlineMarkdown, parseReleaseNotes } from '../src/utils/releaseNotes.ts';

describe('parseReleaseNotes', () => {
  it('returns empty array for blank input', () => {
    assert.deepEqual(parseReleaseNotes('   \n\n  '), []);
  });

  it('parses headings, nested bullet lists, and paragraphs from Keep a Changelog format', () => {
    const md = [
      '## [0.3.1] - 2026-11-25',
      '',
      '### Fixed',
      '- **Commit Graph (`get_commit_graph`)**: Replaced `git log --all` with `--branches --remotes --tags HEAD`.',
      '  - Excludes `refs/t3/*` internal checkpoints.',
      '',
      'See [Release v0.3.1](https://github.com/acme/repo/releases/tag/v0.3.1) for details.',
    ].join('\n');

    const blocks = parseReleaseNotes(md);
    assert.equal(blocks.length, 4);

    assert.deepEqual(blocks[0], {
      type: 'heading',
      level: 2,
      text: '[0.3.1] - 2026-11-25',
    });
    assert.deepEqual(blocks[1], {
      type: 'heading',
      level: 3,
      text: 'Fixed',
    });
    assert.deepEqual(blocks[2], {
      type: 'list',
      ordered: false,
      items: [
        {
          indent: 0,
          text: '**Commit Graph (`get_commit_graph`)**: Replaced `git log --all` with `--branches --remotes --tags HEAD`.',
        },
        {
          indent: 1,
          text: 'Excludes `refs/t3/*` internal checkpoints.',
        },
      ],
    });
    assert.deepEqual(blocks[3], {
      type: 'paragraph',
      text: 'See [Release v0.3.1](https://github.com/acme/repo/releases/tag/v0.3.1) for details.',
    });
  });

  it('parses ordered lists and fenced code blocks', () => {
    const md = [
      '1. First step',
      '2. Second step',
      '',
      '```bash',
      'cargo test --workspace',
      '```',
    ].join('\n');

    const blocks = parseReleaseNotes(md);
    assert.deepEqual(blocks, [
      {
        type: 'list',
        ordered: true,
        items: [
          { indent: 0, text: 'First step' },
          { indent: 0, text: 'Second step' },
        ],
      },
      {
        type: 'code',
        text: 'cargo test --workspace',
      },
    ]);
  });
});

describe('parseInlineMarkdown', () => {
  it('parses bold, inline code, markdown links, and bare https urls safely', () => {
    const tokens = parseInlineMarkdown(
      '**Fix**: use `--branches` via [PR #4](https://github.com/a/b/pull/4) or https://example.com/docs and ignore [bad](javascript:alert(1)).'
    );

    assert.deepEqual(tokens, [
      { type: 'bold', value: 'Fix' },
      { type: 'text', value: ': use ' },
      { type: 'code', value: '--branches' },
      { type: 'text', value: ' via ' },
      { type: 'link', label: 'PR #4', href: 'https://github.com/a/b/pull/4' },
      { type: 'text', value: ' or ' },
      { type: 'link', label: 'https://example.com/docs', href: 'https://example.com/docs' },
      { type: 'text', value: ' and ignore ' },
      { type: 'text', value: 'bad' },
      { type: 'text', value: '.' },
    ]);
  });
});
