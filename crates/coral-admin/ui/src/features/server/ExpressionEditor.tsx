import { useRef } from "react";

import { cx } from "../../lib/cx";
import { highlightExpression, highlightTemplate, type ExprSegment, type ExprSegmentKind } from "./expr";

type ExpressionEditorProps = {
  label: string;
  value: string;
  onChange: (value: string) => void;
  mode: "template" | "condition";
  placeholder?: string;
  error?: string | null;
  suggestions?: readonly string[];
};

const SEGMENT_CLASSES: Record<ExprSegmentKind, string> = {
  literal: "text-fg",
  brace: "text-accent",
  field: "text-info",
  string: "text-ok",
  number: "text-warning",
  keyword: "text-upstream",
  operator: "text-subtle",
};

export function ExpressionEditor({ label, value, onChange, mode, placeholder = "", error = null, suggestions = [] }: ExpressionEditorProps) {
  const areaRef = useRef<HTMLTextAreaElement>(null);
  const segments: ExprSegment[] = mode === "template" ? highlightTemplate(value) : highlightExpression(value);

  const insertAtCursor = (text: string) => {
    const area = areaRef.current;
    if (area === null) {
      onChange(value + text);
      return;
    }
    const { selectionStart, selectionEnd } = area;
    onChange(value.slice(0, selectionStart) + text + value.slice(selectionEnd));
    requestAnimationFrame(() => {
      area.focus();
      area.setSelectionRange(selectionStart + text.length, selectionStart + text.length);
    });
  };

  return (
    <div className="flex flex-col gap-1.5">
      <div className={cx("relative rounded-md border bg-bg font-mono text-12 leading-5", error === null ? "border-border-strong focus-within:border-accent" : "border-danger/50")}>
        <pre aria-hidden className="pointer-events-none m-0 min-h-9 px-3 py-2 break-words whitespace-pre-wrap">
          {value.length === 0 && <span className="text-subtle">{placeholder}</span>}
          {segments.map((segment, index) => (
            <span key={index} className={SEGMENT_CLASSES[segment.kind]}>
              {segment.text}
            </span>
          ))}
          {"\n"}
        </pre>
        <textarea
          ref={areaRef}
          aria-label={label}
          aria-invalid={error === null ? undefined : true}
          value={value}
          spellCheck={false}
          onChange={(event) => onChange(event.target.value)}
          className="absolute inset-0 h-full w-full resize-none bg-transparent px-3 py-2 font-mono text-12 leading-5 break-words whitespace-pre-wrap text-transparent caret-fg outline-none"
        />
      </div>
      {error !== null && <span className="text-11 text-danger">{error}</span>}
      {suggestions.length > 0 && (
        <div className="flex flex-wrap items-center gap-1">
          <span className="text-11 text-subtle">Insert field:</span>
          {suggestions.map((path) => (
            <button key={path} type="button" onClick={() => insertAtCursor(path)} className="rounded bg-surface-2 px-1.5 py-0.5 font-mono text-11 text-info hover:bg-overlay">
              {path}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export function HighlightedCode({ text, mode = "condition" }: { text: string; mode?: "template" | "condition" }) {
  const segments = mode === "template" ? highlightTemplate(text) : highlightExpression(text);
  return (
    <code className="rounded bg-bg px-1.5 py-0.5 font-mono text-12 wrap-anywhere">
      {segments.map((segment, index) => (
        <span key={index} className={SEGMENT_CLASSES[segment.kind]}>
          {segment.text}
        </span>
      ))}
    </code>
  );
}
