import { useEffect, useRef, useState } from 'react';

export interface PickerOption {
  value: string;
  label: string;
  sub?: string;
}

interface PickerProps {
  value: string;
  options: PickerOption[];
  placeholder?: string;
  searchable?: boolean;
  disabled?: boolean;
  onChange: (value: string) => void;
}

export function Picker({
  value,
  options,
  placeholder = 'Выбрать...',
  searchable = false,
  disabled = false,
  onChange,
}: PickerProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
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

  const current = options.find((o) => o.value === value);
  const filtered = query.trim()
    ? options.filter(
        (o) =>
          o.label.toLowerCase().includes(query.toLowerCase()) ||
          (o.sub && o.sub.toLowerCase().includes(query.toLowerCase())),
      )
    : options;

  return (
    <div className="gw-picker" ref={rootRef}>
      <button
        type="button"
        className="gw-picker-btn"
        disabled={disabled}
        onClick={() => {
          setQuery('');
          setOpen((v) => !v);
        }}
      >
        <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
          {current ? current.label : placeholder}
        </span>
        <span className="gw-muted">▾</span>
      </button>

      {open && (
        <div className="gw-picker-menu">
          {searchable && (
            <input
              className="gw-input"
              autoFocus
              placeholder="Поиск..."
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          )}
          <div className="gw-picker-list">
            {filtered.length === 0 && <div className="gw-muted" style={{ padding: 6 }}>Ничего не найдено</div>}
            {filtered.map((opt) => (
              <button
                key={opt.value}
                type="button"
                className={`gw-picker-item ${opt.value === value ? 'active' : ''}`}
                onClick={() => {
                  onChange(opt.value);
                  setOpen(false);
                }}
              >
                <div style={{ fontWeight: 500 }}>{opt.label}</div>
                {opt.sub && <div className="gw-muted">{opt.sub}</div>}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
