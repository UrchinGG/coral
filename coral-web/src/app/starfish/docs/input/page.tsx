import type { Metadata } from "next";
import { Section, P, Mono, Signature } from "@/components/starfish/DocsKit";

export const metadata: Metadata = {
  title: "Input - Starfish Docs",
  description: "Keyboard and mouse input API reference for Starfish plugins.",
};

export default function InputPage() {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">Input</h1>
      <p className="text-sm text-white/40 mb-10">Keybinds, and exclusive text/mouse capture for overlay UI.</p>

      <Section title="Keybinds">
        <Signature>{"starfish.input.bind(key, options) -> nil"}</Signature>
        <P>Key names are case-insensitive (<Mono>{'"F1"'}</Mono>, <Mono>{'"W"'}</Mono>). <Mono>options</Mono>: <Mono>onPress?</Mono>, <Mono>onRelease?</Mono>, <Mono>passThrough?</Mono> (default false), <Mono>activeInMenu?</Mono> (default false). Replaces any existing bind for that key.</P>
        <Signature>{"starfish.input.unbind(key) -> nil"}</Signature>
        <Signature>{"starfish.input.isHeld(key) -> boolean"}</Signature>
      </Section>

      <Section title="Cursor and support">
        <Signature>{"starfish.input.getCursor() -> (x, y)"}</Signature>
        <P>Last known free-cursor position; stale while captured.</P>
        <Signature>{"starfish.input.supported() -> boolean"}</Signature>
        <P>Whether the input-injection tap is live in the game process.</P>
      </Section>

      <Section title="Text capture">
        <Signature>{"starfish.input.captureText(opts) -> TextSession"}</Signature>
        <P>Starts an exclusive text-editing session, superseding any prior one. <Mono>opts</Mono>: <Mono>initial?</Mono>, <Mono>onChange?(state)</Mono>, <Mono>onSubmit?(text)</Mono>, <Mono>onCancel?()</Mono>.</P>
        <Signature>{"session:get() -> {text, cursor, selection} | nil"}</Signature>
        <P><Mono>selection</Mono> is <Mono>{"{start, finish}"}</Mono>.</P>
        <Signature>{"session:set(text, cursor?) -> nil"}</Signature>
        <Signature>{"session:cursorFromX(offsetPx, size, extend) -> nil"}</Signature>
        <Signature>{"session:stop() -> nil"}</Signature>
        <P>Ends the session yourself — <Mono>onCancel</Mono> does not fire. If instead a click lands outside every hit region, or the game recaptures the mouse, the session is closed for you and <Mono>onCancel</Mono> does fire — both are things a plugin can't otherwise notice on its own.</P>
      </Section>

      <Section title="Mouse capture">
        <Signature>{"starfish.input.captureMouse(opts) -> MouseSession"}</Signature>
        <P>Starts an exclusive mouse-capture session, superseding any prior one. <Mono>opts</Mono>: <Mono>onMiss?(event)</Mono> (<Mono>{'{kind="down"|"up"|"scroll", button, x, y, dy}'}</Mono>), <Mono>onCancel?()</Mono>.</P>
        <Signature>{"session:stop() -> nil"}</Signature>
        <P>Ends the session yourself — <Mono>onCancel</Mono> does not fire. Pressing Escape closes it for you instead, the same guarantee a vanilla screen makes, and does fire <Mono>onCancel</Mono>.</P>
      </Section>
    </div>
  );
}
