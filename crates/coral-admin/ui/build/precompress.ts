import { brotliCompressSync, constants } from "node:zlib";

import type { Plugin, Rolldown } from "vite";

type Chunk = Rolldown.OutputChunk;

type BudgetLine = { name: string; bytes: number; limit: number };

const KILOBYTE = 1_000;
const INITIAL_JS_BUDGET_BYTES = 112 * KILOBYTE;
const LAZY_CHUNK_BUDGET_BYTES = 30 * KILOBYTE;
const COMPRESSIBLE_FILE = /\.(?:js|css)$/;
const BROTLI_SUFFIX = ".br";
const NAME_COLUMN_WIDTH = 32;
const INITIAL_JS_NAME = "initial JS";

export function precompressWithBudgets(): Plugin {
  return {
    name: "coral-precompress-with-budgets",
    apply: "build",
    generateBundle: {
      order: "post",
      handler(_options, bundle) {
        const compressed = compressBundle(bundle);
        for (const [fileName, source] of compressed) this.emitFile({ type: "asset", fileName: `${fileName}${BROTLI_SUFFIX}`, source });
        const lines = budgetLines(chunksOf(bundle), compressed);
        this.environment.logger.info(formatReport(lines));
        const over = lines.filter((line) => line.bytes > line.limit);
        if (over.length > 0) this.error(`Brotli budget exceeded: ${over.map(describeLine).join("; ")}`);
      },
    },
  };
}

function compressBundle(bundle: Rolldown.OutputBundle): Map<string, Uint8Array> {
  const compressed = new Map<string, Uint8Array>();
  for (const output of Object.values(bundle)) {
    if (!COMPRESSIBLE_FILE.test(output.fileName)) continue;
    const source = output.type === "chunk" ? output.code : output.source;
    compressed.set(output.fileName, brotliCompressSync(source, { params: { [constants.BROTLI_PARAM_QUALITY]: constants.BROTLI_MAX_QUALITY } }));
  }
  return compressed;
}

function chunksOf(bundle: Rolldown.OutputBundle): Map<string, Chunk> {
  const chunks = new Map<string, Chunk>();
  for (const output of Object.values(bundle)) if (output.type === "chunk") chunks.set(output.fileName, output);
  return chunks;
}

function budgetLines(chunks: Map<string, Chunk>, compressed: Map<string, Uint8Array>): BudgetLine[] {
  const sizeOf = (files: Iterable<string>) => [...files].reduce((total, file) => total + (compressed.get(file)?.byteLength ?? 0), 0);
  const initial = new Set([...chunks.values()].filter((chunk) => chunk.isEntry).flatMap((chunk) => [...staticClosure(chunk.fileName, chunks)]));
  const lazy = [...chunks.values()]
    .filter((chunk) => chunk.isDynamicEntry)
    .map((chunk) => ({ name: chunk.name, bytes: sizeOf([...staticClosure(chunk.fileName, chunks)].filter((file) => !initial.has(file))), limit: LAZY_CHUNK_BUDGET_BYTES }))
    .sort((a, b) => b.bytes - a.bytes);
  return [{ name: INITIAL_JS_NAME, bytes: sizeOf(initial), limit: INITIAL_JS_BUDGET_BYTES }, ...lazy];
}

function staticClosure(start: string, chunks: Map<string, Chunk>): Set<string> {
  const reached = [start];
  for (const fileName of reached) {
    for (const imported of chunks.get(fileName)?.imports ?? []) if (!reached.includes(imported)) reached.push(imported);
  }
  return new Set(reached);
}

function formatReport(lines: readonly BudgetLine[]): string {
  const rows = lines.map((line) => `  ${line.name.padEnd(NAME_COLUMN_WIDTH)} ${kilobytes(line.bytes).padStart(9)} / ${kilobytes(line.limit)}`);
  return ["brotli q11 sizes against the bundle budgets (lazy chunks include their static imports outside the initial set):", ...rows].join("\n");
}

function describeLine(line: BudgetLine): string {
  return `${line.name} is ${kilobytes(line.bytes)}, over ${kilobytes(line.limit)}`;
}

function kilobytes(bytes: number): string {
  return `${(bytes / KILOBYTE).toFixed(1)} kB`;
}
