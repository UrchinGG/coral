const UNITS = ["", "k", "M", "B"];
const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"];
const STEP = 1000;
const BYTE_STEP = 1024;
const DECIMAL_BELOW = 100;

const grouped = new Intl.NumberFormat("en-US");

const scaled = (
  value: number,
  step: number,
  units: string[],
  space: string,
) => {
  let index = 0;
  while (Math.abs(value) >= step && index < units.length - 1) {
    value /= step;
    index += 1;
  }
  const digits = index && value < DECIMAL_BELOW ? 1 : 0;
  return `${value.toFixed(digits).replace(/\.0$/, "")}${space}${units[index]}`;
};

export const FORMATS = {
  number: (value: number) => grouped.format(value),
  short: (value: number) =>
    value < STEP ? grouped.format(value) : scaled(value, STEP, UNITS, ""),
  bytes: (value: number) => scaled(value, BYTE_STEP, BYTE_UNITS, " "),
  text: (value: unknown) => String(value),
};

export type Format = keyof typeof FORMATS;

export const titleCase = (key: string) =>
  key.replace(/_/g, " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
