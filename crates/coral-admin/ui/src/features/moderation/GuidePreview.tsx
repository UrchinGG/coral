import type { ReactNode } from "react";

const DIVIDER = /^-{3,}$/;
const SMALL_PREFIX = "-# ";
const INLINE_MARKUP = /(\*\*[^*]+\*\*|`[^`]+`)/g;
const BOLD_MARK = "**";
const CODE_MARK = "`";

export function GuidePreview({ body, button }: { body: string; button: string }) {
  return (
    <div className="flex flex-col gap-0.5 rounded-md border border-border bg-bg p-4">
      {body.split("\n").map((line, index) => (
        <PreviewLine key={index} line={line} />
      ))}
      <span className="mt-2 w-fit cursor-not-allowed rounded-md border border-border-strong bg-surface-2 px-3 py-1.5 text-12 text-muted">{button}</span>
    </div>
  );
}

function PreviewLine({ line }: { line: string }) {
  if (DIVIDER.test(line.trim())) return <hr className="my-2 border-border" />;
  if (line.startsWith(SMALL_PREFIX)) return <div className="text-12 text-subtle">{inline(line.slice(SMALL_PREFIX.length))}</div>;
  return <div className="min-h-5 text-13 wrap-anywhere">{inline(line)}</div>;
}

function inline(text: string): ReactNode[] {
  return text.split(INLINE_MARKUP).map((part, index) => {
    if (part.startsWith(BOLD_MARK) && part.endsWith(BOLD_MARK) && part.length > BOLD_MARK.length * 2) {
      return (
        <strong key={index} className="font-semibold">
          {part.slice(BOLD_MARK.length, -BOLD_MARK.length)}
        </strong>
      );
    }
    if (part.startsWith(CODE_MARK) && part.endsWith(CODE_MARK) && part.length > 1) {
      return (
        <code key={index} className="rounded bg-surface-2 px-1 font-mono text-[0.92em]">
          {part.slice(1, -1)}
        </code>
      );
    }
    return <span key={index}>{part}</span>;
  });
}
