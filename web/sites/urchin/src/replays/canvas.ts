import type { Viewer } from "../viewer/pkg/replay_viewer.js";
import { $ } from "./dom";
import { seekHint, toast } from "./overlays";

const REACH_PX = {
  mouse: { click: 5, pick: 28 },
  touch: { click: 12, pick: 44 },
};
const PIXEL_RATIO_CAP = { fine: 2, coarse: 1.5, software: 1 };
const MIN_PIXEL_RATIO = 0.5;
const PIXEL_BUDGET = { software: 1.2e6, low: 4.5e6, mid: 6e6, high: 9e6 };
const CLICK_MS = 450;
const DOUBLE_TAP_MS = 320;
const DOUBLE_TAP_PX = 60;
const SKIP_MS = 10000;

const reachOf = (event: PointerEvent) =>
  REACH_PX[event.pointerType === "mouse" ? "mouse" : "touch"];

const distance = (a: PointerEvent, b: PointerEvent) =>
  Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);

export const bindCanvas = (
  viewer: Viewer,
  follow: (index: number) => void,
  chatting: () => boolean,
) => {
  const canvas = $<HTMLCanvasElement>("#view");
  const stage = $("#stage");
  const transport = $("#transport");
  const panel = $("#match");
  const motion = matchMedia("(prefers-reduced-motion: reduce)");
  const coarse = matchMedia("(pointer: coarse)");
  let pressed: PointerEvent | undefined;
  let hovered: PointerEvent | undefined;
  let lastTap: PointerEvent | undefined;
  const { software, tier } = viewer.adapter;

  const tappable = () => viewer.camera !== "fly" && !chatting();

  const isClick = (event: PointerEvent) =>
    pressed !== undefined &&
    event.button === 0 &&
    event.timeStamp - pressed.timeStamp < CLICK_MS &&
    distance(event, pressed) <= reachOf(event).click;

  const isDoubleTap = (event: PointerEvent) => {
    const double =
      lastTap !== undefined &&
      event.timeStamp - lastTap.timeStamp < DOUBLE_TAP_MS &&
      distance(event, lastTap) < DOUBLE_TAP_PX;
    lastTap = double ? undefined : event;
    return double;
  };

  const doubleTap = (event: PointerEvent) => {
    const across = event.offsetX / canvas.clientWidth;
    if (across < 0.35) {
      viewer.seek(viewer.time - SKIP_MS);
      seekHint("−10 s", "left");
    } else if (across > 0.65) {
      viewer.seek(viewer.time + SKIP_MS);
      seekHint("+10 s", "right");
    } else {
      viewer.toggle();
      seekHint(viewer.playing ? "Play" : "Pause", "middle");
    }
  };

  const playerAt = (event: PointerEvent) =>
    viewer.pickPlayer(event.offsetX, event.offsetY, reachOf(event).pick);

  const showHover = () => {
    if (hovered) {
      const over = tappable() && playerAt(hovered) !== undefined;
      canvas.style.cursor = over ? "pointer" : "";
    }
    hovered = undefined;
  };

  canvas.addEventListener("pointerdown", (event) => {
    pressed = event.isPrimary ? event : undefined;
    canvas.setPointerCapture(event.pointerId);
    if (viewer.camera === "fly" && !chatting()) canvas.requestPointerLock();
    viewer.input(event);
  });
  canvas.addEventListener("pointerup", (event) => {
    viewer.input(event);
    if (!isClick(event) || !tappable()) return;
    if (event.pointerType !== "mouse" && isDoubleTap(event)) {
      doubleTap(event);
      return;
    }
    const index = playerAt(event);
    if (index !== undefined) follow(index);
  });
  canvas.addEventListener("pointermove", (event) => {
    viewer.input(event);
    if (event.pointerType !== "mouse" || event.buttons) return;
    if (!hovered) requestAnimationFrame(showHover);
    hovered = event;
  });
  canvas.addEventListener("pointercancel", (event) => viewer.input(event));
  canvas.addEventListener("dblclick", (event) => {
    if (pressed?.pointerType === "mouse") viewer.input(event);
  });
  canvas.addEventListener("contextmenu", (event) => event.preventDefault());
  canvas.addEventListener(
    "wheel",
    (event) => {
      if (chatting()) return;
      event.preventDefault();
      viewer.input(event);
      if (viewer.camera === "fly") {
        toast(`Flying at ${Math.round(viewer.flySpeed)} blocks/s`, 1200);
      }
    },
    { passive: false },
  );

  const pixelRatio = (width: number, height: number) => {
    const cap = software
      ? PIXEL_RATIO_CAP.software
      : coarse.matches
        ? PIXEL_RATIO_CAP.coarse
        : PIXEL_RATIO_CAP.fine;
    const budget = Math.sqrt(
      PIXEL_BUDGET[software ? "software" : tier] / Math.max(width * height, 1),
    );
    return Math.max(MIN_PIXEL_RATIO, Math.min(devicePixelRatio, cap, budget));
  };

  const resize = () => {
    const { clientWidth, clientHeight } = canvas;
    viewer.resize(
      clientWidth,
      clientHeight,
      pixelRatio(clientWidth, clientHeight),
    );
  };
  const inset = () => {
    const bottom =
      stage.getBoundingClientRect().bottom -
      transport.getBoundingClientRect().top;
    viewer.setInsets(panel.inert ? 0 : panel.offsetWidth, bottom);
    stage.style.setProperty("--transport-inset", `${bottom}px`);
  };
  const followMotion = () => viewer.setReducedMotion(motion.matches);
  const watchPixelRatio = () =>
    matchMedia(`(resolution: ${devicePixelRatio}dppx)`).addEventListener(
      "change",
      () => {
        resize();
        watchPixelRatio();
      },
      { once: true },
    );
  new ResizeObserver(resize).observe(canvas);
  const insets = new ResizeObserver(inset);
  insets.observe(transport);
  insets.observe(panel);
  new MutationObserver(inset).observe(panel, { attributeFilter: ["inert"] });
  motion.addEventListener("change", followMotion);
  watchPixelRatio();
  resize();
  inset();
  followMotion();
};
