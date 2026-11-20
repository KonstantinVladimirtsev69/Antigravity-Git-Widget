import { useEffect, useRef, useState } from 'react';
import type { IntegrationMode } from './types';

export interface IntegrationModeOption {
  value: IntegrationMode;
  label: string;
  shortLabel: string;
  desc: string;
}

export const INTEGRATION_MODES: IntegrationModeOption[] = [
  {
    value: 'merge',
    label: 'Merge',
    shortLabel: 'Merge',
    desc: 'Standard git merge (FF if possible, otherwise merge commit)',
  },
  {
    value: 'merge_no_ff',
    label: 'Merge (--no-ff)',
    shortLabel: 'No-FF',
    desc: 'Always create a merge commit preserving branch history',
  },
  {
    value: 'squash',
    label: 'Squash Merge',
    shortLabel: 'Squash',
    desc: 'Combine all source commits into one commit',
  },
  {
    value: 'rebase',
    label: 'Rebase',
    shortLabel: 'Rebase',
    desc: 'Replay target commits linearly on top of source',
  },
  {
    value: 'ff_only',
    label: 'Fast-Forward Only',
    shortLabel: 'FF Only',
    desc: 'Merge only if linear fast-forward is possible',
  },
];

export function getModeLabel(mode: IntegrationMode): string {
  return INTEGRATION_MODES.find((m) => m.value === mode)?.label ?? 'Merge';
}

interface IntegrateSplitButtonProps {
  mode: IntegrationMode;
  disabled: boolean;
  hasUnpulled?: boolean;
  onModeChange: (mode: IntegrationMode) => void;
  onExecute: () => void;
}

export function IntegrateSplitButton({
  mode,
  disabled,
  hasUnpulled = false,
  onModeChange,
  onExecute,
}: IntegrateSplitButtonProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onDocClick = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', onDocClick);
    return () => document.removeEventListener('mousedown', onDocClick);
  }, []);

  const current = INTEGRATION_MODES.find((m) => m.value === mode) ?? INTEGRATION_MODES[0];

  return (
    <div className={`gw-split ${open ? 'open' : ''}`} ref={rootRef}>
      <button
        type="button"
        className={`gw-btn gw-split-main ${hasUnpulled ? 'gw-btn-warn-outline' : ''}`}
        disabled={disabled}
        title={current.desc}
        onClick={onExecute}
      >
        {current.shortLabel}
      </button>
      <button
        type="button"
        className={`gw-btn gw-split-arrow ${hasUnpulled ? 'gw-btn-warn-outline' : ''}`}
        disabled={disabled}
        title="Choose integration mode"
        aria-label="Choose integration mode"
        onClick={() => setOpen((v) => !v)}
      >
        ▾
      </button>
      {open && (
        <div className="gw-split-menu">
          {INTEGRATION_MODES.map((opt) => (
            <button
              key={opt.value}
              type="button"
              className={`gw-picker-item ${opt.value === mode ? 'active' : ''}`}
              onClick={() => {
                onModeChange(opt.value);
                setOpen(false);
              }}
            >
              <div style={{ fontWeight: 500 }}>{opt.label}</div>
              <div className="gw-muted" style={{ fontSize: 10 }}>
                {opt.desc}
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
