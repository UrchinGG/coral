import type { Metadata } from "next";
import { Section, P, Mono, Signature, PropTable } from "@/components/starfish/DocsKit";

export const metadata: Metadata = {
  title: "Players - Starfish Docs",
  description: "Player tracking API reference for Starfish plugins.",
};

export default function PlayersPage() {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">Players</h1>
      <p className="text-sm text-white/40 mb-10">Read tab-list players and the local player.</p>

      <Section title="Lookup">
        <Signature>{"starfish.players.all() -> Player[]"}</Signature>
        <P>Every tracked (tab-list) player.</P>
        <Signature>{"starfish.players.byName(name) -> Player | nil"}</Signature>
        <Signature>{"starfish.players.byUuid(uuid) -> Player | nil"}</Signature>
        <Signature>{"starfish.players.count() -> number"}</Signature>
        <Signature>{"starfish.players.me() -> LocalPlayer | nil"}</Signature>
        <P>The live local player handle. <Mono>nil</Mono> until the local uuid/name are known.</P>
        <Signature>{"starfish.players.distance(a, b) -> number"}</Signature>
        <P>Euclidean distance between two positions/entities/players.</P>
      </Section>

      <Section title="Player fields">
        <PropTable rows={[
          ["uuid", "string", ""],
          ["name", "string", ""],
          ["displayName", "string", ""],
          ["ping", "number", ""],
          ["gamemode", "string", ""],
          ["team", "Team | nil", ""],
          ["entityId", "number | nil", ""],
          ["properties", "table", "Keyed by property name, each {name, value, signature?}"],
        ]} />
      </Section>

      <Section title="LocalPlayer fields">
        <P>Returned by <Mono>players.me()</Mono>. Live/reactive, not a snapshot.</P>
        <PropTable rows={[
          ["uuid", "string | nil", ""],
          ["name", "string | nil", ""],
          ["displayName", "string | nil", "Falls back to name"],
          ["team", "Team | nil", ""],
          ["entityId", "number | nil", ""],
          ["position", "{x, y, z}", ""],
          ["rotation", "{yaw, pitch}", ""],
          ["health", "number", ""],
          ["food", "number", ""],
          ["saturation", "number", ""],
          ["heldSlot", "number", ""],
          ["experience", "{bar, level}", ""],
          ["gamemode", "string", ""],
        ]} />
      </Section>
    </div>
  );
}
