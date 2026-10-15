import { CalendarRange, X } from "lucide-react";
import { useState } from "react";

import { fromInputValue, toInputValue, useTimeFormat, zoneLabel, type TimeZoneMode } from "../lib/time";
import { Button, IconButton } from "./Button";
import { Input } from "./Input";
import { Popover } from "./Popover";
import { Segmented } from "./Segmented";

export type TimeRange<W extends string> = { kind: "window"; window: W } | { kind: "custom"; from: string; to: string };

type TimeRangePickerProps<W extends string> = {
  label?: string;
  value: TimeRange<W>;
  windows: readonly W[];
  defaultWindow: W;
  onChange: (range: TimeRange<W>) => void;
};

type CustomRangeFormProps = {
  mode: TimeZoneMode;
  initial: { from: string; to: string };
  onApply: (range: { from: string; to: string }) => void;
};

const HOUR_MS = 3_600_000;

export function TimeRangePicker<W extends string>({ label = "Time range", value, windows, defaultWindow, onChange }: TimeRangePickerProps<W>) {
  const { mode, time } = useTimeFormat();
  const options = windows.map((window) => ({ value: window, label: window }));
  return (
    <div className="flex flex-wrap items-center gap-2">
      <Segmented label={label} value={value.kind === "window" ? value.window : null} options={options} onChange={(window) => onChange({ kind: "window", window })} />
      {value.kind === "custom" && (
        <span className="inline-flex h-[26px] items-center gap-1.5 rounded-full border border-accent/45 bg-accent/14 pr-0.5 pl-2.5 text-12 whitespace-nowrap">
          {time(value.from, { seconds: false, date: "always" })} → {time(value.to, { seconds: false, date: "always", zone: true })}
          <IconButton label="Clear custom range" size="sm" onClick={() => onChange({ kind: "window", window: defaultWindow })}>
            <X className="size-3" />
          </IconButton>
        </span>
      )}
      <Popover
        label="Custom range"
        button={
          <>
            <CalendarRange className="size-3" />
            Custom
          </>
        }
        buttonClassName="inline-flex h-7 items-center gap-1.5 rounded-md border border-border-strong bg-surface-2 px-2.5 text-12 font-medium hover:bg-overlay"
        panelClassName="w-[300px] p-3"
      >
        {(close) => (
          <CustomRangeForm
            mode={mode}
            initial={value.kind === "custom" ? value : lastHour()}
            onApply={(range) => {
              onChange({ kind: "custom", ...range });
              close();
            }}
          />
        )}
      </Popover>
    </div>
  );
}

function CustomRangeForm({ mode, initial, onApply }: CustomRangeFormProps) {
  const [from, setFrom] = useState(() => toInputValue(initial.from, mode));
  const [to, setTo] = useState(() => toInputValue(initial.to, mode));
  const fromIso = fromInputValue(from, mode);
  const toIso = fromInputValue(to, mode);
  const valid = fromIso !== null && toIso !== null && fromIso < toIso;
  const zone = zoneLabel(mode);
  return (
    <form
      className="flex flex-col gap-2.5"
      onSubmit={(event) => {
        event.preventDefault();
        if (valid) onApply({ from: fromIso, to: toIso });
      }}
    >
      <Input type="datetime-local" label={`From (${zone})`} value={from} onChange={(event) => setFrom(event.target.value)} />
      <Input type="datetime-local" label={`To (${zone})`} value={to} onChange={(event) => setTo(event.target.value)} />
      {!valid && <span className="text-11 text-danger">The start must be before the end.</span>}
      <Button type="submit" variant="primary" disabled={!valid}>
        Apply
      </Button>
    </form>
  );
}

function lastHour(): { from: string; to: string } {
  const now = Date.now();
  return { from: new Date(now - HOUR_MS).toISOString(), to: new Date(now).toISOString() };
}
