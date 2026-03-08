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
          ? "border border-[var(--accent)] bg-[#1f6feb]/30"
          : "border border-[#30363d] bg-[#0d1117]"
      }`}
    >
      <span
        className={`absolute left-[1px] top-[1px] h-4 w-4 rounded-full transition-transform duration-300 ${
          checked
            ? "translate-x-5 bg-[var(--accent)] shadow-[0_0_10px_var(--accent)]"
            : "bg-[#8b949e]"
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
      className={`w-full rounded-lg border border-[#30363d] bg-[#0d1117]/50 px-3 py-2 text-sm text-[#c9d1d9] placeholder:text-[#6e7681] transition-all focus:outline-none focus:ring-1 focus:ring-[var(--accent)] ${className ?? ""}`}
    />
  );
}
