export type CurlRequest = {
  method: string;
  url: string;
  headers: readonly (readonly [string, string])[];
  body: string | null;
};

export type CurlCommand = {
  command: string;
  omittedHeaders: string[];
};

const HEADER_NAME = /^[A-Za-z0-9-]+$/;
const NUL = "\u0000";
const LAST_C0_CONTROL = 0x1f;
const DELETE = 0x7f;
const LAST_C1_CONTROL = 0x9f;
const ARGUMENT_SEPARATOR = " \\\n  ";
const SINGLE_QUOTE_ESCAPE = "'\\''";
const HEX_RADIX = 16;
const BASE64_CHUNK_BYTES = 0x8000;
const encoder = new TextEncoder();

export function buildCurl(request: CurlRequest): CurlCommand {
  const accepted = request.headers.filter(isCarriableHeader);
  const omittedHeaders = request.headers.filter((header) => !isCarriableHeader(header)).map(([name]) => name);
  const args = [
    "curl",
    "--globoff",
    "--request",
    quoteShellArgument(request.method),
    ...accepted.flatMap(([name, value]) => ["--header", quoteShellArgument(`${name}: ${value}`)]),
    ...(request.body === null ? [] : ["--data-binary", "@-"]),
    "--",
    quoteShellArgument(request.url),
  ];
  const curl = joinArguments(args);
  const command = request.body === null ? curl : `${bodySource(request.body)} | ${curl}`;
  return { command, omittedHeaders };
}

export function quoteShellArgument(value: string): string {
  if (value.includes(NUL)) throw new RangeError("A shell argument cannot carry a NUL byte");
  if (value === "") return "''";
  const segments: string[] = [];
  let printable = "";
  for (const char of value) {
    if (!isNonPrintable(char)) {
      printable += char;
      continue;
    }
    if (printable !== "") segments.push(singleQuoted(printable));
    printable = "";
    segments.push(ansiCQuoted(char));
  }
  if (printable !== "") segments.push(singleQuoted(printable));
  return segments.join("");
}

function isCarriableHeader([name, value]: readonly [string, string]): boolean {
  return HEADER_NAME.test(name) && !value.includes(NUL);
}

function bodySource(body: string): string {
  if (!body.includes(NUL)) return `printf '%s' ${quoteShellArgument(body)}`;
  return `printf '%s' ${singleQuoted(base64Of(body))} | base64 --decode`;
}

function base64Of(text: string): string {
  const bytes = encoder.encode(text);
  let binary = "";
  for (let start = 0; start < bytes.length; start += BASE64_CHUNK_BYTES) {
    binary += String.fromCharCode(...bytes.subarray(start, start + BASE64_CHUNK_BYTES));
  }
  return btoa(binary);
}

function joinArguments(args: string[]): string {
  const [program, ...rest] = args;
  const lines = [program];
  for (let index = 0; index < rest.length; index += 1) {
    const arg = rest[index];
    const pairsWithNext = arg.startsWith("--") && index + 1 < rest.length && !rest[index + 1].startsWith("--");
    lines.push(pairsWithNext ? `${arg} ${rest[++index]}` : arg);
  }
  return lines.join(ARGUMENT_SEPARATOR);
}

function isNonPrintable(char: string): boolean {
  const code = char.charCodeAt(0);
  return code <= LAST_C0_CONTROL || (code >= DELETE && code <= LAST_C1_CONTROL);
}

function singleQuoted(text: string): string {
  return `'${text.replaceAll("'", SINGLE_QUOTE_ESCAPE)}'`;
}

function ansiCQuoted(char: string): string {
  const bytes = [...encoder.encode(char)].map((byte) => `\\x${byte.toString(HEX_RADIX).padStart(2, "0")}`);
  return `$'${bytes.join("")}'`;
}
