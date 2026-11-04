import init, { Viewer } from "../viewer/pkg/replay_viewer.js";
import { viewerWasm } from "../viewer/wasm";
import { ASSETS, assetUrl } from "./assets";

type Progress = (fraction: number | null) => void;
type WebGpu = { requestAdapter(): Promise<unknown> };

const WEBGPU_PROBE_MS = 2500;
const VIEWER_START_MS = 20000;

const request = async (url: string) => {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url} answered ${response.status}.`);
  return response;
};

const readBytes = async (url: string, progress: Progress) => {
  const response = await request(url);
  const total = Number(response.headers.get("content-length"));
  const reader = response.body!.getReader();
  const chunks: Uint8Array<ArrayBuffer>[] = [];
  let received = 0;
  for (let read = await reader.read(); !read.done; read = await reader.read()) {
    chunks.push(read.value);
    received += read.value.length;
    progress(total ? Math.min(received / total, 1) : null);
  }
  return new Uint8Array(await new Blob(chunks).arrayBuffer());
};

const readAsset = async (name: string) => {
  const { body, headers } = await request(assetUrl(name));
  const unzipped =
    headers.get("content-encoding") === "gzip"
      ? body!
      : body!.pipeThrough(new DecompressionStream("gzip"));
  return new Uint8Array(await new Response(unzipped).arrayBuffer());
};

const timer = (ms: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, ms));

const supportsWebGpu = async () => {
  const gpu = (navigator as Navigator & { gpu?: WebGpu }).gpu;
  if (!gpu) return false;
  const adapter = gpu.requestAdapter().catch(() => null);
  const answer = await Promise.race([
    adapter,
    timer(WEBGPU_PROBE_MS).then(() => null),
  ]);
  return answer !== null;
};

const createViewer = async (canvas: HTMLCanvasElement) =>
  Promise.race([
    Viewer.create(canvas, await supportsWebGpu()),
    timer(VIEWER_START_MS).then(() => {
      throw new Error("The graphics driver never answered.");
    }),
  ]);

export const download = (url: string) => {
  let fraction: number | null = null;
  let report: Progress = () => {};
  const bytes = readBytes(url, (latest) => report((fraction = latest)));
  return (progress: Progress) => {
    report = progress;
    progress(fraction);
    return bytes;
  };
};

export const fileSource = (file: File) => ({
  name: file.name,
  src: undefined,
  read: async () => new Uint8Array(await file.arrayBuffer()),
});

export const urlSource = (src: string, read = download(src)) => {
  const { pathname } = new URL(src, location.href);
  return {
    name: decodeURIComponent(pathname.slice(pathname.lastIndexOf("/") + 1)),
    src,
    read,
  };
};

export const startViewer = async (canvas: HTMLCanvasElement) => {
  if (!viewerWasm) throw new Error("This build doesn't include the viewer.");
  const [viewer, files] = await Promise.all([
    init({ module_or_path: viewerWasm }).then(() => createViewer(canvas)),
    Promise.all(
      ASSETS.map(async (name) => [name, await readAsset(name)] as const),
    ),
  ]);
  viewer.loadAssets(Object.fromEntries(files));
  return viewer;
};
