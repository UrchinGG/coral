import type { Metadata } from "next";
import { Section, P, Mono, Signature } from "@/components/starfish/DocsKit";

export const metadata: Metadata = {
  title: "Text - Starfish Docs",
  description: "Text component API reference for Starfish plugins.",
};

export default function TextPage() {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">Text</h1>
      <p className="text-sm text-white/40 mb-10">Build rich text components — used wherever chat or overlay text accepts click/hover formatting.</p>

      <Section title="Building">
        <Signature>{"starfish.text.of(s) -> Component"}</Signature>
        <P>Builds a component from a legacy <Mono>§</Mono>-coded string.</P>
        <Signature>{"starfish.text.empty() -> Component"}</Signature>
        <Signature>{"starfish.text.join(parts, sep?) -> Component"}</Signature>
        <P>Concatenates components, inserting <Mono>sep</Mono> (default <Mono>{'""'}</Mono>) between them.</P>
      </Section>

      <Section title="Converting">
        <Signature>{"starfish.text.legacy(component) -> string"}</Signature>
        <P>Flattens to a legacy <Mono>§</Mono>-coded string. Loses click/hover.</P>
        <Signature>{"starfish.text.plain(s) -> string"}</Signature>
        <P>Strips all <Mono>§x</Mono> formatting codes, leaving readable text.</P>
        <Signature>{"starfish.text.json(component) -> string"}</Signature>
        <Signature>{"starfish.text.fromJson(s) -> Component"}</Signature>
        <P>Serialize to/parse from wire-format JSON. Preserves click/hover.</P>
      </Section>

      <Section title="Component methods">
        <P>Every method below returns a new component (originals are not mutated) and can be chained.</P>
        <Signature>{"c:color(color) -> Component"}</Signature>
        <Signature>{"c:bold() -> Component"}</Signature>
        <Signature>{"c:italic() -> Component"}</Signature>
        <Signature>{"c:underline() -> Component"}</Signature>
        <Signature>{"c:strikethrough() -> Component"}</Signature>
        <Signature>{"c:obfuscated() -> Component"}</Signature>
        <Signature>{"c:hover(value) -> Component"}</Signature>
        <P>Sets a text hover event.</P>
        <Signature>{"c:click(opts) -> Component"}</Signature>
        <P><Mono>opts.action</Mono>: <Mono>runCommand | suggestCommand | openUrl | copyToClipboard</Mono>, <Mono>opts.value</Mono>.</P>
        <Signature>{"c:run(cmd) -> Component"}</Signature>
        <Signature>{"c:suggest(cmd) -> Component"}</Signature>
        <Signature>{"c:url(url) -> Component"}</Signature>
        <Signature>{"c:copy(value) -> Component"}</Signature>
        <P>Shorthand for <Mono>click</Mono> with the matching action.</P>
        <Signature>{"c:append(other) -> Component"}</Signature>
        <P>Same as <Mono>{"c .. other"}</Mono> — both push <Mono>other</Mono> onto <Mono>extra</Mono>.</P>
      </Section>
    </div>
  );
}
