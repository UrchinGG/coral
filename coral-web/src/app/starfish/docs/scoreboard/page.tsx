import type { Metadata } from "next";
import { Section, P, Mono, Signature, PropTable } from "@/components/starfish/DocsKit";

export const metadata: Metadata = {
  title: "Scoreboard - Starfish Docs",
  description: "Scoreboard API reference for Starfish plugins.",
};

export default function ScoreboardPage() {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">Scoreboard</h1>
      <p className="text-sm text-white/40 mb-10">Read teams, objectives, and scores.</p>

      <Section title="Teams">
        <Signature>{"starfish.scoreboard.teams() -> Team[]"}</Signature>
        <Signature>{"starfish.scoreboard.team(name) -> Team | nil"}</Signature>
        <PropTable rows={[
          ["name", "string", ""],
          ["displayName", "string", ""],
          ["prefix", "string", ""],
          ["suffix", "string", ""],
          ["color", "string | nil", ""],
          ["nameTagVisibility", "string", ""],
          ["players", "string[]", ""],
        ]} />
      </Section>

      <Section title="Objectives and scores">
        <Signature>{"starfish.scoreboard.objectives() -> Objective[]"}</Signature>
        <Signature>{"starfish.scoreboard.objective(name) -> Objective | nil"}</Signature>
        <PropTable rows={[
          ["name", "string", ""],
          ["displayName", "string", ""],
          ["type", "string", ""],
        ]} />
        <Signature>{"starfish.scoreboard.scores(objectiveName) -> {name, score}[]"}</Signature>
        <P>Sorted highest-first.</P>
        <Signature>{"starfish.scoreboard.score(objectiveName, entry) -> number | nil"}</Signature>
      </Section>

      <Section title="Display slots">
        <Signature>{"starfish.scoreboard.sidebar() -> Sidebar | nil"}</Signature>
        <P>Snapshot of the objective in the sidebar slot: <Mono>{"{title, lines}"}</Mono> (<Mono>lines</Mono> sorted highest-first).</P>
        <Signature>{"starfish.scoreboard.displayed(slot) -> Objective | nil"}</Signature>
        <P><Mono>slot</Mono> must be <Mono>list | sidebar | belowName</Mono>.</P>
      </Section>
    </div>
  );
}
