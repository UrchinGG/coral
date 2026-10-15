import { useTimeFormat } from "../../lib/time";

type WhenProps = { at: string | null; empty?: string; date?: "auto" | "always" };

export function When({ at, empty = "never", date = "always" }: WhenProps) {
  const { time, relative } = useTimeFormat();
  if (at === null) return <span className="text-subtle">{empty}</span>;
  return (
    <time dateTime={at} title={`${time(at, { millis: true, zone: true, date: "always" })} · ${relative(at)}`} className="font-mono text-12 whitespace-nowrap">
      {time(at, { seconds: false, date })}
    </time>
  );
}
