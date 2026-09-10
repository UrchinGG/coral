import Link from "next/link";
import type { Metadata } from "next";
import { Section, P, Mono, Signature } from "@/components/starfish/DocsKit";

export const metadata: Metadata = {
  title: "Docs",
  description: "Plugin API reference for Starfish.",
};

const SECTIONS: { href: string; title: string; description: string }[] = [
  { href: "/docs/chat", title: "Chat", description: "Intercept, send, and format chat messages." },
  { href: "/docs/commands", title: "Commands", description: "Register slash commands and build replies." },
  { href: "/docs/display", title: "Display", description: "Tab-list name prefixes, suffixes, and header text." },
  { href: "/docs/overlay", title: "Overlay", description: "Draw directly into the game's native frame." },
  { href: "/docs/text", title: "Text", description: "Build rich text components." },
  { href: "/docs/entities", title: "Entities", description: "Read tracked entity state." },
  { href: "/docs/players", title: "Players", description: "Read tab-list players and the local player." },
  { href: "/docs/world", title: "World", description: "Read block, chunk, and dimension state." },
  { href: "/docs/inventory", title: "Inventory", description: "Read the local player's inventory." },
  { href: "/docs/scoreboard", title: "Scoreboard", description: "Read teams, objectives, and scores." },
  { href: "/docs/network", title: "Network", description: "HTTP, WebSocket, endpoints, and plugin messages." },
  { href: "/docs/events", title: "Events", description: "Generic pub/sub, plus tick-based timers." },
  { href: "/docs/input", title: "Input", description: "Keybinds and exclusive text/mouse capture." },
  { href: "/docs/plugins", title: "Plugins", description: "Export functions and call into other plugins." },
  { href: "/docs/client", title: "Client", description: "Client-side-only world and entity control." },
];

export default function DocsPage() {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">Plugin API</h1>
      <p className="text-sm text-white/40 mb-10">Lua scripting reference for Starfish plugins. This is a minimal, in-progress pass — it covers what's callable today, not a full guide.</p>

      <Section title="Positions">
        <P>Anywhere a function takes a position, you can pass a plain table, an entity, or the local player:</P>
        <Signature>{"{x, y, z}  |  <Entity>  |  starfish.players.me()"}</Signature>
      </Section>

      <Section title="Logging">
        <Signature>{"starfish.log.info(message) -> nil"}</Signature>
        <Signature>{"starfish.log.debug(message) -> nil"}</Signature>
        <Signature>{"starfish.log.warn(message) -> nil"}</Signature>
        <Signature>{"starfish.log.error(message) -> nil"}</Signature>
        <P>Each logs at its level, prefixed <Mono>[&lt;pluginName&gt;]</Mono>.</P>
      </Section>

      <Section title="Reference">
        <div className="space-y-3">
          {SECTIONS.map((s) => (
            <DocLink key={s.href} {...s} />
          ))}
        </div>
      </Section>
    </div>
  );
}

function DocLink({ href, title, description }: { href: string; title: string; description: string }) {
  return (
    <Link href={href} className="block rounded-lg border border-white/[0.08] bg-[rgba(0,0,0,0.5)] p-4 hover:border-white/[0.14] transition-colors">
      <h3 className="text-sm font-semibold text-white/70 mb-1">{title}</h3>
      <p className="text-[12px] text-white/35">{description}</p>
    </Link>
  );
}
