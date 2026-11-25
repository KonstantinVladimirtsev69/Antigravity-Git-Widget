import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '..', '..');

function readRel(relPath: string): string {
  return fs.readFileSync(path.join(ROOT, relPath), 'utf8');
}

const CYRILLIC_RE = /[\u0400-\u04FF]/;

test('UI components and T3 bridge tooltips/toasts contain no Cyrillic text', () => {
  const uiFiles = [
    'ui/src/components/CloneModal.tsx',
    'ui/src/components/CloneProgress.tsx',
    'ui/src/App.tsx',
  ];
  for (const rel of uiFiles) {
    const src = readRel(rel);
    assert.equal(CYRILLIC_RE.test(src), false, `Expected English-only UI text in ${rel}`);
  }

  const uiBridge = readRel('crates/git-widget-sidecar/assets/t3-ui-bridge.js');
  const uiBridgeStrings = [
    ...uiBridge.matchAll(/\.(?:title|textContent|innerHTML)\s*=\s*(['"`])(.*?)\1/g),
  ].map((m) => m[2]);
  assert.ok(uiBridgeStrings.length >= 3, 'Expected UI string assignments in t3-ui-bridge.js');
  for (const msg of uiBridgeStrings) {
    assert.equal(CYRILLIC_RE.test(msg), false, `Expected English UI bridge string, found: ${msg}`);
  }

  const projectBridge = readRel('crates/git-widget-sidecar/assets/t3-project-bridge.js');
  const toasts = [...projectBridge.matchAll(/showToast\((['"`])(.*?)\1\)/g)].map((m) => m[2]);
  assert.ok(toasts.length >= 6, 'Expected showToast calls in t3-project-bridge.js');
  for (const msg of toasts) {
    assert.equal(CYRILLIC_RE.test(msg), false, `Expected English toast message, found: ${msg}`);
  }
});

test('CloneModal explains auto-filled folder name in English', () => {
  const modal = readRel('ui/src/components/CloneModal.tsx');
  assert.match(modal, /Folder name \(auto-filled from URL\)/);
  assert.match(modal, /Browse/);
  assert.match(modal, /Open in Explorer/);
  assert.match(modal, /Add to T3 Code/);
});

test('folder_picker avoids invisible WS_EX_TOPMOST STATIC owner', () => {
  const picker = readRel('crates/git-widget-sidecar/src/folder_picker.rs');
  assert.equal(
    picker.includes('WS_EX_TOPMOST | WS_EX_TOOLWINDOW'),
    false,
    'folder_picker.rs must not create an invisible WS_EX_TOPMOST toolwindow owner',
  );
  assert.match(picker, /Show\(None\)/);
});
