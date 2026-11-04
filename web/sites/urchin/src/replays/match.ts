import type { ReplayInfo, ReplayPlayer } from "../viewer/pkg/replay_viewer.js";
import { readEvents, type GameEvent } from "./events";
import { denick, type Nick } from "./nicks";
import { teamDyed, type Team } from "./teams";

export interface Player extends ReplayPlayer {
  team: Team;
  nick?: Nick;
}

export interface Match {
  info: ReplayInfo;
  players: Player[];
  roster: Player[];
  events: GameEvent[];
  bedWars: boolean;
}

export type PlayerState = "alive" | "dead" | "out";

export const readMatch = (info: ReplayInfo, nickSkins: Set<string>): Match => {
  const players = info.players.map((player) => ({
    ...player,
    ...denick(player, nickSkins),
    team: teamDyed(player.teamColor),
  }));
  const events = readEvents(info.chat, players);
  return {
    info,
    players,
    roster: players.toSorted(
      (a, b) =>
        a.team.order - b.team.order || a.username.localeCompare(b.username),
    ),
    events,
    bedWars:
      events.some(({ kind }) => kind === "bed") ||
      /bed ?wars/i.test(info.game ?? ""),
  };
};

export const isOut = (match: Match, player: Player, time: number) =>
  match.events.some(
    (event) =>
      event.kind === "final" && event.victim === player && event.t <= time,
  );

export const stateOf = (
  match: Match,
  player: Player,
  presence: Uint8Array,
  time: number,
): PlayerState =>
  isOut(match, player, time)
    ? "out"
    : presence[player.index]
      ? "alive"
      : "dead";

export const successorOf = (
  match: Match,
  player: Player,
  presence: Uint8Array,
  time: number,
) => {
  const final = match.events.findLast(
    (event) =>
      event.kind === "final" && event.victim === player && event.t <= time,
  );
  const killer = final?.kind === "final" ? final.actor : undefined;
  const teammates = match.roster.filter(({ team }) => team === player.team);
  const from = match.roster.indexOf(player);
  const following = [
    ...match.roster.slice(from + 1),
    ...match.roster.slice(0, from),
  ];
  return [killer, ...teammates, ...following].find(
    (candidate) =>
      candidate &&
      candidate !== player &&
      stateOf(match, candidate, presence, time) === "alive",
  );
};
