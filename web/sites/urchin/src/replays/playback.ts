import type { Camera, Viewer } from "../viewer/pkg/replay_viewer.js";
import { $ } from "./dom";

const STEP_MS = 1000;
const REPEAT_DELAY_MS = 400;
const REPEAT_MS = 120;

export const bindPlayback = (
  viewer: Viewer,
  seekBy: (ms: number) => void,
  setCamera: (camera: Camera) => void,
) => {
  const stage = $("#stage");
  const play = $("#play");
  const speed = $<HTMLSelectElement>("#speed");
  const flyHint = $("#fly-hint");
  const cameras = [...document.querySelectorAll<HTMLElement>("[data-camera]")];
  let playing: boolean | undefined;
  let camera: Camera | undefined;

  const showFlyHint = () => {
    flyHint.hidden = camera !== "fly" || !!document.pointerLockElement;
  };

  const hold = (button: HTMLElement, ms: number) => {
    let timer = 0;
    let held = false;
    button.addEventListener("pointerdown", (event) => {
      if (event.button !== 0) return;
      held = false;
      timer = window.setTimeout(function repeat() {
        held = true;
        seekBy(ms);
        timer = window.setTimeout(repeat, REPEAT_MS);
      }, REPEAT_DELAY_MS);
    });
    for (const type of ["pointerup", "pointerleave", "pointercancel"]) {
      button.addEventListener(type, () => clearTimeout(timer));
    }
    button.addEventListener("click", () => {
      if (!held) seekBy(ms);
      held = false;
    });
  };

  hold($("#step-back"), -STEP_MS);
  hold($("#step-forward"), STEP_MS);
  play.addEventListener("click", () => viewer.toggle());
  speed.addEventListener("change", () => {
    viewer.setSpeed(Number(speed.value));
    if (!speed.matches(":focus-visible")) speed.blur();
  });
  for (const button of cameras) {
    button.addEventListener("click", () =>
      setCamera(button.dataset.camera as Camera),
    );
  }
  cameras[0].parentElement!.addEventListener("keydown", (event) => {
    const step =
      event.code === "ArrowRight" ? 1 : event.code === "ArrowLeft" ? -1 : 0;
    const target = event.target as HTMLElement;
    if (!step || !target.matches(":focus-visible")) return;
    event.preventDefault();
    event.stopPropagation();
    const shown = cameras.filter((button) => button.checkVisibility());
    const next = shown.at((shown.indexOf(target) + step) % shown.length)!;
    setCamera(next.dataset.camera as Camera);
    next.focus();
  });
  document.addEventListener("pointerlockchange", showFlyHint);

  return {
    stepSpeed(direction: number) {
      speed.selectedIndex = Math.min(
        Math.max(speed.selectedIndex + direction, 0),
        speed.length - 1,
      );
      viewer.setSpeed(Number(speed.value));
      return speed.selectedOptions[0].text;
    },
    update() {
      if (viewer.playing !== playing) {
        playing = viewer.playing;
        play.toggleAttribute("data-playing", playing);
        play.ariaLabel = playing ? "Pause" : "Play";
      }
      if (viewer.camera !== camera) {
        camera = viewer.camera;
        stage.dataset.camera = camera;
        for (const button of cameras) {
          const checked = button.dataset.camera === camera;
          button.ariaChecked = String(checked);
          button.tabIndex = checked ? 0 : -1;
        }
        showFlyHint();
      }
    },
  };
};
