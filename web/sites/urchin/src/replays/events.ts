import type { ChatLine } from "../viewer/pkg/replay_viewer.js";
import type { Player } from "./match";
import { coloured, withoutPictographs } from "./mctext";
import { teamNamed, type Team } from "./teams";

export type GameEvent =
  | { t: number; kind: "kill"; victim: Player; actor: Player }
  | { t: number; kind: "final"; victim: Player; actor?: Player }
  | { t: number; kind: "bed"; team: Team; actor?: Player }
  | { t: number; kind: "eliminated"; team: Team };

export type EventKind = GameEvent["kind"];

export const EVENT_LABELS: Record<EventKind, string> = {
  kill: "Kill",
  final: "Final kill",
  bed: "Bed broken",
  eliminated: "Team eliminated",
};

const BED = /^BED DESTRUCTION > (\w+) Bed\b/;
const ELIMINATED = /^TEAM ELIMINATED > (\w+) Team has been eliminated/;
const FINAL = /^(?:\[[^\]]+\] )?(\S+) (.*?)FINAL KILL!?$/;
const DEATH = /\b(?:was|fell|slipped|be)\b/i;

export const readEvents = (chat: ChatLine[], players: Player[]) => {
  const patterns = players.map((player) => ({
    player,
    pattern: new RegExp(
      `(?<!\\w)${RegExp.escape(player.username)}(?!\\w)`,
      "gi",
    ),
  }));
  const named = new Map(
    players.map((player) => [player.username.toLowerCase(), player]),
  );

  const read = ({ t, text }: ChatLine): GameEvent | undefined => {
    const line = withoutPictographs(text)
      .replace(/\s{2,}/g, " ")
      .trim();
    const mentions = patterns
      .flatMap(({ player, pattern }) =>
        Array.from(line.matchAll(pattern), ({ index }) => ({ player, index })),
      )
      .sort((a, b) => a.index - b.index);
    const last = (skip?: Player) =>
      mentions.findLast(({ player }) => player !== skip)?.player;

    const bed = BED.exec(line);
    if (bed) return { t, kind: "bed", team: teamNamed(bed[1]), actor: last() };
    const eliminated = ELIMINATED.exec(line);
    if (eliminated)
      return { t, kind: "eliminated", team: teamNamed(eliminated[1]) };
    const final = FINAL.exec(line);
    if (final) {
      const victim = named.get(final[1].toLowerCase());
      return victim && { t, kind: "final", victim, actor: last(victim) };
    }
    const [first] = mentions;
    const actor = first && last(first.player);
    if (actor && first.index <= 2 && DEATH.test(line)) {
      return { t, kind: "kill", victim: first.player, actor };
    }
  };

  const events = chat.flatMap((line) => read(line) ?? []);
  return [...events, ...eliminations(events, players)].sort(
    (a, b) => a.t - b.t,
  );
};

const eliminations = (events: GameEvent[], players: Player[]): GameEvent[] => {
  const announced = new Set(
    events.flatMap((event) =>
      event.kind === "eliminated" ? [event.team] : [],
    ),
  );
  const teams = Map.groupBy(
    players.filter(({ team }) => team.known && !announced.has(team)),
    ({ team }) => team,
  );
  return [...teams].flatMap(([team, members]) => {
    const bed = events.find(
      (event) => event.kind === "bed" && event.team === team,
    );
    const lastFinal = Math.max(
      ...members.map(
        (member) =>
          events.find(
            (event) => event.kind === "final" && event.victim === member,
          )?.t ?? Infinity,
      ),
    );
    if (!bed || lastFinal === Infinity) return [];
    return [{ t: Math.max(bed.t, lastFinal), kind: "eliminated", team }];
  });
};

export const involved = (event: GameEvent) =>
  event.kind === "eliminated"
    ? undefined
    : event.kind === "bed"
      ? event.actor
      : (event.actor ?? event.victim);

export const feedText = (event: GameEvent) => {
  const player = ({ username, team }: Player) => coloured(username, team.color);
  const team = ({ name, color }: Team) => coloured(name, color);
  switch (event.kind) {
    case "kill":
      return [player(event.actor), " killed ", player(event.victim)];
    case "final":
      return event.actor
        ? [player(event.actor), " final killed ", player(event.victim)]
        : [player(event.victim), " took a final death"];
    case "bed":
      return event.actor
        ? [player(event.actor), " broke ", team(event.team), " bed"]
        : [team(event.team), " bed was broken"];
    case "eliminated":
      return [team(event.team), " team was eliminated"];
  }
};
