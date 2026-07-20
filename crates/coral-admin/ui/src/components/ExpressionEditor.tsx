import { useRef } from "react";
import { highlightExpression, highlightTemplate, type ExprSegment, type ExprSegmentKind } from "../expr";

type ExpressionEditorProps = {
  value: string;
  onChange: (value: string) => void;
  mode: "template" | "condition";
  placeholder?: string;
  error?: string | null;
  suggestions?: string[];
};

const SEGMENT_CLASSES: Record<ExprSegmentKind, string> = {
  literal: "text-gray-300",
  brace: "text-accent",
  field: "text-sky-300",
  string: "text-emerald-300",
  number: "text-amber-300",
  keyword: "text-fuchsia-400",
  operator: "text-gray-500",
};

export function ExpressionEditor({
  value,
  onChange,
  mode,
  placeholder = "",
  error = null,
  suggestions = [],
}: ExpressionEditorProps) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const segments: ExprSegment[] =
    mode === "template" ? highlightTemplate(value) : highlightExpression(value);

  const insertAtCursor = (text: string) => {
    const el = textareaRef.current;
    if (!el) {
      onChange(value + text);
      return;
    }
    const start = el.selectionStart;
    const end = el.selectionEnd;
    onChange(value.slice(0, start) + text + value.slice(end));
    requestAnimationFrame(() => {
      el.focus();
      el.setSelectionRange(start + text.length, start + text.length);
    });
  };

  return (
    <div>
      <div
        className={`relative rounded-md border bg-black/30 font-mono text-xs leading-5 ${
          error ? "border-danger/40" : "border-white/10 focus-within:border-accent/40"
        }`}
      >
        <pre aria-hidden className="pointer-events-none m-0 min-h-[2.25rem] px-3 py-2 break-words whitespace-pre-wrap">
          {value.length === 0 && <span className="text-gray-600">{placeholder}</span>}
          {segments.map((segment, i) => (
            <span key={i} className={SEGMENT_CLASSES[segment.kind]}>
              {segment.text}
            </span>
          ))}
          {"\n"}
        </pre>
        <textarea
          ref={textareaRef}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          spellCheck={false}
          className="absolute inset-0 h-full w-full resize-none bg-transparent px-3 py-2 font-mono text-xs leading-5 break-words whitespace-pre-wrap text-transparent caret-gray-200 outline-none"
        />
      </div>
      {error && <div className="mt-1 text-xs text-danger">{error}</div>}
      {suggestions.length > 0 && (
        <div className="mt-2 flex flex-wrap items-center gap-1">
          <span className="text-[10px] text-gray-500">Insert field:</span>
          {suggestions.map((path) => (
            <button
              key={path}
              onClick={() => insertAtCursor(path)}
              className="rounded bg-white/5 px-1.5 py-0.5 font-mono text-[10px] text-sky-300 hover:bg-white/10"
            >
              {path}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
