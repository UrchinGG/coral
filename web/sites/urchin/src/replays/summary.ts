import { $, h } from "./dom";
import type { Match } from "./match";

const LOCAL_TIME = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});
const NEW_YORK = new Intl.DateTimeFormat("en-US", {
  timeZone: "America/New_York",
  hourCycle: "h23",
  year: "numeric",
  month: "numeric",
  day: "numeric",
  hour: "numeric",
  minute: "numeric",
});

const hypixelTime = (date: string, time: string) => {
  const day = /^(\d{1,2})\/(\d{1,2})\/(\d{4})$/.exec(date);
  const hour = /^(\d{1,2}):(\d{2})/.exec(time);
  if (!day || !hour) return;
  const wall = Date.UTC(+day[3], +day[1] - 1, +day[2], +hour[1], +hour[2]);
  let instant = wall + 5 * 3600000;
  for (let pass = 0; pass < 2; pass++) {
    const parts = Object.fromEntries(
      NEW_YORK.formatToParts(instant).map(({ type, value }) => [type, +value]),
    );
    instant +=
      wall -
      Date.UTC(
        parts.year,
        parts.month - 1,
        parts.day,
        parts.hour,
        parts.minute,
      );
  }
  return new Date(instant);
};

export const showSummary = (
  match: Match,
  name: string,
  bytes: Uint8Array<ArrayBuffer>,
) => {
  const { game, mode, map, date, time, players } = match.info;
  const heading = [game, mode, map].filter(Boolean).join(" · ") || name;
  const stamp = [date, time].filter(Boolean).join(" ");
  const played = date && time ? hypixelTime(date, time) : undefined;
  const when = played ? LOCAL_TIME.format(played) : stamp;

  document.title = `${heading} — Urchin`;
  $("#title").textContent = [game, mode].filter(Boolean).join(" ") || name;
  const subtitle = $("#subtitle");
  subtitle.replaceChildren(
    ...(map ? [h("span", { className: "map", textContent: map })] : []),
    h("span", {
      className: "details",
      textContent: [`${players.length} players`, when]
        .filter(Boolean)
        .join(" · "),
    }),
  );
  subtitle.title = played ? `On Hypixel: ${stamp}` : "";

  const download = $<HTMLAnchorElement>("#download");
  URL.revokeObjectURL(download.href);
  download.href = URL.createObjectURL(new Blob([bytes]));
  download.download = name;
  download.hidden = false;
};

export const showLoadingTitle = () => {
  const title = $("#title");
  const subtitle = $("#subtitle");
  const shown = {
    title: title.textContent,
    details: [...subtitle.childNodes],
    tip: subtitle.title,
  };
  title.textContent = "Loading replay";
  subtitle.replaceChildren();
  subtitle.title = "";
  return () => {
    title.textContent = shown.title;
    subtitle.replaceChildren(...shown.details);
    subtitle.title = shown.tip;
  };
};
