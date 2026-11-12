import type { Viewer } from "../viewer/pkg/replay_viewer.js";

type Shortcuts = Record<string, (event: KeyboardEvent) => void>;

const SEEKING = new Set(["ArrowLeft", "ArrowRight"]);

export const bindKeys = (viewer: Viewer, shortcuts: Shortcuts) => {
  addEventListener("keydown", (event) => {
    const target = event.target as HTMLElement;
    if (target.closest("input, select, textarea, dialog")) return;
    if (
      target instanceof HTMLButtonElement &&
      (event.code === "Space" || event.code === "Enter")
    ) {
      if (target.matches(":focus-visible")) return;
      target.blur();
    }
    const shortcut = shortcuts[event.code] ?? shortcuts[event.key];
    if (shortcut && !event.ctrlKey && !event.metaKey && !event.altKey) {
      event.preventDefault();
      if (!event.repeat || SEEKING.has(event.code)) shortcut(event);
    }
    viewer.input(event);
  });
  addEventListener("keyup", (event) => viewer.input(event));
  addEventListener("blur", (event) => viewer.input(event));
};
