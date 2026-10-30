import { countUntil, h } from "./dom";
import type { EventKind } from "./events";
import { isOut, stateOf, type Match, type Player } from "./match";
import { coloured } from "./mctext";
import { nickTag } from "./nicks";
import { face } from "./skins";

const STATS: EventKind[][] = [["kill", "final"], ["final"], ["bed"]];

export const showRoster = (
  list: HTMLElement,
  match: Match,
  follow: (index: number) => void,
) => {
  const rows = match.roster.map((player) => {
    const stats = STATS.map((kinds) => ({
      cell: h("span", { textContent: "0" }),
      times: match.events.filter(
        (event) =>
          kinds.includes(event.kind) &&
          "actor" in event &&
          event.actor === player,
      ),
    }));
    const element = h(
      "button",
      {
        type: "button",
        className: "player",
        onclick: () => follow(player.index),
      },
      face(player.skinHash),
      h(
        "span",
        { className: "name mc" },
        player.username,
        h("span", { className: "nick", textContent: nickTag(player) }),
      ),
      h("span", { className: "stats" }, ...stats.map(({ cell }) => cell)),
    );
    element.style.setProperty("--color", player.team.color);
    return { player, element, stats };
  });

  const teamStatus = (players: Player[], time: number) => {
    const { team } = players[0];
    const left = players.filter((player) => !isOut(match, player, time)).length;
    const happened = (kind: "bed" | "eliminated") =>
      match.events.some(
        (event) =>
          event.kind === kind && event.team === team && event.t <= time,
      );
    if (!match.bedWars || !team.known) return { status: "", mark: "", tip: "" };
    if (happened("eliminated") || (happened("bed") && !left)) {
      return { status: "out", mark: "✗", tip: "Eliminated" };
    }
    return happened("bed")
      ? {
          status: "bedless",
          mark: String(left),
          tip: `Bed gone, ${left} player${left === 1 ? "" : "s"} left`,
        }
      : { status: "alive", mark: "✓", tip: "Bed intact" };
  };

  const teams = [
    ...Map.groupBy(rows, ({ player }) => player.team.name).values(),
  ];
  const grouped =
    teams.length > 1 && teams.some((members) => members.length > 1);
  const groups = (grouped ? teams : []).map((members) => {
    const players = members.map(({ player }) => player);
    const { team } = players[0];
    const state = h("span", { className: "state" });
    const element = h(
      "section",
      { className: "team", ariaLabel: `${team.name} team` },
      h(
        "p",
        { className: "team-head mc" },
        coloured(team.name[0], team.color),
        ` ${team.name}: `,
        state,
      ),
      ...members.map(({ element }) => element),
    );
    element.style.setProperty("--color", team.color);
    return {
      element,
      update(time: number) {
        const { status, mark, tip } = teamStatus(players, time);
        element.dataset.status = status;
        state.textContent = mark;
        state.dataset.tip = tip;
      },
    };
  });
  list.replaceChildren(
    ...(grouped ? groups : rows).map(({ element }) => element),
  );

  let shownWatched: number | undefined;
  return (time: number, presence: Uint8Array, watched?: number) => {
    if (watched !== shownWatched) {
      shownWatched = watched;
      const row = rows.find(({ player }) => player.index === watched)?.element;
      if (row?.checkVisibility({ visibilityProperty: true })) {
        row.scrollIntoView({ block: "nearest" });
      }
    }
    for (const { player, element, stats } of rows) {
      const state = stateOf(match, player, presence, time);
      const [kills, finals, beds] = stats.map(
        ({ cell, times }) =>
          (cell.textContent = String(countUntil(times, time))),
      );
      element.dataset.state = state;
      element.ariaLabel = `${player.username}${nickTag(player)}, ${kills} kills, ${finals} finals, ${beds} beds${state === "alive" ? "" : `, ${state}`}`;
      element.ariaCurrent = player.index === watched ? "true" : null;
    }
    for (const group of groups) group.update(time);
  };
};
