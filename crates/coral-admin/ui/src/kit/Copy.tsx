import { Check, Copy } from "lucide-react";
import { lazy, Suspense, useEffect, useState, type ReactNode } from "react";

import { copyText } from "../lib/clipboard";
import { cx } from "../lib/cx";
import { Button } from "./Button";

type CopyButtonProps = {
  text: string | (() => string);
  label?: string;
  children?: ReactNode;
  className?: string;
};

type CopyFieldProps = {
  value: string;
  label?: string;
  secret?: boolean;
  children?: ReactNode;
};

const COPIED_MS = 1_400;

const ManualCopyDialog = lazy(() => import("./ManualCopyDialog").then((module) => ({ default: module.ManualCopyDialog })));

export function CopyButton({ text, label = "Copy", children, className }: CopyButtonProps) {
  const [copied, setCopied] = useState(false);
  const [manualText, setManualText] = useState<string | null>(null);

  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), COPIED_MS);
    return () => window.clearTimeout(timer);
  }, [copied]);

  const copy = async () => {
    const value = typeof text === "function" ? text() : text;
    const outcome = await copyText(value);
    if (outcome === "copied") setCopied(true);
    else setManualText(value);
  };

  const icon = copied ? <Check className="size-3" /> : <Copy className="size-3" />;
  return (
    <>
      {children === undefined ? (
        <button
          type="button"
          aria-label={label}
          title={label}
          onClick={copy}
          className={cx(
            "inline-flex size-[22px] shrink-0 items-center justify-center rounded border border-transparent align-middle transition-colors hover:border-border hover:bg-surface-2 hover:text-fg",
            copied ? "text-ok" : "text-subtle",
            className,
          )}
        >
          {icon}
        </button>
      ) : (
        <Button size="sm" onClick={copy} icon={icon} className={cx(copied && "text-ok", className)}>
          {copied ? "Copied" : children}
        </Button>
      )}
      <Suspense>{manualText !== null && <ManualCopyDialog text={manualText} onClose={() => setManualText(null)} />}</Suspense>
    </>
  );
}

export function CopyField({ value, label, secret = false, children }: CopyFieldProps) {
  return (
    <span className="flex min-w-0 items-start gap-1">
      <span className={cx("min-w-0 pt-0.5 font-mono text-12 break-all", secret && "text-entity-key")}>{children ?? value}</span>
      <CopyButton text={value} label={label ?? "Copy"} />
    </span>
  );
}
