import type { Metadata } from "next";
import { Section, P, Mono, Signature } from "@/components/starfish/DocsKit";

export const metadata: Metadata = {
  title: "Display - Starfish Docs",
  description: "Tab list display API reference for Starfish plugins.",
};

export default function DisplayPage() {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">Display</h1>
      <p className="text-sm text-white/40 mb-10">Tab-list name prefixes, suffixes, and header text. Every function scopes to the calling plugin's own contribution — multiple plugins can modify the same player without overwriting each other.</p>

      <Section title="Prefix / suffix">
        <Signature>{"starfish.display.setPrefix(uuid, value, opts?) -> nil"}</Signature>
        <Signature>{"starfish.display.appendPrefix(uuid, value, opts?) -> nil"}</Signature>
        <Signature>{"starfish.display.prependPrefix(uuid, value, opts?) -> nil"}</Signature>
        <Signature>{"starfish.display.clearPrefix(uuid) -> nil"}</Signature>
        <P>Set, append to, prepend to, or clear this plugin's prefix contribution for a player. <Mono>opts.priority</Mono> controls ordering against other plugins' contributions.</P>
        <Signature>{"starfish.display.setSuffix(uuid, value, opts?) -> nil"}</Signature>
        <Signature>{"starfish.display.appendSuffix(uuid, value, opts?) -> nil"}</Signature>
        <Signature>{"starfish.display.prependSuffix(uuid, value, opts?) -> nil"}</Signature>
        <Signature>{"starfish.display.clearSuffix(uuid) -> nil"}</Signature>
        <P>Same, for the suffix.</P>
      </Section>

      <Section title="Reading">
        <Signature>{"starfish.display.prefix(uuid) -> string"}</Signature>
        <Signature>{"starfish.display.suffix(uuid) -> string"}</Signature>
        <P>Combined (all-plugins) rendered prefix/suffix for a player.</P>
        <Signature>{"starfish.display.othersPrefix(uuid) -> string"}</Signature>
        <Signature>{"starfish.display.othersSuffix(uuid) -> string"}</Signature>
        <P>Combined prefix/suffix contributed by every plugin except the caller.</P>
        <Signature>{"starfish.display.isModified(uuid) -> boolean"}</Signature>
        <P>Whether any plugin has modified this player's display name.</P>
      </Section>

      <Section title="Tab-list removal">
        <Signature>{"starfish.display.holdRemoval(uuid) -> nil"}</Signature>
        <P>Keeps a player's tab-list entry alive past the server's own removal, until released.</P>
        <Signature>{"starfish.display.releaseRemoval(uuid) -> nil"}</Signature>
        <P>Releases this plugin's hold. Once the last holder releases, the withheld removal replays.</P>
      </Section>

      <Section title="Header">
        <Signature>{"starfish.display.setTabHeaderAppend(text) -> nil"}</Signature>
        <Signature>{"starfish.display.clearTabHeaderAppend() -> nil"}</Signature>
        <P>Set or clear text this plugin appends to the tab-list header.</P>
      </Section>

      <Section title="Clearing">
        <Signature>{"starfish.display.clear(uuid) -> nil"}</Signature>
        <P>Clears both prefix and suffix contributions for a player.</P>
        <Signature>{"starfish.display.clearAll() -> nil"}</Signature>
        <P>Clears this plugin's contributions for every player it has modified.</P>
      </Section>
    </div>
  );
}
