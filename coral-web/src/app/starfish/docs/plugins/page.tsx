import type { Metadata } from "next";
import { Section, P, Mono, Signature, PropTable } from "@/components/starfish/DocsKit";

export const metadata: Metadata = {
  title: "Plugins - Starfish Docs",
  description: "Inter-plugin API and capability reference for Starfish plugins.",
};

export default function PluginsPage() {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">Plugins</h1>
      <p className="text-sm text-white/40 mb-10">Export functions for other plugins to call, and call into theirs.</p>

      <Section title="starfish.plugin">
        <P>This plugin's own identity and exports.</P>
        <Signature>{"starfish.plugin.export(name, func) -> nil"}</Signature>
        <P>Registers <Mono>func</Mono> under <Mono>name</Mono>, callable by dependents via <Mono>plugins.call</Mono>/<Mono>require</Mono>/<Mono>optional</Mono>.</P>
        <Signature>{"starfish.plugin.unexport(name) -> nil"}</Signature>
        <Signature>{"starfish.plugin.list() -> string[]"}</Signature>
        <Signature>{"starfish.plugin.name() -> string"}</Signature>
        <Signature>{"starfish.plugin.version() -> string"}</Signature>
        <Signature>{"starfish.plugin.manifest() -> table"}</Signature>
        <PropTable rows={[
          ["name", "string", ""],
          ["version", "string", ""],
          ["displayName", "string", ""],
          ["prefix", "string", ""],
          ["author", "string", ""],
          ["credits", "string", ""],
          ["description", "string", ""],
          ["dependencies", "string[]", ""],
          ["requires", "string[]", "Declared capability requirements"],
        ]} />
      </Section>

      <Section title="starfish.plugins">
        <P>Calling into other plugins.</P>
        <Signature>{"starfish.plugins.call(pluginName, funcName, ...) -> ..."}</Signature>
        <P>Depth-guarded (max 100), dependency-checked cross-VM call into another plugin's export. Args/results are JSON-marshalled between Lua VMs. Errors if the target plugin/function doesn't exist, or the dependency isn't declared.</P>
        <Signature>{"starfish.plugins.has(pluginName) -> boolean"}</Signature>
        <Signature>{"starfish.plugins.exports(pluginName) -> string[] | nil"}</Signature>
        <Signature>{"starfish.plugins.list() -> string[]"}</Signature>
        <Signature>{"starfish.plugins.require(pluginName) -> table"}</Signature>
        <P>Errors immediately unless <Mono>pluginName</Mono> is a declared dependency. Returns a proxy whose methods forward to <Mono>call</Mono>.</P>
        <Signature>{"starfish.plugins.optional(pluginName) -> table | nil"}</Signature>
        <P>Like <Mono>require</Mono>, but returns <Mono>nil</Mono> instead of erroring if not a declared dependency. The proxy's calls resolve to <Mono>nil</Mono> if the target isn't loaded or hasn't exported that function.</P>
      </Section>

      <Section title="starfish.capabilities">
        <Signature>{"starfish.capabilities.has(name) -> boolean"}</Signature>
        <Signature>{"starfish.capabilities.get(name) -> value | nil"}</Signature>
        <Signature>{"starfish.capabilities.require(name) -> nil"}</Signature>
        <P>Errors if the capability isn't present, otherwise no-op.</P>
      </Section>
    </div>
  );
}
