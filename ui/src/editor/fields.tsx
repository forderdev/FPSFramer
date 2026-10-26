import { RotateCcw } from 'lucide-react';
import type { ReactNode } from 'react';
import { Button, ColorField, NumberInput, Segmented, Select, Switch, TextInput } from '../components/ui';

export function Section({ title, children, open = true }: { title: string; children: ReactNode; open?: boolean }) {
  return (
    <details className="ed-section" open={open}>
      <summary>{title}</summary>
      <div className="inner">{children}</div>
    </details>
  );
}

export function Prop({ label, children, help }: { label: string; children: ReactNode; help?: string }) {
  return (
    <div>
      <div className="prop">
        <span className="plabel">{label}</span>
        <div style={{ minWidth: 0 }}>{children}</div>
      </div>
      {help && <div className="field-help" style={{ marginLeft: '7.1rem', marginTop: 2 }}>{help}</div>}
    </div>
  );
}

export function PNum({ label, value, onChange, min, max, step, suffix, help, placeholder }: { label: string; value: number | undefined; onChange: (v: number | undefined) => void; min?: number; max?: number; step?: number; suffix?: string; help?: string; placeholder?: string }) {
  return (
    <Prop label={label} help={help}>
      {value === undefined && placeholder ? (
        <div className="row" style={{ gap: 6 }}>
          <span className="muted">{placeholder}</span>
          <Button size="sm" onClick={() => onChange(min ?? 0)}>
            Set
          </Button>
        </div>
      ) : (
        <NumberInput label={label} small value={value ?? 0} min={min} max={max} step={step} suffix={suffix} onChange={(v) => onChange(v)} />
      )}
    </Prop>
  );
}

export function PText({ label, value, onChange, placeholder, multiline }: { label: string; value: string; onChange: (v: string) => void; placeholder?: string; multiline?: boolean }) {
  return (
    <Prop label={label}>
      {multiline ? <textarea className="textarea" style={{ minHeight: '4rem' }} aria-label={label} value={value} onChange={(e) => onChange(e.target.value)} placeholder={placeholder} /> : <TextInput label={label} small value={value} onChange={onChange} placeholder={placeholder} />}
    </Prop>
  );
}

export function PColor({ label, value, onChange, fallback, resetTo }: { label: string; value: string | undefined; onChange: (v: string | undefined) => void; fallback: string; resetTo?: string }) {
  return (
    <Prop label={label}>
      <div className="row" style={{ gap: 6 }}>
        <ColorField label={label} value={value ?? fallback} onChange={onChange} />
        {resetTo !== undefined && value !== undefined && (
          <Button variant="ghost" size="sm" onClick={() => onChange(undefined)} title="Use the layout default">
            <RotateCcw size={14} /> Default
          </Button>
        )}
      </div>
    </Prop>
  );
}

export function PSelect<T extends string | number>({ label, value, options, onChange, help }: { label: string; value: T; options: { value: T; label: string }[]; onChange: (v: T) => void; help?: string }) {
  return (
    <Prop label={label} help={help}>
      <Select small label={label} value={value} options={options} onChange={onChange} />
    </Prop>
  );
}

export function PSeg<T extends string | number>({ label, value, options, onChange }: { label: string; value: T; options: { value: T; label: ReactNode }[]; onChange: (v: T) => void }) {
  return (
    <Prop label={label}>
      <Segmented label={label} value={value} options={options} onChange={onChange} />
    </Prop>
  );
}

export function PSwitch({ label, value, onChange, help }: { label: string; value: boolean; onChange: (v: boolean) => void; help?: string }) {
  return (
    <Prop label={label} help={help}>
      <Switch label={label} checked={value} onChange={onChange} />
    </Prop>
  );
}
