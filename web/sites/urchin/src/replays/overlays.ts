import { $, safely } from "./dom";

const screen = $("#loading");
const bar = $("#loading-bar");
const percent = $("#loading-percent");
const GPU_NOTICE_DISMISSED = "replay.gpu-notice-dismissed";

export const loading = {
  progress(fraction: number | null) {
    screen.hidden = false;
    bar.toggleAttribute("data-indeterminate", fraction === null);
    bar.style.setProperty("--progress", String(fraction ?? 0));
    bar.ariaValueNow =
      fraction === null ? null : String(Math.round(fraction * 100));
    percent.textContent = bar.ariaValueNow ? `${bar.ariaValueNow}%` : "";
  },
  done() {
    screen.hidden = true;
  },
};

const flash = (element: HTMLElement) => {
  let timer = 0;
  return (message: string, duration: number) => {
    clearTimeout(timer);
    element.textContent = message;
    element.toggleAttribute("data-shown", true);
    timer = window.setTimeout(
      () => element.toggleAttribute("data-shown", false),
      duration,
    );
  };
};

const flashToast = flash($("#toast"));
const seekHintBox = $("#seek-hint");
const flashSeekHint = flash(seekHintBox);

export const toast = (message: string, duration = 3200) =>
  flashToast(message, duration);

export const seekHint = (
  message: string,
  side: "left" | "middle" | "right",
) => {
  seekHintBox.dataset.side = side;
  flashSeekHint(message, 450);
};

export const showError = (title: string, error: unknown) => {
  console.error(error);
  $("#error-title").textContent = title;
  $("#error-message").textContent =
    error instanceof Error ? error.message : String(error);
  $("#error").hidden = false;
};

export const hideError = () => {
  $("#error").hidden = true;
};

export const showGpuNotice = () => {
  if (safely(() => sessionStorage.getItem(GPU_NOTICE_DISMISSED))) return;
  const notice = $("#gpu-notice");
  notice.hidden = false;
  $("#gpu-notice-close").addEventListener("click", () => {
    notice.hidden = true;
    safely(() => sessionStorage.setItem(GPU_NOTICE_DISMISSED, "1"));
  });
};

export const bindTooltips = () => {
  const tip = $("#tooltip");
  const text = $("#tooltip-text");
  const key = $("#tooltip-key");
  let target: HTMLElement | undefined;
  let timer = 0;
  let hiddenAt = 0;

  const place = (anchor: HTMLElement) => {
    text.textContent = anchor.dataset.tip || anchor.ariaLabel;
    key.textContent = anchor.dataset.key ?? "";
    key.hidden = !anchor.dataset.key;
    tip.hidden = false;
    const rect = anchor.getBoundingClientRect();
    const above = rect.top - tip.offsetHeight - 8;
    const left = Math.min(
      Math.max(rect.left + (rect.width - tip.offsetWidth) / 2, 8),
      innerWidth - tip.offsetWidth - 8,
    );
    tip.style.translate = `${left}px ${above < 8 ? rect.bottom + 8 : above}px`;
  };

  const hide = () => {
    clearTimeout(timer);
    if (!tip.hidden) hiddenAt = performance.now();
    tip.hidden = true;
    target = undefined;
  };

  const show = (anchor: HTMLElement, delay: number) => {
    hide();
    target = anchor;
    timer = window.setTimeout(
      () => place(anchor),
      performance.now() - hiddenAt < 600 ? 0 : delay,
    );
  };

  const anchorOf = (event: Event) =>
    (event.target as Element).closest<HTMLElement>("[data-tip]");

  document.addEventListener("pointerover", (event) => {
    const anchor = anchorOf(event);
    if (anchor === target) return;
    if (anchor && event.pointerType === "mouse") show(anchor, 450);
    else hide();
  });
  document.addEventListener("pointerout", (event) => {
    if (!event.relatedTarget) hide();
  });
  document.addEventListener("focusin", (event) => {
    const anchor = anchorOf(event);
    if (anchor?.matches(":focus-visible")) show(anchor, 0);
  });
  for (const type of ["pointerdown", "focusout", "keydown"]) {
    document.addEventListener(type, hide);
  }
  addEventListener("blur", hide);
};
