import type { Metadata } from "next";
import { Section, P, Mono, Signature } from "@/components/starfish/DocsKit";

export const metadata: Metadata = {
  title: "Events - Starfish Docs",
  description: "Event and timer API reference for Starfish plugins.",
};

export default function EventsPage() {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">Events</h1>
      <p className="text-sm text-white/40 mb-10">Generic pub/sub, plus tick-based timers. Both return a <Mono>Subscription</Mono> — the same handle type <Mono>chat.onReceive</Mono>, <Mono>entities.onSpawn</Mono>, and every other <Mono>onX</Mono> call in the API returns.</p>

      <Section title="Subscription">
        <Signature>{"sub:off() -> nil"}</Signature>
        <P>Cancels the subscription. Every persistent registration across the API returns one of these.</P>
      </Section>

      <Section title="starfish.events">
        <Signature>{"starfish.events.on(eventName, handler) -> Subscription"}</Signature>
        <P><Mono>eventName</Mono> is <Mono>namespace:name</Mono>. Reserved namespaces must be a real catalogued event or this errors.</P>
        <Signature>{"starfish.events.once(eventName, handler) -> Subscription"}</Signature>
        <P>Same as <Mono>on</Mono>, but removes itself after firing once.</P>
        <Signature>{"starfish.events.off(subscription) -> nil"}</Signature>
        <P>Equivalent to <Mono>subscription:off()</Mono>.</P>
        <Signature>{"starfish.events.emit(eventName, data?) -> nil"}</Signature>
        <P>Delivers synchronously to this plugin's own handlers, and queues a cross-plugin broadcast.</P>
      </Section>

      <Section title="starfish.timers">
        <Signature>{"starfish.timers.everyTick(callback) -> Subscription"}</Signature>
        <P>Fires on every tick until cancelled.</P>
        <Signature>{"starfish.timers.delay(ms, callback) -> Subscription"}</Signature>
        <P>Fires once after <Mono>ms</Mono> (rounded up to the nearest tick).</P>
        <Signature>{"starfish.timers.interval(ms, callback) -> Subscription"}</Signature>
        <P>Fires repeatedly every <Mono>ms</Mono> until cancelled.</P>
      </Section>
    </div>
  );
}
