import { X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { acceleratorFromEvent, acceleratorHasModifier } from '../lib/router';
import { Button, Kbd } from './ui';

export function HotkeyField({ value, onChange, label }: { value: string | null; onChange: (v: string | null) => void; label: string }) {
  const [listening, setListening] = useState(false);
  const [hint, setHint] = useState<string | null>(null);
  const btn = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!listening) return;
    const down = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === 'Escape') {
        setListening(false);
        setHint(null);
        return;
      }
      const a = acceleratorFromEvent(e);
      if (!a) return;
      if (!acceleratorHasModifier(a)) {
        setHint('Include Ctrl, Alt or Shift so the shortcut doesn’t interfere with your game.');
        return;
      }
      setListening(false);
      setHint(null);
      onChange(a);
      btn.current?.focus();
    };
    window.addEventListener('keydown', down, true);
    return () => window.removeEventListener('keydown', down, true);
  }, [listening, onChange]);
  return (
    <div>
      <div className="row">
        <button ref={btn} type="button" className="btn" style={{ minWidth: '11rem', justifyContent: 'center' }} aria-label={`${label}: ${value ?? 'not set'}. Press to change.`} onClick={() => setListening(true)} onBlur={() => setListening(false)}>
          {listening ? 'Press the keys… (Esc to cancel)' : value ? <Kbd>{value}</Kbd> : <span className="faint">Not set</span>}
        </button>
        {value && !listening && (
          <Button size="sm" variant="ghost" icon aria-label={`Clear ${label}`} title="Remove" onClick={() => onChange(null)}>
            <X size={15} />
          </Button>
        )}
      </div>
      {hint && (
        <div className="field-help" role="status" style={{ color: 'var(--warn)' }}>
          {hint}
        </div>
      )}
    </div>
  );
}
