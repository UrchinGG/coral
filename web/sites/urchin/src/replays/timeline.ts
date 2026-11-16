import { $, clock } from "./dom";
import type { Match } from "./match";

export const bindTimeline = (seek: (time: number) => void) => {
  const track = $("#scrubber");
  const tip = $("#scrub-tip");
  let duration = 1;
  let pendingSeek: number | undefined;
  let pressed = false;

  const fraction = (clientX: number) => {
    const { left, width } = track.getBoundingClientRect();
    return Math.min(Math.max((clientX - left) / width, 0), 1);
  };

  const seekAt = (clientX: number) => {
    if (pendingSeek === undefined) {
      requestAnimationFrame(() => {
        seek(pendingSeek!);
        pendingSeek = undefined;
      });
    }
    pendingSeek = fraction(clientX) * duration;
  };

  const hover = (event: PointerEvent) => {
    const x = fraction(event.clientX);
    track.style.setProperty("--hover", String(x));
    tip.textContent = clock(x * duration);
    tip.hidden = false;
    const room = track.clientWidth - tip.offsetWidth;
    tip.style.translate = `${Math.min(Math.max(x * track.clientWidth - tip.offsetWidth / 2, 0), room)}px`;
  };

  const leave = () => {
    tip.hidden = true;
    track.style.removeProperty("--hover");
  };

  const release = () => {
    pressed = false;
    track.toggleAttribute("data-dragging", false);
  };

  track.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    track.setPointerCapture(event.pointerId);
    pressed = true;
    track.toggleAttribute("data-dragging", true);
    seekAt(event.clientX);
  });
  track.addEventListener("pointermove", (event) => {
    hover(event);
    if (pressed) seekAt(event.clientX);
  });
  track.addEventListener("pointerup", (event) => {
    release();
    if (event.pointerType !== "mouse") leave();
  });
  track.addEventListener("pointercancel", () => {
    release();
    leave();
  });
  track.addEventListener("pointerleave", () => {
    if (!pressed) leave();
  });

  return (match: Match) => {
    duration = match.info.durationMs;
    track.ariaValueMax = String(Math.round(duration));
    $("#duration").textContent = clock(duration);
    let second = -1;
    return (time: number) => {
      track.style.setProperty("--progress", String(time / duration));
      if (Math.floor(time / 1000) === second) return;
      second = Math.floor(time / 1000);
      track.ariaValueNow = String(Math.round(time));
      track.ariaValueText = `${clock(time)} of ${clock(duration)}`;
      $("#time").textContent = clock(time);
    };
  };
};
