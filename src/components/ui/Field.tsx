import { useId, type ReactNode } from 'react';

export function Field({
  label,
  hint,
  aside,
  error,
  children,
  className = '',
}: {
  label?: ReactNode;
  hint?: ReactNode;
  aside?: ReactNode;
  error?: string | null;
  children: ReactNode | ((id: string) => ReactNode);
  className?: string;
}) {
  const id = useId();
  return (
    <fieldset className={`fieldset min-w-0 ${className}`}>
      {(label || aside) && (
        <legend className="fieldset-legend w-full min-w-0">
          <span className="min-w-0 break-words">{label}</span>
          {aside ? <span className="ml-auto text-xs opacity-70">{aside}</span> : null}
        </legend>
      )}
      {typeof children === 'function' ? children(id) : children}
      {hint && !error ? <p className="min-w-0 break-words text-xs leading-relaxed text-base-content/60">{hint}</p> : null}
      {error ? (
        <p role="alert" className="min-w-0 break-words text-xs leading-relaxed text-error">
          {error}
        </p>
      ) : null}
    </fieldset>
  );
}

export function Toggle({
  label,
  description,
  checked,
  onChange,
  disabled,
}: {
  label: string;
  description?: string;
  checked: boolean;
  onChange: (value: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <label className="flex items-start justify-between gap-4 rounded-field px-3 py-2.5 hover:bg-base-200 transition-colors cursor-pointer">
      <span className="min-w-0">
        <span className="block text-sm font-medium">{label}</span>
        {description ? (
          <span className="block text-xs text-base-content/60 mt-0.5">{description}</span>
        ) : null}
      </span>
      <input
        type="checkbox"
        className="toggle toggle-primary shrink-0"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
    </label>
  );
}
