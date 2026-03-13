import { ReactNode } from "react";
import type { Language } from "../../i18n";
import { useI18n } from "../../i18n";
import { AnimatedPanel, AnimatedPressButton } from "../MotionKit";
import { CyberInput } from "../UI";

export function LanguageSwitch({
  value,
  onChange
}: {
  value: Language;
  onChange: (lang: Language) => void;
}) {
  return (
    <div className="inline-flex items-center gap-2 rounded-lg bg-transparent p-1">
      <AnimatedPressButton
        type="button"
        onClick={() => onChange("zh-CN")}
        className={`rounded-md px-2.5 py-1 text-sm transition ${
          value === "zh-CN"
            ? "bg-transparent text-[var(--accent)]"
            : "text-[var(--text-secondary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]"
        }`}
      >
        CN
      </AnimatedPressButton>
      <AnimatedPressButton
        type="button"
        onClick={() => onChange("en-US")}
        className={`rounded-md px-2.5 py-1 text-sm transition ${
          value === "en-US"
            ? "bg-transparent text-[var(--accent)]"
            : "text-[var(--text-secondary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]"
        }`}
      >
        EN
      </AnimatedPressButton>
    </div>
  );
}

export function SelectionChips<T extends string>({
  value,
  onChange,
  options
}: {
  value: T;
  onChange: (value: T) => void;
  options: Array<{ value: T; label: string }>;
}) {
  return (
    <div className="inline-flex flex-wrap items-center gap-2">
      {options.map((option) => (
        <AnimatedPressButton
          key={option.value}
          type="button"
          onClick={() => onChange(option.value)}
          className={`rounded-md px-3 py-1.5 text-sm transition ${
            value === option.value
              ? "bg-transparent text-[var(--accent)]"
              : "bg-transparent text-[var(--text-secondary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]"
          }`}
        >
          {option.label}
        </AnimatedPressButton>
      ))}
    </div>
  );
}

export function SettingCard({
  title,
  description,
  children
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <AnimatedPanel className="glass-panel-infer flex items-center justify-between gap-4 rounded-lg px-3 py-3">
      <div className="min-w-0">
        <div className="text-sm text-[var(--text-primary)]">{title}</div>
        {description ? <div className="mt-1 text-xs text-[var(--text-secondary)]">{description}</div> : null}
      </div>
      <div className="shrink-0">{children}</div>
    </AnimatedPanel>
  );
}

const CUSTOM_VALUE = "__custom__";

export function ModelRoleSelector({
  label,
  value,
  options,
  customMode,
  onToggleCustom,
  onChange
}: {
  label: string;
  value: string;
  options: string[];
  customMode: boolean;
  onToggleCustom: () => void;
  onChange: (value: string) => void;
}) {
  const { t } = useI18n();
  const hasValue = options.includes(value);
  const selectValue = customMode || !hasValue ? CUSTOM_VALUE : value;
  return (
    <AnimatedPanel className="glass-panel-infer space-y-2 rounded-lg px-3 py-3">
      <div className="text-sm text-[var(--text-primary)]">{label}</div>
      <div className="flex items-center gap-2">
        <select
          value={selectValue}
          onChange={(event) => {
            const next = event.target.value;
            if (next === CUSTOM_VALUE) {
              onToggleCustom();
            } else {
              onChange(next);
            }
          }}
          className="h-9 min-w-0 flex-1 rounded-lg border border-transparent bg-transparent px-2 text-sm text-[var(--text-primary)] outline-none transition hover:bg-[var(--accent-soft)] focus:bg-[var(--accent-soft)] focus:ring-1 focus:ring-[var(--line-soft-focus)]"
        >
          {options.map((item) => (
            <option key={item} value={item}>
              {item}
            </option>
          ))}
          <option value={CUSTOM_VALUE}>{t("modelUseCustom")}</option>
        </select>
        <AnimatedPressButton
          type="button"
          onClick={onToggleCustom}
          className={`rounded-md px-3 py-1.5 text-sm transition ${
            customMode
              ? "bg-[var(--accent-soft)] text-[var(--accent)]"
              : "bg-transparent text-[var(--text-secondary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]"
          }`}
        >
          {t("modelUseCustom")}
        </AnimatedPressButton>
      </div>
      {customMode ? (
        <CyberInput value={value} onChange={onChange} placeholder={t("modelCustomPlaceholder")} />
      ) : null}
    </AnimatedPanel>
  );
}
