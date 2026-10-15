import { useId, type InputHTMLAttributes, type ReactNode, type TextareaHTMLAttributes } from "react";

import { cx } from "../lib/cx";
import { CONTROL, FIELD_LABEL } from "./control-styles";

type FieldProps = {
  label: ReactNode;
  htmlFor: string;
  hint?: ReactNode;
  error?: ReactNode;
  className?: string;
  children: ReactNode;
};

type InputProps = InputHTMLAttributes<HTMLInputElement> & {
  label?: ReactNode;
  hint?: ReactNode;
  error?: ReactNode;
  mono?: boolean;
};

type TextareaProps = TextareaHTMLAttributes<HTMLTextAreaElement> & {
  label?: ReactNode;
  hint?: ReactNode;
  error?: ReactNode;
  mono?: boolean;
};

export function Field({ label, htmlFor, hint, error, className, children }: FieldProps) {
  return (
    <div className={cx("flex min-w-0 flex-col gap-1", className)}>
      <label htmlFor={htmlFor} className={FIELD_LABEL}>
        {label}
      </label>
      {children}
      {hint !== undefined && <span className="text-11 text-subtle">{hint}</span>}
      {error !== undefined && error !== null && <span className="text-11 text-danger">{error}</span>}
    </div>
  );
}

export function Input({ label, hint, error, mono = false, className, id, ...rest }: InputProps) {
  const generatedId = useId();
  const inputId = id ?? generatedId;
  const input = (
    <input
      id={inputId}
      autoComplete="off"
      spellCheck={false}
      aria-invalid={error ? true : undefined}
      className={cx(CONTROL, mono && "font-mono", className)}
      {...rest}
    />
  );
  if (label === undefined) return input;
  return (
    <Field label={label} htmlFor={inputId} hint={hint} error={error}>
      {input}
    </Field>
  );
}

export function Textarea({ label, hint, error, mono = false, className, id, rows = 3, ...rest }: TextareaProps) {
  const generatedId = useId();
  const areaId = id ?? generatedId;
  const area = (
    <textarea
      id={areaId}
      rows={rows}
      aria-invalid={error ? true : undefined}
      className={cx(CONTROL, "h-auto resize-y py-2 leading-normal", mono && "font-mono", className)}
      {...rest}
    />
  );
  if (label === undefined) return area;
  return (
    <Field label={label} htmlFor={areaId} hint={hint} error={error}>
      {area}
    </Field>
  );
}
