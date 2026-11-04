import type { Viewer } from "../viewer/pkg/replay_viewer.js";
import { $ } from "./dom";

interface Variant {
  file: string;
  weight: number;
  volume: number;
  pitch: number;
}

interface Playing {
  panner: PannerNode;
  at: Float32Array;
}

const ROOT = "/replay-assets/sounds";
const STORAGE_KEY = "replay.sound";
const MASTER_VOLUME = 0.7;
const STRIDE = 7;

const storedEnabled = () => {
  try {
    return localStorage.getItem(STORAGE_KEY) !== "off";
  } catch {
    return true;
  }
};

const store = (enabled: boolean) => {
  try {
    localStorage.setItem(STORAGE_KEY, enabled ? "on" : "off");
  } catch {}
};

const pick = (variants: Variant[]) => {
  let roll =
    Math.random() * variants.reduce((sum, { weight }) => sum + weight, 0);
  for (const variant of variants) {
    roll -= variant.weight;
    if (roll < 0) return variant;
  }
  return variants[0];
};

const place = ({ panner, at: [x, y, z] }: Playing, listener: Float32Array) => {
  const [px, py, pz, fx, fy, fz, ux, uy, uz] = listener;
  const [dx, dy, dz] = [x - px, y - py, z - pz];
  const [rx, ry, rz] = [
    fy * uz - fz * uy,
    fz * ux - fx * uz,
    fx * uy - fy * ux,
  ];
  panner.positionX.value = dx * rx + dy * ry + dz * rz;
  panner.positionY.value = dx * ux + dy * uy + dz * uz;
  panner.positionZ.value = -(dx * fx + dy * fy + dz * fz);
};

export const bindSound = (viewer: Viewer) => {
  const button = $("#sound");
  const buffers = new Map<string, Promise<AudioBuffer | undefined>>();
  const playing = new Set<Playing>();
  let enabled = storedEnabled();
  let audio: { context: AudioContext; master: GainNode } | undefined;
  let table: Promise<Record<string, Variant[]>> | undefined;

  const wake = () => {
    if (!enabled) return;
    if (!audio) {
      const context = new AudioContext();
      const master = new GainNode(context, { gain: MASTER_VOLUME });
      master.connect(context.destination);
      audio = { context, master };
    }
    if (audio.context.state === "suspended") void audio.context.resume();
  };

  const setEnabled = (on: boolean) => {
    enabled = on;
    store(on);
    button.ariaPressed = String(on);
    if (on) wake();
    else void audio?.context.suspend();
    return on;
  };

  const variants = async (name: string) => {
    table ??= fetch(`${ROOT}/sounds.json`)
      .then((response) => (response.ok ? response.json() : {}))
      .catch(() => ({}));
    return (await table)[name] ?? [];
  };

  const buffer = (context: AudioContext, file: string) => {
    if (!buffers.has(file)) {
      buffers.set(
        file,
        fetch(`${ROOT}/${file}.ogg`)
          .then((response) => response.arrayBuffer())
          .then((data) => context.decodeAudioData(data))
          .catch(() => undefined),
      );
    }
    return buffers.get(file)!;
  };

  const play = async (name: string, sound: Float32Array) => {
    const choices = await variants(name);
    if (!audio || !choices.length) return;
    const { context, master } = audio;
    const variant = pick(choices);
    const decoded = await buffer(context, variant.file);
    if (!decoded || !enabled) return;
    const [, , , volume, pitch, reach] = sound;
    const source = new AudioBufferSourceNode(context, {
      buffer: decoded,
      playbackRate: Math.min(Math.max(pitch * variant.pitch, 0.5), 2),
    });
    const gain = new GainNode(context, {
      gain: Math.min(volume, 1) * variant.volume,
    });
    const panner = new PannerNode(context, {
      panningModel: "equalpower",
      distanceModel: "linear",
      refDistance: 0,
      maxDistance: reach,
    });
    const entry = { panner, at: sound };
    place(entry, viewer.listener);
    playing.add(entry);
    source.addEventListener("ended", () => playing.delete(entry));
    source.connect(gain).connect(panner).connect(master);
    source.start();
  };

  button.addEventListener("click", () => setEnabled(!enabled));
  for (const type of ["pointerdown", "pointerup", "keydown"])
    addEventListener(type, wake);
  button.ariaPressed = String(enabled);

  return {
    toggle: () => setEnabled(!enabled),
    show: (names: string[]) => () => {
      if (audio?.context.state !== "running") return;
      const listener = viewer.listener;
      for (const entry of playing) place(entry, listener);
      const heard = viewer.heard;
      for (let index = 0; index < heard.length; index += STRIDE) {
        void play(names[heard[index]], heard.slice(index + 1, index + STRIDE));
      }
    },
  };
};
