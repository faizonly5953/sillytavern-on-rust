// Base UI primitives: Button, IconButton, Input, Textarea, Modal, Field, Toasts.

import { useEffect, useState, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode, type TextareaHTMLAttributes } from 'react';
import { X } from 'lucide-react';
import { useApp } from '../lib/store';
import { cn } from '../lib/utils';

type Variant = 'primary' | 'secondary' | 'ghost' | 'danger';

const variants: Record<Variant, string> = {
  primary: 'bg-accent text-accent-fg hover:brightness-110',
  secondary: 'bg-elevated text-fg border border-border hover:bg-border/60',
  ghost: 'text-muted hover:text-fg hover:bg-elevated',
  danger: 'bg-danger/15 text-danger hover:bg-danger/25',
};

export function Button({
  variant = 'secondary',
  className,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  return (
    <button
      type="button"
      className={cn(
        'inline-flex h-9 shrink-0 items-center justify-center gap-1.5 rounded-lg px-3 text-sm font-medium transition disabled:pointer-events-none disabled:opacity-40',
        variants[variant],
        className,
      )}
      {...props}
    />
  );
}

export function IconButton({
  label,
  className,
  active,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { label: string; active?: boolean }) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      className={cn(
        'inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-lg text-muted transition hover:bg-elevated hover:text-fg disabled:pointer-events-none disabled:opacity-40',
        active && 'bg-elevated text-fg',
        className,
      )}
      {...props}
    />
  );
}

export const inputClass =
  'w-full rounded-lg border border-border bg-bg px-3 text-sm text-fg placeholder:text-muted/70 outline-none transition focus:border-accent/70 focus:ring-2 focus:ring-accent/20';

export function Input({ className, ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return <input className={cn(inputClass, 'h-9', className)} {...props} />;
}

export function Textarea({ className, ...props }: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return <textarea className={cn(inputClass, 'min-h-20 resize-y py-2 leading-relaxed', className)} {...props} />;
}

export function Field({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <label className="block space-y-1.5">
      <span className="text-xs font-medium uppercase tracking-wide text-muted">{label}</span>
      {children}
      {hint && <span className="block text-xs text-muted">{hint}</span>}
    </label>
  );
}

export function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className="flex w-full items-center justify-between gap-3 rounded-lg py-1.5 text-left text-sm"
    >
      <span>{label}</span>
      <span className={cn('relative h-5 w-9 shrink-0 rounded-full transition', checked ? 'bg-accent' : 'bg-border')}>
        <span
          className={cn(
            'absolute top-0.5 h-4 w-4 rounded-full bg-white shadow transition-all',
            checked ? 'left-[18px]' : 'left-0.5',
          )}
        />
      </span>
    </button>
  );
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
}) {
  return (
    <div className="flex rounded-lg border border-border bg-bg p-0.5">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          onClick={() => onChange(o.value)}
          className={cn(
            'flex-1 rounded-md px-2 py-1.5 text-xs font-medium transition',
            value === o.value ? 'bg-elevated text-fg shadow-sm' : 'text-muted hover:text-fg',
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Slider({
  label,
  value,
  min,
  max,
  step,
  onChange,
  hint,
  unit,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (v: number) => void;
  hint?: string;
  unit?: string;
}) {
  return (
    <div className="space-y-2">
      <div className="flex items-center gap-3">
        <span className="min-w-0 flex-1 text-sm font-medium">{label}</span>
        <span className="flex shrink-0 items-center gap-1.5 text-xs text-muted">
          <input
            type="number"
            aria-label={label}
            value={value}
            min={min}
            max={max}
            step={step}
            onChange={(e) => {
              const n = Number(e.target.value);
              if (!Number.isNaN(n)) onChange(n);
            }}
            // Not inputClass: its w-full would beat w-24 and stretch the box across the row.
            className="h-8 w-24 rounded-lg border border-border bg-bg px-2.5 text-right text-sm tabular-nums text-fg outline-none transition focus:border-accent/70 focus:ring-2 focus:ring-accent/20"
          />
          {unit}
        </span>
      </div>
      <input
        type="range"
        aria-label={label}
        value={value}
        min={min}
        max={max}
        step={step}
        onChange={(e) => onChange(Number(e.target.value))}
        className="block w-full accent-[var(--accent)]"
      />
      {hint && <p className="text-xs text-muted">{hint}</p>}
    </div>
  );
}

/**
 * Dropdown for header buttons; closes on outside click, Escape, or when `children` calls `close`.
 * Phones: a full-width card under the header (anchoring to the button clipped it off-screen on
 * 360px wide screens). From `sm` up: a card under the button, `width` is its `sm:w-*` class.
 */
export function Popover({
  trigger,
  children,
  width = 'sm:w-56',
}: {
  trigger: (api: { open: boolean; toggle: () => void }) => ReactNode;
  children: (close: () => void) => ReactNode;
  width?: string;
}) {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false);
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open]);
  return (
    <div className="relative">
      {trigger({ open, toggle: () => setOpen((v) => !v) })}
      {open && (
        <>
          <div className="fixed inset-0 z-20" onClick={() => setOpen(false)} />
          <div
            className={cn(
              'animate-pop fixed inset-x-3 top-[calc(env(safe-area-inset-top)+3.75rem)] z-30 rounded-2xl border border-border bg-elevated p-1.5 shadow-xl',
              'sm:absolute sm:inset-x-auto sm:right-0 sm:top-11 sm:origin-top-right',
              width,
            )}
          >
            {children(() => setOpen(false))}
          </div>
        </>
      )}
    </div>
  );
}

export function MenuItem({ className, ...props }: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      className={cn(
        'flex w-full items-center gap-2.5 rounded-xl px-3 py-2.5 text-left text-sm transition hover:bg-border/60',
        className,
      )}
      {...props}
    />
  );
}

/**
 * Centered popup. `required`: no close button, Escape and backdrop clicks are
 * ignored (the popup shakes instead) so the user must pick one of its actions.
 */
export function Modal({
  open,
  onClose,
  title,
  children,
  footer,
  wide,
  required,
}: {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
  wide?: boolean;
  required?: boolean;
}) {
  const [shake, setShake] = useState(false);
  useEffect(() => {
    if (!open || required) return;
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose();
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose, required]);
  if (!open) return null;
  return (
    <div
      className="animate-fade fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-3 backdrop-blur-sm sm:p-6"
      onMouseDown={required ? () => setShake(true) : onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        onMouseDown={(e) => e.stopPropagation()}
        onAnimationEnd={() => setShake(false)}
        className={cn(
          'flex max-h-[88dvh] w-full flex-col overflow-hidden rounded-3xl border border-border bg-surface shadow-2xl shadow-black/40',
          wide ? 'max-w-3xl' : 'max-w-lg',
          shake ? 'animate-shake' : 'animate-pop',
        )}
      >
        <div className="flex items-center gap-2 border-b border-border px-5 py-3.5">
          <h2 className="flex-1 truncate text-base font-semibold">{title}</h2>
          {!required && (
            <IconButton label="Close" onClick={onClose}>
              <X size={18} />
            </IconButton>
          )}
        </div>
        <div className="flex-1 overflow-y-auto px-5 py-4">{children}</div>
        {footer && (
          <div className="flex flex-wrap items-center gap-2 border-t border-border px-5 py-3">
            {footer}
          </div>
        )}
      </div>
    </div>
  );
}

export function Avatar({ src, name, className }: { src: string | null; name: string; className?: string }) {
  if (src) return <img src={src} alt="" loading="lazy" draggable={false} className={cn('shrink-0 bg-elevated object-cover', className)} />;
  return (
    <div className={cn('grid shrink-0 place-items-center bg-elevated text-sm font-semibold text-muted', className)}>
      {name.trim().charAt(0).toUpperCase() || '?'}
    </div>
  );
}

export function Toasts() {
  const toasts = useApp((s) => s.toasts);
  const dismiss = useApp((s) => s.dismissToast);
  return (
    <div className="pointer-events-none fixed inset-x-0 top-[calc(env(safe-area-inset-top)+0.75rem)] z-[60] flex flex-col items-center gap-2 px-3">
      {toasts.map((t) => (
        <button
          key={t.id}
          type="button"
          onClick={() => dismiss(t.id)}
          className={cn(
            'pointer-events-auto max-w-md rounded-xl border px-4 py-2.5 text-left text-sm shadow-lg backdrop-blur',
            t.kind === 'error' ? 'border-danger/40 bg-surface/95 text-danger' : 'border-border bg-surface/95 text-fg',
          )}
        >
          {t.text}
        </button>
      ))}
    </div>
  );
}
