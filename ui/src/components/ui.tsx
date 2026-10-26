import { AlertTriangle, CheckCircle2, ChevronDown, Info, OctagonAlert, Search, X, XCircle } from 'lucide-react';
import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState, type ButtonHTMLAttributes, type CSSProperties, type ReactNode } from 'react';
import { create } from 'zustand';
import { STATUS_LABEL } from '../lib/format';
import type { Status } from '../lib/types';

// ───────────────────────────── buttons ─────────────────────────────

interface BtnProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'default' | 'primary' | 'danger' | 'ghost';
  size?: 'md' | 'sm';
  icon?: boolean;
}

export function Button({ variant = 'default', size = 'md', icon, className = '', type = 'button', ...p }: BtnProps) {
  const cls = ['btn', variant !== 'default' ? variant : '', size === 'sm' ? 'sm' : '', icon ? 'icon' : '', className].filter(Boolean).join(' ');
  return <button type={type} className={cls} {...p} />;
}

// ───────────────────────────── tooltips ─────────────────────────────

export function InfoTip({ text, label = 'More information' }: { text: ReactNode; label?: string }) {
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ left: number; top: number }>({ left: 0, top: 0 });
  const ref = useRef<HTMLButtonElement>(null);
  const id = useId();
  const show = useCallback(() => {
    const r = ref.current?.getBoundingClientRect();
    if (!r) return;
    const left = Math.min(Math.max(8, r.left + r.width / 2 - 160), window.innerWidth - 336);
    setPos({ left, top: r.bottom + 8 });
    setOpen(true);
  }, []);
  useEffect(() => {
    if (!open) return;
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false);
    window.addEventListener('keydown', esc);
    return () => window.removeEventListener('keydown', esc);
  }, [open]);
  return (
    <span className="tip">
      <button ref={ref} type="button" className="tip-btn" aria-label={label} aria-describedby={open ? id : undefined} onMouseEnter={show} onMouseLeave={() => setOpen(false)} onFocus={show} onBlur={() => setOpen(false)}>
        <Info size={15} aria-hidden />
      </button>
      {open && (
        <span role="tooltip" id={id} className="tip-pop" style={{ left: pos.left, top: pos.top }}>
          {text}
        </span>
      )}
    </span>
  );
}

/** Hover/focus tooltip around arbitrary content (keeps the child's own semantics). */
export function Tip({ text, children }: { text: ReactNode; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ left: number; top: number }>({ left: 0, top: 0 });
  const ref = useRef<HTMLSpanElement>(null);
  const id = useId();
  const show = () => {
    const r = ref.current?.getBoundingClientRect();
    if (!r) return;
    setPos({ left: Math.min(Math.max(8, r.left), window.innerWidth - 336), top: r.bottom + 6 });
    setOpen(true);
  };
  return (
    <span ref={ref} className="tip" onMouseEnter={show} onMouseLeave={() => setOpen(false)} onFocus={show} onBlur={() => setOpen(false)} aria-describedby={open ? id : undefined}>
      {children}
      {open && (
        <span role="tooltip" id={id} className="tip-pop" style={{ left: pos.left, top: pos.top }}>
          {text}
        </span>
      )}
    </span>
  );
}

// ───────────────────────────── form controls ─────────────────────────────

export function Switch({ checked, onChange, label, disabled, id }: { checked: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean; id?: string }) {
  return <button id={id} type="button" role="switch" aria-checked={checked} aria-label={label} disabled={disabled} className="switch" onClick={() => onChange(!checked)} />;
}

export function Slider({ value, min, max, step = 1, onChange, label, format }: { value: number; min: number; max: number; step?: number; onChange: (v: number) => void; label: string; format?: (v: number) => string }) {
  const fill = ((value - min) / (max - min || 1)) * 100;
  return (
    <div className="row" style={{ width: '100%' }}>
      <input
        type="range"
        className="slider"
        min={min}
        max={max}
        step={step}
        value={value}
        aria-label={label}
        style={{ '--fill': `${fill}%` } as CSSProperties}
        onChange={(e) => onChange(Number(e.target.value))}
      />
      <span className="num muted" style={{ minWidth: '3.2rem', textAlign: 'right', fontSize: '0.8125rem' }}>
        {format ? format(value) : value}
      </span>
    </div>
  );
}

export function Segmented<T extends string | number>({ value, options, onChange, label }: { value: T; options: { value: T; label: ReactNode; title?: string }[]; onChange: (v: T) => void; label: string }) {
  return (
    <div className="seg" role="group" aria-label={label}>
      {options.map((o) => (
        <button key={String(o.value)} type="button" aria-pressed={o.value === value} title={o.title} onClick={() => onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Select<T extends string | number>({ value, options, onChange, label, small, id }: { value: T; options: { value: T; label: string }[]; onChange: (v: T) => void; label: string; small?: boolean; id?: string }) {
  return (
    <select
      id={id}
      className={`select ${small ? 'sm' : ''}`}
      aria-label={label}
      value={String(value)}
      onChange={(e) => {
        const raw = e.target.value;
        const match = options.find((o) => String(o.value) === raw);
        if (match) onChange(match.value);
      }}
    >
      {options.map((o) => (
        <option key={String(o.value)} value={String(o.value)}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

export function TextInput({ value, onChange, placeholder, label, small, maxLength, onEnter, autoFocus }: { value: string; onChange: (v: string) => void; placeholder?: string; label: string; small?: boolean; maxLength?: number; onEnter?: () => void; autoFocus?: boolean }) {
  return (
    <input
      className={`input ${small ? 'sm' : ''}`}
      value={value}
      placeholder={placeholder}
      aria-label={label}
      maxLength={maxLength}
      autoFocus={autoFocus}
      onChange={(e) => onChange(e.target.value)}
      onKeyDown={(e) => e.key === 'Enter' && onEnter?.()}
    />
  );
}

/** Number input that keeps what you type until you leave the field, then clamps. */
export function NumberInput({ value, onChange, min, max, step = 1, label, small, suffix }: { value: number; onChange: (v: number) => void; min?: number; max?: number; step?: number; label: string; small?: boolean; suffix?: string }) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const commit = () => {
    let n = parseFloat(text);
    if (!Number.isFinite(n)) {
      setText(String(value));
      return;
    }
    if (min !== undefined) n = Math.max(min, n);
    if (max !== undefined) n = Math.min(max, n);
    setText(String(n));
    if (n !== value) onChange(n);
  };
  return (
    <div className="row" style={{ gap: '0.4rem' }}>
      <input
        className={`input ${small ? 'sm' : ''}`}
        style={{ width: '6rem' }}
        type="number"
        inputMode="decimal"
        value={text}
        min={min}
        max={max}
        step={step}
        aria-label={label}
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => e.key === 'Enter' && commit()}
      />
      {suffix && <span className="muted">{suffix}</span>}
    </div>
  );
}

export function SearchBox({ value, onChange, placeholder = 'Search…', label = 'Search' }: { value: string; onChange: (v: string) => void; placeholder?: string; label?: string }) {
  return (
    <div style={{ position: 'relative', flex: 1, minWidth: '10rem' }}>
      <Search size={16} aria-hidden style={{ position: 'absolute', left: '0.75rem', top: '50%', transform: 'translateY(-50%)', color: 'var(--text-3)' }} />
      <input className="input" style={{ paddingLeft: '2.2rem' }} type="search" value={value} placeholder={placeholder} aria-label={label} onChange={(e) => onChange(e.target.value)} />
    </div>
  );
}

const SWATCHES = ['#3ddcff', '#4ade80', '#ffb454', '#ff6b9a', '#b79bff', '#ffe14d', '#ffffff'];

export function ColorField({ value, onChange, label, swatches = false }: { value: string; onChange: (v: string) => void; label: string; swatches?: boolean }) {
  const hex = /^#[0-9a-f]{6}$/i.test(value) ? value : value.length === 4 && value.startsWith('#') ? `#${value[1]}${value[1]}${value[2]}${value[2]}${value[3]}${value[3]}` : '#ffffff';
  return (
    <div className="swatches">
      {swatches &&
        SWATCHES.map((c) => (
          <button key={c} type="button" className="swatch" style={{ background: c }} aria-label={`${label}: ${c}`} aria-pressed={value.toLowerCase() === c} onClick={() => onChange(c)} />
        ))}
      <input type="color" className="color-input" value={hex} aria-label={label} onChange={(e) => onChange(e.target.value)} />
    </div>
  );
}

export function Field({ label, help, children, tip }: { label: string; help?: ReactNode; children: ReactNode; tip?: ReactNode }) {
  const id = useId();
  return (
    <div className="field">
      <label className="field-label" htmlFor={id}>
        {label}
        {tip && <InfoTip text={tip} />}
      </label>
      <div id={id}>{children}</div>
      {help && <div className="field-help">{help}</div>}
    </div>
  );
}

/** A labelled settings row: title + description on the left, control on the right. */
export function Setting({ title, desc, children, tip }: { title: string; desc?: ReactNode; children: ReactNode; tip?: ReactNode }) {
  return (
    <div className="setting">
      <div className="grow">
        <div className="setting-title row" style={{ gap: '0.4rem' }}>
          {title}
          {tip && <InfoTip text={tip} />}
        </div>
        {desc && <div className="setting-desc">{desc}</div>}
      </div>
      <div className="ctrl">{children}</div>
    </div>
  );
}

// ───────────────────────────── display ─────────────────────────────

export function Card({ children, className = '', ...p }: { children: ReactNode; className?: string } & React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={`card ${className}`} {...p}>
      {children}
    </div>
  );
}

export function CardHead({ title, sub, right, icon }: { title: ReactNode; sub?: ReactNode; right?: ReactNode; icon?: ReactNode }) {
  return (
    <div className="card-head">
      {icon}
      <div>
        <div className="card-title">{title}</div>
        {sub && <div className="card-sub">{sub}</div>}
      </div>
      <div className="spacer" />
      {right}
    </div>
  );
}

export function PageHead({ title, sub, actions }: { title: ReactNode; sub?: ReactNode; actions?: ReactNode }) {
  return (
    <div className="page-head">
      <div>
        <h1 className="page-title">{title}</h1>
        {sub && <p className="page-sub">{sub}</p>}
      </div>
      <div className="spacer" />
      <div className="row wrap">{actions}</div>
    </div>
  );
}

export function Badge({ children, tone }: { children: ReactNode; tone?: 'ok' | 'warn' | 'bad' | 'info' | 'accent' }) {
  return <span className={`badge ${tone ?? ''}`}>{children}</span>;
}

export function StatusPill({ status }: { status: Status }) {
  const Icon = status === 'normal' ? CheckCircle2 : status === 'elevated' ? AlertTriangle : OctagonAlert;
  return (
    <span className={`pill ${status}`}>
      <Icon size={13} aria-hidden />
      {STATUS_LABEL[status]}
    </span>
  );
}

type BannerTone = 'info' | 'ok' | 'warn' | 'bad';

export function Banner({ tone = 'info', title, children, actions, className = '' }: { tone?: BannerTone; title?: ReactNode; children?: ReactNode; actions?: ReactNode; className?: string }) {
  const Icon = tone === 'ok' ? CheckCircle2 : tone === 'warn' ? AlertTriangle : tone === 'bad' ? OctagonAlert : Info;
  return (
    <div className={`banner ${tone} ${className}`} role={tone === 'bad' ? 'alert' : 'status'}>
      <Icon size={20} aria-hidden />
      <div className="grow">
        {title && <div className="banner-title">{title}</div>}
        {children && <div className={title ? 'muted' : ''}>{children}</div>}
      </div>
      {actions && <div className="row wrap">{actions}</div>}
    </div>
  );
}

export function Empty({ icon, title, children, action }: { icon?: ReactNode; title: string; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className="empty">
      {icon}
      <div className="empty-title">{title}</div>
      {children && <div style={{ maxWidth: '30rem' }}>{children}</div>}
      {action}
    </div>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return <kbd className="kbd">{children}</kbd>;
}

export function Chip({ children, on, onClick, title }: { children: ReactNode; on?: boolean; onClick?: () => void; title?: string }) {
  return (
    <button type="button" className={`chip ${on ? 'on' : ''}`} aria-pressed={on} title={title} onClick={onClick}>
      {children}
    </button>
  );
}

/** Secondary content that stays out of the way until asked for. Closed by default. */
export function Disclosure({ title, sub, children, defaultOpen = false, tone }: { title: ReactNode; sub?: ReactNode; children: ReactNode; defaultOpen?: boolean; tone?: 'danger' }) {
  return (
    <details className={`disclosure ${tone ?? ''}`} open={defaultOpen}>
      <summary>
        <ChevronDown size={16} aria-hidden className="chev" />
        <span className="disc-title">{title}</span>
        {sub && <span className="disc-sub">{sub}</span>}
      </summary>
      <div className="disc-body">{children}</div>
    </details>
  );
}

export function Tabs<T extends string>({ value, tabs, onChange }: { value: T; tabs: { id: T; label: string; icon?: ReactNode }[]; onChange: (v: T) => void }) {
  return (
    <div className="tabs" role="tablist">
      {tabs.map((t) => (
        <button key={t.id} type="button" role="tab" aria-selected={t.id === value} onClick={() => onChange(t.id)}>
          <span className="row" style={{ gap: '0.4rem' }}>
            {t.icon}
            {t.label}
          </span>
        </button>
      ))}
    </div>
  );
}

// ───────────────────────────── modal ─────────────────────────────

const FOCUSABLE = 'a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])';

export function Modal({ title, onClose, children, footer, wide, className = '' }: { title: string; onClose: () => void; children: ReactNode; footer?: ReactNode; wide?: boolean; className?: string }) {
  const ref = useRef<HTMLDivElement>(null);
  const titleId = useId();
  useLayoutEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    const el = ref.current;
    const first = el?.querySelector<HTMLElement>('[data-autofocus]') ?? el?.querySelector<HTMLElement>(FOCUSABLE);
    first?.focus();
    return () => prev?.focus?.();
  }, []);
  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onClose();
    } else if (e.key === 'Tab' && ref.current) {
      const items = Array.from(ref.current.querySelectorAll<HTMLElement>(FOCUSABLE));
      if (!items.length) return;
      const first = items[0];
      const last = items[items.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  };
  return (
    <div className="scrim" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div ref={ref} className={`modal ${wide ? 'wide' : ''} ${className}`} role="dialog" aria-modal="true" aria-labelledby={titleId} onKeyDown={onKey}>
        <div className="modal-head">
          <h2 className="modal-title" id={titleId}>
            {title}
          </h2>
          <div className="spacer" />
          <Button variant="ghost" size="sm" icon aria-label="Close" onClick={onClose}>
            <X size={18} />
          </Button>
        </div>
        <div className="modal-body">{children}</div>
        {footer && <div className="modal-foot">{footer}</div>}
      </div>
    </div>
  );
}

// ───────────────────────────── confirm dialog ─────────────────────────────

interface ConfirmReq {
  title: string;
  body: ReactNode;
  confirmLabel: string;
  danger: boolean;
  resolve: (ok: boolean) => void;
}

const useConfirmStore = create<{ req: ConfirmReq | null }>(() => ({ req: null }));

/** Ask before something disruptive. Resolves to true when the user confirms. */
export function confirmAction(o: { title: string; body: ReactNode; confirmLabel?: string; danger?: boolean }): Promise<boolean> {
  return new Promise((resolve) => {
    useConfirmStore.setState({ req: { title: o.title, body: o.body, confirmLabel: o.confirmLabel ?? 'Continue', danger: o.danger ?? false, resolve } });
  });
}

export function ConfirmHost() {
  const req = useConfirmStore((s) => s.req);
  if (!req) return null;
  const done = (ok: boolean) => {
    req.resolve(ok);
    useConfirmStore.setState({ req: null });
  };
  return (
    <Modal
      title={req.title}
      onClose={() => done(false)}
      footer={
        <>
          <Button onClick={() => done(false)} data-autofocus>
            Cancel
          </Button>
          <Button variant={req.danger ? 'danger' : 'primary'} onClick={() => done(true)}>
            {req.confirmLabel}
          </Button>
        </>
      }
    >
      <div className="muted">{req.body}</div>
    </Modal>
  );
}

// ───────────────────────────── toasts ─────────────────────────────

export function ToastIcon({ level }: { level: 'info' | 'success' | 'warning' | 'error' }) {
  if (level === 'success') return <CheckCircle2 size={20} aria-hidden />;
  if (level === 'error') return <XCircle size={20} aria-hidden />;
  if (level === 'warning') return <AlertTriangle size={20} aria-hidden />;
  return <Info size={20} aria-hidden />;
}

export function Spinner({ label = 'Loading' }: { label?: string }) {
  return <div className="spinner" role="status" aria-label={label} />;
}
