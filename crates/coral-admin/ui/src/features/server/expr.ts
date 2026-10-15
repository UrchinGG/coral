export const NICKNAME_MAX_LEN = 32;

const MAX_DEPTH = 16;
const MAX_OUTPUT_LEN = 256;

export type EvalValue = number | string | boolean | null;
export type JsonValue = string | number | boolean | null | JsonValue[] | { [key: string]: JsonValue };

export type RenderedNickname = {
  full: string;
  truncated: string;
  wasTruncated: boolean;
  hasTruncatableSegment: boolean;
};

export type ExprSegmentKind = "literal" | "brace" | "field" | "string" | "number" | "keyword" | "operator";
export type ExprSegment = { text: string; kind: ExprSegmentKind };

type Token =
  | { t: "number"; value: number }
  | { t: "string"; value: string }
  | { t: "ident"; value: string }
  | { t: "dot" }
  | { t: "plus" }
  | { t: "minus" }
  | { t: "star" }
  | { t: "slash" }
  | { t: "percent" }
  | { t: "lparen" }
  | { t: "rparen" }
  | { t: "gt" }
  | { t: "lt" }
  | { t: "ge" }
  | { t: "le" }
  | { t: "eq" }
  | { t: "ne" }
  | { t: "and" }
  | { t: "or" }
  | { t: "not" }
  | { t: "if" }
  | { t: "else" }
  | { t: "comma" }
  | { t: "colon" };

type CompareOp = "gt" | "lt" | "ge" | "le" | "eq" | "ne";
type BinOp = CompareOp | "add" | "sub" | "mul" | "div" | "mod" | "and" | "or";

type Expr =
  | { e: "number"; value: number }
  | { e: "string"; value: string }
  | { e: "field"; path: string[] }
  | { e: "binop"; left: Expr; op: BinOp; right: Expr }
  | { e: "not"; inner: Expr }
  | { e: "cond"; branches: [Expr, Expr][]; fallback: Expr };

const KEYWORDS = new Set(["if", "else", "and", "or", "not"]);

const utf8 = new TextEncoder();

export function validateCondition(condition: string): string | null {
  try {
    new Parser(tokenize(condition)).parse();
    return null;
  } catch (err) {
    return errorMessage(err);
  }
}

export function validateTemplate(template: string): string | null {
  const chars = Array.from(template);
  let i = 0;
  try {
    while (i < chars.length) {
      if (chars[i] === "{") {
        const [inner, end] = extractBraceContent(chars, i + 1);
        i = end;
        const trimmed = inner.trim();
        const exprInput = trimmed.startsWith("..") ? trimmed.slice(2).trim() : trimmed;
        const [exprStr] = splitFormatSpec(exprInput);
        new Parser(tokenize(exprStr)).parse();
      } else {
        i += 1;
      }
    }
    return null;
  } catch (err) {
    return errorMessage(err);
  }
}

export function evalCondition(condition: string, ctx: JsonValue): boolean {
  return asBool(evalExpr(new Parser(tokenize(condition)).parse(), ctx));
}

export function renderNickname(template: string, ctx: JsonValue): RenderedNickname {
  const { before, truncatable, after } = renderTemplate(template, ctx);
  const full = before + (truncatable ?? "") + after;
  const truncated = toTruncated(before, truncatable, after, NICKNAME_MAX_LEN);
  return {
    full,
    truncated,
    wasTruncated: truncated !== full,
    hasTruncatableSegment: truncatable !== null,
  };
}

export function byteLength(s: string): number {
  return utf8.encode(s).length;
}

export function highlightTemplate(template: string): ExprSegment[] {
  const segments: ExprSegment[] = [];
  const chars = Array.from(template);
  let i = 0;
  let literal = "";

  const flushLiteral = () => {
    if (literal) {
      segments.push({ text: literal, kind: "literal" });
      literal = "";
    }
  };

  while (i < chars.length) {
    if (chars[i] === "{") {
      flushLiteral();
      const [inner, end] = extractBraceContent(chars, i + 1);
      const closed = end > i + 1 + Array.from(inner).length;
      segments.push({ text: "{", kind: "brace" });
      segments.push(...highlightExpression(inner));
      if (closed) segments.push({ text: "}", kind: "brace" });
      i = end;
    } else {
      literal += chars[i];
      i += 1;
    }
  }
  flushLiteral();
  return segments;
}

export function highlightExpression(expr: string): ExprSegment[] {
  const segments: ExprSegment[] = [];
  const chars = Array.from(expr);
  let i = 0;

  const push = (text: string, kind: ExprSegmentKind) => {
    const last = segments[segments.length - 1];
    if (last && last.kind === kind) {
      last.text += text;
    } else {
      segments.push({ text, kind });
    }
  };

  while (i < chars.length) {
    const c = chars[i];
    if (/\s/.test(c)) {
      push(c, "literal");
      i += 1;
    } else if (c === '"') {
      let j = i + 1;
      while (j < chars.length && chars[j] !== '"') j += 1;
      if (j < chars.length) j += 1;
      push(chars.slice(i, j).join(""), "string");
      i = j;
    } else if (/[0-9]/.test(c)) {
      let j = i;
      while (j < chars.length && /[0-9.]/.test(chars[j])) j += 1;
      push(chars.slice(i, j).join(""), "number");
      i = j;
    } else if (/[A-Za-z_]/.test(c)) {
      let j = i;
      while (j < chars.length && /[A-Za-z0-9_]/.test(chars[j])) j += 1;
      let word = chars.slice(i, j).join("");
      if (KEYWORDS.has(word)) {
        push(word, "keyword");
        i = j;
        continue;
      }
      while (j < chars.length && chars[j] === "." && j + 1 < chars.length && /[A-Za-z0-9_]/.test(chars[j + 1])) {
        let k = j + 1;
        while (k < chars.length && /[A-Za-z0-9_]/.test(chars[k])) k += 1;
        word += "." + chars.slice(j + 1, k).join("");
        j = k;
      }
      push(word, "field");
      i = j;
    } else {
      push(c, "operator");
      i += 1;
    }
  }
  return segments;
}

export function contextFieldPaths(ctx: JsonValue, limit = 48): string[] {
  const paths: string[] = [];
  const walk = (value: JsonValue, prefix: string, depth: number) => {
    if (paths.length >= limit) return;
    if (value === null || typeof value !== "object" || Array.isArray(value)) {
      if (prefix) paths.push(prefix);
      return;
    }
    if (depth >= 3) return;
    for (const [key, child] of Object.entries(value)) {
      walk(child, prefix ? `${prefix}.${key}` : key, depth + 1);
    }
  };
  walk(ctx, "", 0);
  return paths;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function tokenize(input: string): Token[] {
  const tokens: Token[] = [];
  const chars = Array.from(input);
  let i = 0;

  const SINGLE: Record<string, Token> = {
    ".": { t: "dot" },
    "+": { t: "plus" },
    "-": { t: "minus" },
    "*": { t: "star" },
    "/": { t: "slash" },
    "%": { t: "percent" },
    "(": { t: "lparen" },
    ")": { t: "rparen" },
    ",": { t: "comma" },
    ":": { t: "colon" },
  };

  while (i < chars.length) {
    const c = chars[i];
    if (/\s/.test(c)) {
      i += 1;
    } else if (SINGLE[c]) {
      tokens.push(SINGLE[c]);
      i += 1;
    } else if (c === ">" && chars[i + 1] === "=") {
      tokens.push({ t: "ge" });
      i += 2;
    } else if (c === "<" && chars[i + 1] === "=") {
      tokens.push({ t: "le" });
      i += 2;
    } else if (c === "=" && chars[i + 1] === "=") {
      tokens.push({ t: "eq" });
      i += 2;
    } else if (c === "!" && chars[i + 1] === "=") {
      tokens.push({ t: "ne" });
      i += 2;
    } else if (c === ">") {
      tokens.push({ t: "gt" });
      i += 1;
    } else if (c === "<") {
      tokens.push({ t: "lt" });
      i += 1;
    } else if (c === '"') {
      i += 1;
      const start = i;
      while (i < chars.length && chars[i] !== '"') i += 1;
      tokens.push({ t: "string", value: chars.slice(start, i).join("") });
      if (i < chars.length) i += 1;
    } else if (/[0-9]/.test(c)) {
      const start = i;
      while (i < chars.length && /[0-9.]/.test(chars[i])) i += 1;
      const s = chars.slice(start, i).join("");
      const n = Number(s);
      if (Number.isNaN(n)) throw new Error(`invalid number: '${s}'`);
      tokens.push({ t: "number", value: n });
    } else if (/[A-Za-z0-9_]/.test(c)) {
      const start = i;
      while (i < chars.length && /[A-Za-z0-9_]/.test(chars[i])) i += 1;
      const word = chars.slice(start, i).join("");
      if (word === "and" || word === "or" || word === "not" || word === "if" || word === "else") {
        tokens.push({ t: word });
      } else {
        tokens.push({ t: "ident", value: word });
      }
    } else {
      throw new Error(`unexpected character: '${c}'`);
    }
  }

  return tokens;
}

class Parser {
  private tokens: Token[];
  private pos = 0;
  private depth = 0;

  constructor(tokens: Token[]) {
    this.tokens = tokens;
  }

  parse(): Expr {
    const expr = this.parseOr();
    if (this.pos < this.tokens.length) {
      throw new Error(`unexpected token: ${describeToken(this.tokens[this.pos])}`);
    }
    return expr;
  }

  private peek(): Token | undefined {
    return this.tokens[this.pos];
  }

  private advance(): Token | undefined {
    const tok = this.tokens[this.pos];
    if (tok) this.pos += 1;
    return tok;
  }

  private expect(kind: Token["t"]): void {
    const tok = this.advance();
    if (!tok || tok.t !== kind) {
      throw new Error(`expected '${kind}', got ${tok ? describeToken(tok) : "end of input"}`);
    }
  }

  private enter(): void {
    this.depth += 1;
    if (this.depth > MAX_DEPTH) throw new Error("expression too deeply nested");
  }

  private leave(): void {
    this.depth -= 1;
  }

  private parseOr(): Expr {
    this.enter();
    let left = this.parseAnd();
    while (this.peek()?.t === "or") {
      this.advance();
      left = { e: "binop", left, op: "or", right: this.parseAnd() };
    }
    this.leave();
    return left;
  }

  private parseAnd(): Expr {
    this.enter();
    let left = this.parseComparison();
    while (this.peek()?.t === "and") {
      this.advance();
      left = { e: "binop", left, op: "and", right: this.parseComparison() };
    }
    this.leave();
    return left;
  }

  private parseComparison(): Expr {
    this.enter();
    const left = this.parseAdditive();
    const op = this.peekComparisonOp();
    if (!op) {
      this.leave();
      return left;
    }
    this.advance();
    const right = this.parseAdditive();
    this.leave();
    return { e: "binop", left, op, right };
  }

  private parseAdditive(): Expr {
    this.enter();
    let left = this.parseMultiplicative();
    for (;;) {
      const t = this.peek()?.t;
      if (t !== "plus" && t !== "minus") break;
      this.advance();
      left = { e: "binop", left, op: t === "plus" ? "add" : "sub", right: this.parseMultiplicative() };
    }
    this.leave();
    return left;
  }

  private parseMultiplicative(): Expr {
    this.enter();
    let left = this.parseUnary();
    for (;;) {
      const t = this.peek()?.t;
      if (t !== "star" && t !== "slash" && t !== "percent") break;
      this.advance();
      const op: BinOp = t === "star" ? "mul" : t === "slash" ? "div" : "mod";
      left = { e: "binop", left, op, right: this.parseUnary() };
    }
    this.leave();
    return left;
  }

  private parseUnary(): Expr {
    this.enter();
    let expr: Expr;
    if (this.peek()?.t === "not") {
      this.advance();
      expr = { e: "not", inner: this.parseUnary() };
    } else {
      expr = this.parsePrimary();
    }
    this.leave();
    return expr;
  }

  private parsePrimary(): Expr {
    const tok = this.advance();
    if (!tok) throw new Error("unexpected end of expression");
    switch (tok.t) {
      case "number":
        return { e: "number", value: tok.value };
      case "string":
        return { e: "string", value: tok.value };
      case "if":
        return this.parseCond();
      case "ident": {
        const path = [tok.value];
        while (this.peek()?.t === "dot") {
          this.advance();
          const next = this.advance();
          if (!next || next.t !== "ident") {
            throw new Error(`expected field name after '.', got ${next ? describeToken(next) : "end of input"}`);
          }
          path.push(next.value);
        }
        return { e: "field", path };
      }
      case "lparen": {
        const expr = this.parseOr();
        this.expect("rparen");
        return expr;
      }
      default:
        throw new Error(`unexpected token: ${describeToken(tok)}`);
    }
  }

  private parseCond(): Expr {
    this.enter();
    const branches: [Expr, Expr][] = [];
    let subject: Expr | null = null;

    for (;;) {
      let condition: Expr;
      const shortOp = subject ? this.peekComparisonOp() : null;
      if (subject && shortOp) {
        this.advance();
        condition = { e: "binop", left: subject, op: shortOp, right: this.parseAdditive() };
      } else {
        condition = this.parseOr();
        if (!subject && condition.e === "binop") {
          subject = condition.left;
        }
      }

      this.expect("colon");
      branches.push([condition, this.parseOr()]);

      if (this.peek()?.t !== "comma") {
        throw new Error("expected ',' or 'else' in conditional");
      }
      this.advance();

      if (this.peek()?.t === "else") {
        this.advance();
        this.expect("colon");
        const fallback = this.parseOr();
        this.leave();
        return { e: "cond", branches, fallback };
      }
    }
  }

  private peekComparisonOp(): CompareOp | null {
    const t = this.peek()?.t;
    return t === "gt" || t === "lt" || t === "ge" || t === "le" || t === "eq" || t === "ne" ? t : null;
  }
}

function describeToken(tok: Token): string {
  if (tok.t === "ident") return `'${tok.value}'`;
  if (tok.t === "string") return `"${tok.value}"`;
  if (tok.t === "number") return `'${tok.value}'`;
  return `'${tok.t}'`;
}

function asNumber(v: EvalValue): number {
  if (typeof v === "number") return v;
  if (v === true) return 1;
  return 0;
}

function asBool(v: EvalValue): boolean {
  if (typeof v === "boolean") return v;
  if (typeof v === "number") return v !== 0;
  if (typeof v === "string") return v.length > 0;
  return false;
}

function displayValue(v: EvalValue): string {
  if (v === null) return "";
  if (typeof v === "number") return String(v);
  return String(v);
}

function formatValue(v: EvalValue, spec: string): string {
  if (typeof v === "number") {
    const precision = parseFormatSpec(spec);
    return precision === null ? String(v) : v.toFixed(precision);
  }
  return displayValue(v);
}

function parseFormatSpec(spec: string): number | null {
  const trimmed = spec.trim();
  if (!trimmed.startsWith(".") || !trimmed.endsWith("f")) return null;
  const n = Number(trimmed.slice(1, -1));
  return Number.isInteger(n) && n >= 0 ? n : null;
}

function resolveField(ctx: JsonValue, path: string[]): EvalValue {
  let current: JsonValue = ctx;
  for (const key of path) {
    if (current === null || typeof current !== "object" || Array.isArray(current)) return null;
    const next: JsonValue | undefined = current[key];
    if (next === undefined) return null;
    current = next;
  }
  if (current !== null && typeof current === "object") return JSON.stringify(current);
  return current;
}

function evalExpr(expr: Expr, ctx: JsonValue): EvalValue {
  switch (expr.e) {
    case "number":
      return expr.value;
    case "string":
      return expr.value;
    case "field":
      return resolveField(ctx, expr.path);
    case "not":
      return !asBool(evalExpr(expr.inner, ctx));
    case "cond": {
      for (const [condition, value] of expr.branches) {
        if (asBool(evalExpr(condition, ctx))) return evalExpr(value, ctx);
      }
      return evalExpr(expr.fallback, ctx);
    }
    case "binop": {
      const l = evalExpr(expr.left, ctx);
      const r = evalExpr(expr.right, ctx);
      switch (expr.op) {
        case "add":
          return asNumber(l) + asNumber(r);
        case "sub":
          return asNumber(l) - asNumber(r);
        case "mul":
          return asNumber(l) * asNumber(r);
        case "div":
          return asNumber(r) === 0 ? 0 : asNumber(l) / asNumber(r);
        case "mod":
          return asNumber(r) === 0 ? 0 : asNumber(l) % asNumber(r);
        case "gt":
          return asNumber(l) > asNumber(r);
        case "lt":
          return asNumber(l) < asNumber(r);
        case "ge":
          return asNumber(l) >= asNumber(r);
        case "le":
          return asNumber(l) <= asNumber(r);
        case "eq":
          return evalEquality(l, r);
        case "ne":
          return !evalEquality(l, r);
        case "and":
          return asBool(l) && asBool(r);
        case "or":
          return asBool(l) || asBool(r);
      }
    }
  }
}

function evalEquality(l: EvalValue, r: EvalValue): boolean {
  if (l === null && r === null) return true;
  if (l === null || r === null) return false;
  if (typeof l === "string" && typeof r === "string") return l === r;
  return asNumber(l) === asNumber(r);
}

function renderTemplate(template: string, ctx: JsonValue): { before: string; truncatable: string | null; after: string } {
  let output = "";
  let truncatableSpan: [number, number] | null = null;
  const chars = Array.from(template);
  let i = 0;

  while (i < chars.length && output.length < MAX_OUTPUT_LEN) {
    if (chars[i] === "{") {
      const [inner, end] = extractBraceContent(chars, i + 1);
      i = end;

      const trimmed = inner.trim();
      if (trimmed.startsWith("..")) {
        const exprStr = trimmed.slice(2).trim();
        let formatted: string;
        try {
          formatted = displayValue(evalExpr(new Parser(tokenize(exprStr)).parse(), ctx));
        } catch {
          formatted = `{..${exprStr}}`;
        }
        truncatableSpan = [output.length, formatted.length];
        output += formatted;
        continue;
      }

      const [exprStr, formatSpec] = splitFormatSpec(inner);
      try {
        const result = evalExpr(new Parser(tokenize(exprStr)).parse(), ctx);
        output += formatSpec === null ? displayValue(result) : formatValue(result, formatSpec);
      } catch {
        output += `{${inner}}`;
      }
    } else {
      output += chars[i];
      i += 1;
    }
  }

  output = output.slice(0, MAX_OUTPUT_LEN);

  if (truncatableSpan) {
    const [start, len] = truncatableSpan;
    const end = Math.min(start + len, output.length);
    return { before: output.slice(0, start), truncatable: output.slice(start, end), after: output.slice(end) };
  }
  return { before: output, truncatable: null, after: "" };
}

function toTruncated(before: string, truncatable: string | null, after: string, maxLen: number): string {
  if (truncatable === null) return truncateBytes(before + after, maxLen);

  const full = before + truncatable + after;
  if (byteLength(full) <= maxLen) return full;

  const budget = Math.max(0, maxLen - byteLength(before) - byteLength(after));
  if (budget === 0) return truncateBytes(before + after, maxLen);

  return before + truncateBytes(truncatable, budget) + after;
}

function truncateBytes(s: string, maxLen: number): string {
  if (byteLength(s) <= maxLen) return s;
  let out = "";
  let used = 0;
  for (const ch of s) {
    const b = byteLength(ch);
    if (used + b > maxLen) break;
    out += ch;
    used += b;
  }
  return out.trimEnd();
}

function extractBraceContent(chars: string[], start: number): [string, number] {
  let i = start;
  let depth = 1;
  while (i < chars.length && depth > 0) {
    if (chars[i] === "{") depth += 1;
    else if (chars[i] === "}") depth -= 1;
    if (depth > 0) i += 1;
  }
  return [chars.slice(start, i).join(""), i < chars.length ? i + 1 : i];
}

function splitFormatSpec(inner: string): [string, string | null] {
  const colon = inner.lastIndexOf(":");
  if (colon === -1) return [inner, null];
  const after = inner.slice(colon + 1).trim();
  if (after.startsWith(".") && after.endsWith("f")) {
    return [inner.slice(0, colon), after];
  }
  return [inner, null];
}
