import type { Metadata } from "next";
import { Section, P, Mono, Signature, PropTable } from "@/components/starfish/DocsKit";

export const metadata: Metadata = {
  title: "Commands - Starfish Docs",
  description: "Command registration API reference for Starfish plugins.",
};

export default function CommandsPage() {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">Commands</h1>
      <p className="text-sm text-white/40 mb-10">Register slash commands and build replies.</p>

      <Section title="Registering">
        <Signature>{"starfish.commands.register(name, options, handler) -> nil"}</Signature>
        <P>Registers a plugin-scoped subcommand under <Mono>/&lt;plugin&gt; &lt;name&gt;</Mono>.</P>
        <Signature>{"starfish.commands.registerGlobal(name, options, handler) -> nil"}</Signature>
        <P>Registers a top-level <Mono>/&lt;name&gt;</Mono> command, looked up by name alone across all plugins.</P>
        <PropTable rows={[
          ["description", "string?", "Shown in help output"],
          ["arguments", "table?", "Array of argument specs (below)"],
        ]} />
        <P>Each argument spec: <Mono>name</Mono>, <Mono>type</Mono> (<Mono>string | int | number | bool | player | choice | greedy</Mono>, default <Mono>string</Mono>), <Mono>description?</Mono>, <Mono>optional?</Mono>, <Mono>choices?</Mono> (for <Mono>choice</Mono> type). Arguments are type/arity-checked before the handler runs.</P>
        <Signature>{"starfish.commands.unregister(name) -> boolean"}</Signature>
        <P>Removes a previously registered command for the calling plugin.</P>
        <Signature>{"starfish.commands.exists(name) -> boolean"}</Signature>
        <Signature>{"starfish.commands.list() -> table"}</Signature>
        <P>Array of <Mono>{"{name, description}"}</Mono> for the calling plugin's registered commands.</P>
      </Section>

      <Section title="Handler context">
        <P>Command handlers receive a context object and may return <Mono>string | Component | UiReply | nil</Mono>.</P>
        <P>Fields: <Mono>ctx.args</Mono> (bound argument values keyed by name), <Mono>ctx.page</Mono>, <Mono>ctx.pluginName</Mono>, <Mono>ctx.commandName</Mono>.</P>
        <Signature>{"ctx:reply(value) -> nil"}</Signature>
        <P>Queues a reply (<Mono>string | Component | UiReply</Mono>). Multiple calls accumulate in order; the handler's return value, if any, is appended after these.</P>
      </Section>

      <Section title="starfish.ui">
        <Signature>{"starfish.ui.lines(lines) -> UiReply"}</Signature>
        <P>Builds a multi-line reply from an array of strings or components — each entry is one chat line.</P>
        <Signature>{"starfish.ui.page(opts) -> UiReply"}</Signature>
        <PropTable rows={[
          ["title", "string?", "Header line"],
          ["entries", "table", "Array of strings or components"],
          ["perPage", "number?", "Default 10"],
          ["page", "number?", "Default 1"],
          ["command", "string?", "Not yet used for clickable pagination"],
        ]} />
        <P>Renders as title + current page's entries + a <Mono>Page N/Total</Mono> footer. <Mono>UiReply</Mono> is opaque — only valid as a command return value or <Mono>ctx:reply(...)</Mono> argument.</P>
      </Section>
    </div>
  );
}
