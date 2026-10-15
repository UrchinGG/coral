import type { ReactNode } from "react";

import { cx } from "../lib/cx";
import { useTimeFormat } from "../lib/time";
import { TONE_DOT, type Tone } from "./tone";

export type TimelineItem = {
  id: string;
  at: string;
  tone?: Tone;
  title: ReactNode;
  detail?: ReactNode;
};

export function Timeline({ items }: { items: readonly TimelineItem[] }) {
  const { time } = useTimeFormat();
  return (
    <ol className="m-0 flex list-none flex-col p-0 py-1">
      {items.map((item) => (
        <li
          key={item.id}
          className="grid grid-cols-[112px_14px_minmax(0,1fr)] gap-2.5 px-3.5 py-[7px] text-12 max-md:grid-cols-[14px_minmax(0,1fr)]"
        >
          <time dateTime={item.at} className="pt-px font-mono text-11 text-subtle max-md:order-first max-md:col-start-2 max-md:pt-0">
            {time(item.at, { seconds: false, date: "always" })}
          </time>
          <span
            aria-hidden
            className={cx("mt-1 size-[9px] rounded-full shadow-[0_0_0_3px_var(--color-surface)] max-md:row-span-2", TONE_DOT[item.tone ?? "neutral"])}
          />
          <div className="leading-normal">
            {item.title}
            {item.detail !== undefined && <span className="block text-muted">{item.detail}</span>}
          </div>
        </li>
      ))}
    </ol>
  );
}
