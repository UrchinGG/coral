import { useLayoutEffect, useRef, useState, type ReactNode } from "react";

import { cx } from "../lib/cx";

export function ClampText({ children, className }: { children: ReactNode; className?: string }) {
  const ref = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [overflows, setOverflows] = useState(false);

  useLayoutEffect(() => {
    const element = ref.current;
    if (element === null) return;
    const measure = () => setOverflows(element.scrollHeight > element.clientHeight + 1);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [children]);

  return (
    <div className={cx("min-w-0", className)}>
      <div ref={ref} className={cx("wrap-break-word", !open && "line-clamp-2")}>
        {children}
      </div>
      {(overflows || open) && (
        <button type="button" onClick={() => setOpen(!open)} className="p-0 text-11 text-accent">
          {open ? "less" : "more"}
        </button>
      )}
    </div>
  );
}
