import { $, clock, countUntil, h, safely } from "./dom";
import { feedText, type EventKind, type GameEvent } from "./events";
import type { Match } from "./match";
import { mcText } from "./mctext";
import { showRoster } from "./roster";

type Tab = "players" | "events" | "chat";

interface Row {
  t: number;
  element: HTMLElement;
}

const HAND_SCROLL_MS = 5000;
const TAB_KEY = "replay.inspector-tab";
const EVENT_KINDS_KEY = "replay.event-kinds";

const timedRow = (t: number, onclick: () => void, ...text: (Node | string)[]) =>
  h(
    "button",
    { type: "button", className: "row", onclick },
    h("time", { textContent: clock(t) }),
    h("span", { className: "mc" }, ...text),
  );

const dimUpcoming = (rows: Row[]) => {
  let reached = 0;
  for (const { element } of rows) element.classList.add("upcoming");
  return (time: number) => {
    const next = countUntil(rows, time);
    for (
      let index = Math.min(reached, next);
      index < Math.max(reached, next);
      index++
    ) {
      rows[index].element.classList.toggle("upcoming", index >= next);
    }
    reached = next;
    return next;
  };
};

export const bindPanel = (actions: {
  seekBefore: (time: number) => void;
  jump: (event: GameEvent) => void;
  follow: (index: number) => void;
  step: (direction: 1 | -1) => void;
}) => {
  const panel = $("#match");
  const stage = $("#stage");
  const toggle = $("#panel-toggle");
  const chatView = $("#chat");
  const tabs = [...panel.querySelectorAll<HTMLElement>("[role=tab]")];
  const chips = [...panel.querySelectorAll<HTMLElement>("[data-kind]")];
  const savedKinds = safely(() =>
    JSON.parse(localStorage.getItem(EVENT_KINDS_KEY) ?? "null"),
  );
  const kinds = new Set<EventKind>(
    Array.isArray(savedKinds)
      ? savedKinds
      : chips
          .filter((chip) => chip.ariaPressed === "true")
          .map((chip) => chip.dataset.kind as EventKind),
  );
  const savedTab = safely(() => localStorage.getItem(TAB_KEY));
  let pinned = matchMedia("(width > 56.25rem)").matches;
  let events: (Row & { event: GameEvent })[] = [];
  let scrolledAt = -Infinity;

  const setOpen = (open: boolean) => {
    if (!open && panel.contains(document.activeElement)) toggle.focus();
    panel.inert = !open;
    stage.toggleAttribute("data-panel", open);
    toggle.ariaExpanded = String(open);
  };

  const select = (tab: Tab) => {
    for (const button of tabs) {
      const selected = button.dataset.tab === tab;
      button.ariaSelected = String(selected);
      button.tabIndex = selected ? 0 : -1;
      $(`#${button.dataset.tab}`).hidden = !selected;
    }
    scrolledAt = -Infinity;
    safely(() => localStorage.setItem(TAB_KEY, tab));
  };

  const togglePinned = () => {
    pinned = panel.inert;
    setOpen(pinned);
  };

  const filter = () => {
    for (const { event, element } of events)
      element.hidden = !kinds.has(event.kind);
    const shown = events.filter(({ element }) => !element.hidden).length;
    $("#event-count").textContent = String(shown);
    const empty = $("#event-empty");
    empty.hidden = shown > 0;
    empty.textContent = events.length
      ? "Nothing matches the filters."
      : "No kills, beds or final kills in this replay.";
  };

  setOpen(pinned);
  select(
    tabs.some((tab) => tab.dataset.tab === savedTab)
      ? (savedTab as Tab)
      : "players",
  );
  toggle.addEventListener("click", togglePinned);
  for (const tab of tabs) {
    tab.addEventListener("click", () => select(tab.dataset.tab as Tab));
  }
  tabs[0].parentElement!.addEventListener("keydown", (event) => {
    const step =
      event.code === "ArrowRight" ? 1 : event.code === "ArrowLeft" ? -1 : 0;
    if (!step) return;
    event.stopPropagation();
    const next = tabs.at(
      (tabs.indexOf(event.target as HTMLElement) + step) % tabs.length,
    )!;
    select(next.dataset.tab as Tab);
    next.focus();
  });
  for (const chip of chips) {
    const kind = chip.dataset.kind as EventKind;
    chip.ariaPressed = String(kinds.has(kind));
    chip.addEventListener("click", () => {
      if (!kinds.delete(kind)) kinds.add(kind);
      chip.ariaPressed = String(kinds.has(kind));
      safely(() =>
        localStorage.setItem(EVENT_KINDS_KEY, JSON.stringify([...kinds])),
      );
      filter();
    });
  }
  $("#event-prev").addEventListener("click", () => actions.step(-1));
  $("#event-next").addEventListener("click", () => actions.step(1));
  const scrolledByHand = () => {
    scrolledAt = performance.now();
  };
  for (const type of ["wheel", "pointerdown", "touchmove", "keydown"]) {
    chatView.addEventListener(type, scrolledByHand, { passive: true });
  }

  return {
    select,
    shownEvents: () =>
      events.filter(({ element }) => !element.hidden).map(({ event }) => event),
    show(match: Match) {
      const roster = showRoster($("#roster"), match, actions.follow);

      events = match.events.map((event) => ({
        t: event.t,
        event,
        element: timedRow(
          event.t,
          () => actions.jump(event),
          ...feedText(event),
        ),
      }));
      $("#event-list").replaceChildren(...events.map(({ element }) => element));
      filter();

      const lines = match.info.chat.map(({ t, segments }) => ({
        t,
        element: timedRow(t, () => actions.seekBefore(t), ...mcText(segments)),
      }));
      $("#chat-log").replaceChildren(...lines.map(({ element }) => element));
      $("#chat-empty").hidden = lines.length > 0;

      const dimEvents = dimUpcoming(events);
      const dimLines = dimUpcoming(lines);
      return (time: number, presence: Uint8Array, watched?: number) => {
        roster(time, presence, watched);
        dimEvents(time);
        const reached = dimLines(time);
        if (
          !panel.inert &&
          !chatView.hidden &&
          performance.now() - scrolledAt > HAND_SCROLL_MS
        ) {
          lines[reached - 1]?.element.scrollIntoView({ block: "nearest" });
        }
      };
    },
  };
};
