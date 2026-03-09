type CyberToggleProps = {
  checked: boolean;
  onChange: (next: boolean) => void;
  ariaLabel?: string;
};

type CyberInputProps = {
  value: string;
  onChange: (next: string) => void;
  placeholder?: string;
  className?: string;
};

export function CyberToggle({ checked, onChange, ariaLabel }: CyberToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={ariaLabel}
      onClick={() => onChange(!checked)}
      className={`relative h-5 w-10 rounded-full transition-colors duration-300 ${
        checked
          ? "border border-[var(--accent)] bg-[var(--accent-soft)]"
          : "border border-[var(--border-strong)] bg-[var(--bg-surface-2)]"
      }`}
    >
      <span
        className={`absolute left-[1px] top-[1px] h-4 w-4 rounded-full transition-transform duration-300 ${
          checked
            ? "translate-x-5 bg-[var(--accent)] shadow-[0_0_10px_var(--accent)]"
            : "bg-[var(--text-secondary)]"
        }`}
      />
    </button>
  );
}

export function CyberInput({ value, onChange, placeholder, className }: CyberInputProps) {
  return (
    <input
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      className={`w-full rounded-lg border border-[var(--line-soft)] bg-[var(--bg-surface-2)] px-3 py-2 text-sm text-[var(--text-primary)] placeholder:text-[var(--text-muted)] shadow-[var(--float-shadow)] transition-all focus:outline-none focus:ring-1 focus:ring-[var(--line-soft-focus)] focus:shadow-[var(--float-shadow-focus)] ${className ?? ""}`}
    />
  );
}
