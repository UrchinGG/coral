/* tslint:disable */
/* eslint-disable */

export type Camera = "map" | "follow" | "pov" | "fly";

export interface Adapter {
    name: string;
    backend: "webgpu" | "webgl";
    software: boolean;
    tier: "low" | "mid" | "high";
}

export interface ReplayInfo {
    durationMs: number;
    startMs: number;
    game: string | null;
    mode: string | null;
    map: string | null;
    date: string | null;
    time: string | null;
    players: ReplayPlayer[];
    chat: ChatLine[];
    skins: string[];
}

export interface ReplayPlayer {
    index: number;
    username: string;
    teamColor: string;
    skinHash: string | null;
    skinOwner: string | null;
}

export interface ChatLine {
    t: number;
    text: string;
    segments: ChatSegment[];
}

export interface ChatSegment {
    text: string;
    color: string;
    bold: boolean;
    italic: boolean;
    underlined: boolean;
    strikethrough: boolean;
}



export class Viewer {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    static create(canvas: HTMLCanvasElement, web_gpu: boolean): Promise<Viewer>;
    follow(index: number): void;
    frame(now: number): void;
    input(event: Event): void;
    loadAssets(files: any): void;
    load(bytes: Uint8Array): ReplayInfo;
    pickPlayer(x: number, y: number, radius: number): number | undefined;
    presence(): Uint8Array;
    resize(width: number, height: number, pixel_ratio: number): void;
    seek(ms: number): void;
    setCamera(name: Camera): void;
    setFollowed(index: number): void;
    setInsets(right: number, bottom: number): void;
    setReducedMotion(reduced: boolean): void;
    setSkin(index: number, rgba: Uint8Array): void;
    setSpeed(speed: number): void;
    toggle(): void;
    readonly adapter: Adapter;
    autoCamera: boolean;
    readonly camera: Camera;
    readonly durationMs: number;
    readonly flySpeed: number;
    readonly followed: number | undefined;
    readonly heard: Float32Array;
    readonly listener: Float32Array;
    readonly pendingSections: number;
    readonly playing: boolean;
    readonly soundNames: string[];
    readonly time: number;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_viewer_free: (a: number, b: number) => void;
    readonly viewer_adapter: (a: number) => any;
    readonly viewer_autoCamera: (a: number) => number;
    readonly viewer_camera: (a: number) => [number, number];
    readonly viewer_create: (a: any, b: number) => any;
    readonly viewer_durationMs: (a: number) => number;
    readonly viewer_flySpeed: (a: number) => number;
    readonly viewer_follow: (a: number, b: number) => void;
    readonly viewer_followed: (a: number) => number;
    readonly viewer_frame: (a: number, b: number) => void;
    readonly viewer_heard: (a: number) => [number, number];
    readonly viewer_input: (a: number, b: any) => void;
    readonly viewer_listener: (a: number) => [number, number];
    readonly viewer_load: (a: number, b: number, c: number) => [number, number, number];
    readonly viewer_loadAssets: (a: number, b: any) => [number, number];
    readonly viewer_pendingSections: (a: number) => number;
    readonly viewer_pickPlayer: (a: number, b: number, c: number, d: number) => number;
    readonly viewer_playing: (a: number) => number;
    readonly viewer_presence: (a: number) => [number, number];
    readonly viewer_resize: (a: number, b: number, c: number, d: number) => void;
    readonly viewer_seek: (a: number, b: number) => void;
    readonly viewer_setCamera: (a: number, b: number, c: number) => void;
    readonly viewer_setFollowed: (a: number, b: number) => void;
    readonly viewer_setInsets: (a: number, b: number, c: number) => void;
    readonly viewer_setReducedMotion: (a: number, b: number) => void;
    readonly viewer_setSkin: (a: number, b: number, c: number, d: number) => [number, number];
    readonly viewer_setSpeed: (a: number, b: number) => void;
    readonly viewer_set_autoCamera: (a: number, b: number) => void;
    readonly viewer_soundNames: (a: number) => [number, number];
    readonly viewer_time: (a: number) => number;
    readonly viewer_toggle: (a: number) => void;
    readonly wasm_bindgen_b45a68e03d5bb1f7___convert__closures_____invoke___js_sys_7ea326f30f3133ed___Function_fn_wasm_bindgen_b45a68e03d5bb1f7___JsValue_____wasm_bindgen_b45a68e03d5bb1f7___sys__Undefined___js_sys_7ea326f30f3133ed___Function_fn_wasm_bindgen_b45a68e03d5bb1f7___JsValue_____wasm_bindgen_b45a68e03d5bb1f7___sys__Undefined_______true_: (a: number, b: number, c: any, d: any) => void;
    readonly wasm_bindgen_b45a68e03d5bb1f7___convert__closures_____invoke___wasm_bindgen_b45a68e03d5bb1f7___sys__JsNullable_wgpu_62d8cbaefee4dd42___backend__webgpu__webgpu_sys__gen_GpuAdapter__GpuAdapter___core_260d1872709b9755___result__Result_____wasm_bindgen_b45a68e03d5bb1f7___JsError___true_: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_b45a68e03d5bb1f7___convert__closures_____invoke___wasm_bindgen_b45a68e03d5bb1f7___sys__JsNullable_wgpu_62d8cbaefee4dd42___backend__webgpu__webgpu_sys__gen_GpuAdapter__GpuAdapter___core_260d1872709b9755___result__Result_____wasm_bindgen_b45a68e03d5bb1f7___JsError___true__32: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_b45a68e03d5bb1f7___convert__closures_____invoke___wasm_bindgen_b45a68e03d5bb1f7___sys__JsNullable_wgpu_62d8cbaefee4dd42___backend__webgpu__webgpu_sys__gen_GpuAdapter__GpuAdapter___core_260d1872709b9755___result__Result_____wasm_bindgen_b45a68e03d5bb1f7___JsError___true__33: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_b45a68e03d5bb1f7___convert__closures_____invoke___wasm_bindgen_b45a68e03d5bb1f7___sys__JsNullable_wgpu_62d8cbaefee4dd42___backend__webgpu__webgpu_sys__gen_GpuAdapter__GpuAdapter___core_260d1872709b9755___result__Result_____wasm_bindgen_b45a68e03d5bb1f7___JsError___true__34: (a: number, b: number, c: any) => [number, number];
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_destroy_closure: (a: number, b: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __externref_drop_slice: (a: number, b: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
