import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";

import { buildCurl, quoteShellArgument, type CurlRequest } from "./shell";

const TWO_MIB = 2 * 1024 * 1024;
const SHELL_TIMEOUT = 30_000;
const BASH_OUTPUT_LIMIT = 16 * 1024 * 1024;
const ARGV_CURL = `curl() { printf '%s\\0' "$@"; }`;
const STDIN_CURL = "curl() { cat; }";

const sandbox = mkdtempSync(join(tmpdir(), "coral-shell-"));
const canary = join(sandbox, "pwned");

const HOSTILE_VALUES = [
  "it's",
  "''''",
  `$(touch ${canary})`,
  `\`touch ${canary}\``,
  "${HOME}",
  "line one\nline two",
  "carriage\rreturn",
  "tab\there",
  "bell\u0007escape\u001b[31m",
  "delete\u007f and c1\u0085",
  "-oops",
  "--help",
  "back\\slash",
  'double "quotes"',
  "glob * ? [a-z] {a,b}",
  "unicode é 漢字 🙂",
  "!history !!",
];

function runBash(script: string): Buffer {
  return execFileSync("bash", ["--norc", "--noprofile"], { input: script, maxBuffer: BASH_OUTPUT_LIMIT });
}

function argvOf(request: CurlRequest): string[] {
  const output = runBash(`${ARGV_CURL}\n${buildCurl(request).command}\n`).toString("utf8");
  return output.split("\0").slice(0, -1);
}

function stdinOf(request: CurlRequest): string {
  return runBash(`${STDIN_CURL}\n${buildCurl(request).command}\n`).toString("utf8");
}

afterAll(() => rmSync(sandbox, { recursive: true, force: true }));

describe("quoteShellArgument", () => {
  it.each(HOSTILE_VALUES)("round-trips %j through bash unchanged", (value) => {
    const output = runBash(`printf '%s' ${quoteShellArgument(value)}`).toString("utf8");
    expect(output).toBe(value);
    expect(existsSync(canary)).toBe(false);
  });

  it("quotes the empty string", () => {
    expect(quoteShellArgument("")).toBe("''");
  });

  it("escapes a single quote by closing, escaping and reopening", () => {
    expect(quoteShellArgument("it's")).toBe("'it'\\''s'");
  });

  it("writes control bytes as ANSI-C hex segments and never emits them raw", () => {
    const quoted = quoteShellArgument("a\u0001b\nc\u001b");
    expect(quoted).toBe("'a'$'\\x01''b'$'\\x0a''c'$'\\x1b'");
    expect(quoted.includes("\n")).toBe(false);
    expect(quoted.includes("\u0001")).toBe(false);
    expect(quoted.includes("\u001b")).toBe(false);
  });

  it("refuses a NUL byte, which no shell argument can carry", () => {
    expect(() => quoteShellArgument("a\u0000b")).toThrow(RangeError);
  });

  it("writes C1 controls as their UTF-8 bytes", () => {
    expect(quoteShellArgument("\u0085")).toBe("$'\\xc2\\x85'");
  });
});

describe("buildCurl", () => {
  const hostileRequest: CurlRequest = {
    method: "POST",
    url: "https://api.urchin.gg/v3/players?name=it's&q=$(id)`id`",
    headers: [
      ["x-api-key", "it's a $(secret) `key`"],
      ["user-agent", "first line\nsecond line"],
      ["bad header", "dropped"],
      ["x\ninjected", "dropped"],
      ["x-empty", ""],
    ],
    body: '{"uuids":["it\'s"],"note":"$(id)\\n`id`"}\nsecond line',
  };

  it("produces exactly the intended argv for hostile input", () => {
    expect(argvOf(hostileRequest)).toEqual([
      "--globoff",
      "--request",
      "POST",
      "--header",
      "x-api-key: it's a $(secret) `key`",
      "--header",
      "user-agent: first line\nsecond line",
      "--header",
      "x-empty: ",
      "--data-binary",
      "@-",
      "--",
      "https://api.urchin.gg/v3/players?name=it's&q=$(id)`id`",
    ]);
    expect(existsSync(canary)).toBe(false);
  });

  it("pipes the body through stdin byte for byte", () => {
    expect(stdinOf(hostileRequest)).toBe(hostileRequest.body);
  });

  it("omits headers whose names are not plain tokens and reports them", () => {
    expect(buildCurl(hostileRequest).omittedHeaders).toEqual(["bad header", "x\ninjected"]);
  });

  it("passes a dash-prefixed target after the end of options", () => {
    const argv = argvOf({ method: "GET", url: "-K/etc/passwd", headers: [], body: null });
    expect(argv.slice(-2)).toEqual(["--", "-K/etc/passwd"]);
    expect(argv).not.toContain("--data-binary");
  });

  it("omits a header whose value holds a NUL byte and reports it", () => {
    const request: CurlRequest = { method: "GET", url: "https://api.urchin.gg/v3/x", headers: [["x-nul", "a\u0000b"], ["x-kept", "c\u0001d"]], body: null };
    expect(buildCurl(request).omittedHeaders).toEqual(["x-nul"]);
    expect(argvOf(request)).toEqual(["--globoff", "--request", "GET", "--header", "x-kept: c\u0001d", "--", "https://api.urchin.gg/v3/x"]);
  });

  it("replays a body holding NUL and control bytes byte for byte", () => {
    const request: CurlRequest = { method: "PUT", url: "https://api.urchin.gg/v3/x", headers: [], body: "nul\u0000byte\u0007bell é 🙂\u0000" };
    const { command } = buildCurl(request);
    expect(command.includes("\u0000")).toBe(false);
    expect(command.includes("\u0007")).toBe(false);
    expect(stdinOf(request)).toBe(request.body);
  });

  it("handles a 2 MiB body", () => {
    const chunk = '{"uuid":"94589c87999b4a7ab3add2ae50711e64","q":"it\'s $(x)"}\n';
    const body = chunk.repeat(Math.ceil(TWO_MIB / chunk.length)).slice(0, TWO_MIB);
    const request: CurlRequest = { method: "POST", url: "https://api.urchin.gg/v3/players", headers: [], body };
    expect(buildCurl(request).command.length).toBeGreaterThan(TWO_MIB);
    expect(stdinOf(request)).toBe(body);
  }, SHELL_TIMEOUT);

  it("replays a 2 MiB body holding NUL bytes", () => {
    const chunk = "nul\u0000and 'quote' $(x)\n";
    const body = chunk.repeat(Math.ceil(TWO_MIB / chunk.length)).slice(0, TWO_MIB);
    expect(stdinOf({ method: "POST", url: "https://api.urchin.gg/v3/players", headers: [], body })).toBe(body);
  }, SHELL_TIMEOUT);
});
