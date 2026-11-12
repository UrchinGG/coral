import { $, countUntil, h } from "./dom";
import type { Match } from "./match";
import { mcText } from "./mctext";

const LIFETIME_MS = 10000;
const FADE_MS = 2000;
const RESTING_LINES = 6;
const WHEEL_LINES = 3;

export const bindHud = (onClose: () => void) => {
  const hud = $("#hud");
  const log = $("#hud-log");

  const isOpen = () => hud.hasAttribute("data-open");

  const close = () => {
    hud.toggleAttribute("data-open", false);
    onClose();
  };

  addEventListener("keydown", (event) => {
    if (!isOpen() || (event.code !== "PageUp" && event.code !== "PageDown")) {
      return;
    }
    event.preventDefault();
    log.scrollBy({
      top: (event.code === "PageUp" ? -0.5 : 0.5) * log.clientHeight,
    });
  });

  addEventListener(
    "wheel",
    (event) => {
      if (!isOpen()) return;
      event.preventDefault();
      const line = parseFloat(getComputedStyle(log).lineHeight);
      log.scrollBy({ top: Math.sign(event.deltaY) * WHEEL_LINES * line });
    },
    { passive: false },
  );

  return {
    isOpen,
    close,
    open() {
      hud.toggleAttribute("data-open", true);
      log.scrollTop = 0;
    },
    show(match: Match) {
      const chat = match.info.chat;
      const lines = chat.map(({ segments }) =>
        h("p", { className: "line", hidden: true }, ...mcText(segments)),
      );
      $("#hud-lines").replaceChildren(...lines);
      let shown = [0, 0];
      let end = 0;

      const recentFrom = (time: number) => {
        let start = Math.max(end - RESTING_LINES, 0);
        while (start < end && chat[start].t < time - LIFETIME_MS) start++;
        return start;
      };

      return (time: number) => {
        end = countUntil(chat, time);
        const start = isOpen() ? 0 : recentFrom(time);
        const [from, to] = shown;
        for (const [a, b] of [
          [from, start],
          [to, end],
        ]) {
          for (let index = Math.min(a, b); index < Math.max(a, b); index++) {
            lines[index].hidden = index < start || index >= end;
          }
        }
        shown = [start, end];
        if (isOpen()) return;
        for (let index = start; index < end; index++) {
          const fade = (chat[index].t + LIFETIME_MS - time) / FADE_MS;
          lines[index].style.setProperty("--fade", String(Math.min(fade, 1)));
        }
      };
    },
  };
};
