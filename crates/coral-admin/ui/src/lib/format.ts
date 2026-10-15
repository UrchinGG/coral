const MS_PER_SECOND = 1_000;
const TEN_SECONDS_MS = 10_000;
const BYTE_UNITS = ["B", "KB", "MB", "GB"] as const;
const BYTES_PER_UNIT = 1_024;

const integerFormat = new Intl.NumberFormat("en-US", { maximumFractionDigits: 0 });
const decimalFormat = new Intl.NumberFormat("en-US", { maximumFractionDigits: 1 });

export function formatNumber(value: number): string {
  return integerFormat.format(value);
}

export function formatDecimal(value: number): string {
  return decimalFormat.format(value);
}

export function formatMs(ms: number): string {
  if (ms < MS_PER_SECOND) return `${Math.round(ms)} ms`;
  const digits = ms >= TEN_SECONDS_MS ? 1 : 2;
  return `${(ms / MS_PER_SECOND).toFixed(digits)} s`;
}

export function formatPercent(ratio: number, digits = 1): string {
  return `${(ratio * 100).toFixed(digits)}%`;
}

export function formatBytes(bytes: number): string {
  let value = bytes;
  let unit = 0;
  while (value >= BYTES_PER_UNIT && unit < BYTE_UNITS.length - 1) {
    value /= BYTES_PER_UNIT;
    unit += 1;
  }
  return unit === 0 ? `${formatNumber(value)} B` : `${value.toFixed(1)} ${BYTE_UNITS[unit]}`;
}

export function plural(count: number, singular: string, pluralForm = `${singular}s`): string {
  return `${formatNumber(count)} ${count === 1 ? singular : pluralForm}`;
}
