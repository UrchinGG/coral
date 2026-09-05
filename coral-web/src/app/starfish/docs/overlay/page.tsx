import type { Metadata } from "next";
import { Section, P, Mono, Signature } from "@/components/starfish/DocsKit";

export const metadata: Metadata = {
  title: "Overlay - Starfish Docs",
  description: "Rendering API reference for Starfish plugins.",
};

export default function OverlayPage() {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">Overlay</h1>
      <p className="text-sm text-white/40 mb-10">Draw directly into the game's native frame. Drawing functions are only callable from inside an <Mono>onRender</Mono> callback.</p>

      <Section title="State">
        <Signature>{"starfish.overlay.supported() -> boolean"}</Signature>
        <P>Whether the injected renderer is currently live in-game.</P>
        <Signature>{"starfish.overlay.invalidate() -> nil"}</Signature>
        <P>Marks the overlay dirty, forcing a redraw.</P>
        <Signature>{"starfish.overlay.getViewport() -> (width, height)"}</Signature>
        <Signature>{"starfish.overlay.isCursorVisible() -> boolean"}</Signature>
        <Signature>{"starfish.overlay.measureText(text, size) -> (width, size)"}</Signature>
      </Section>

      <Section title="Callbacks">
        <Signature>{"starfish.overlay.onRender(callback) -> Subscription"}</Signature>
        <P>Registers a per-frame render callback. Drawing calls below only work inside it.</P>
        <Signature>{"starfish.overlay.onHit(callback) -> Subscription"}</Signature>
        <P>Registers a mouse-hit-region handler, for regions registered with <Mono>hitRect</Mono>. Event: <Mono>{"{id, type, x, y, button?, dy?}"}</Mono> where <Mono>type</Mono> is <Mono>click | release | scroll | enter | leave</Mono>.</P>
      </Section>

      <Section title="Drawing">
        <Signature>{"starfish.overlay.rect(opts) -> nil"}</Signature>
        <P>Filled rectangle. <Mono>opts</Mono>: <Mono>anchor?</Mono>, <Mono>x?</Mono>, <Mono>y?</Mono>, <Mono>w</Mono>, <Mono>h</Mono>, <Mono>color</Mono> (<Mono>{"{r,g,b,a}"}</Mono>).</P>
        <Signature>{"starfish.overlay.rectOutline(opts) -> nil"}</Signature>
        <P>Same, plus <Mono>thickness?</Mono> (default 1.0).</P>
        <Signature>{"starfish.overlay.text(opts) -> nil"}</Signature>
        <P>Plain text. <Mono>opts</Mono>: <Mono>anchor?</Mono>, <Mono>x?</Mono>, <Mono>y?</Mono>, <Mono>text</Mono>, <Mono>size</Mono>, <Mono>color</Mono>, <Mono>shadow?</Mono> (default false), <Mono>align?</Mono> + <Mono>alignWidth?</Mono> (both required together, or omit both).</P>
        <Signature>{"starfish.overlay.textColored(opts) -> nil"}</Signature>
        <P>Same as <Mono>text</Mono> but supports embedded per-segment color codes; <Mono>shadow?</Mono> defaults true.</P>
      </Section>

      <Section title="Textures">
        <Signature>{"starfish.overlay.loadTexture(opts) -> id"}</Signature>
        <P><Mono>opts</Mono>: <Mono>data</Mono> (raw RGBA8 bytes), <Mono>width</Mono>, <Mono>height</Mono>. Errors if <Mono>data</Mono> length ≠ <Mono>width * height * 4</Mono>.</P>
        <Signature>{"starfish.overlay.unloadTexture(id) -> nil"}</Signature>
        <Signature>{"starfish.overlay.texture(opts) -> nil"}</Signature>
        <P><Mono>opts</Mono>: <Mono>anchor?</Mono>, <Mono>x?</Mono>, <Mono>y?</Mono>, <Mono>w</Mono>, <Mono>h</Mono>, <Mono>texture</Mono> (id), <Mono>tint?</Mono>, <Mono>uv?</Mono> (<Mono>{"{u0,v0,u1,v1}"}</Mono>).</P>
      </Section>

      <Section title="Clipping and offsets">
        <Signature>{"starfish.overlay.pushClip(opts) -> nil"}</Signature>
        <Signature>{"starfish.overlay.popClip() -> nil"}</Signature>
        <P>Push/pop a rectangular clip region (<Mono>anchor?, x?, y?, w, h</Mono>).</P>
        <Signature>{"starfish.overlay.pushOffset(x, y) -> nil"}</Signature>
        <Signature>{"starfish.overlay.popOffset() -> nil"}</Signature>
        <P>Push/pop a draw-position offset.</P>
      </Section>

      <Section title="Hit regions">
        <Signature>{"starfish.overlay.hitRect(opts) -> nil"}</Signature>
        <P>Registers a mouse-hit rectangle, feeding <Mono>onHit</Mono>. <Mono>opts</Mono>: <Mono>id</Mono>, <Mono>anchor?, x?, y?, w, h</Mono>, <Mono>cursor?</Mono> (<Mono>hand | text | ...</Mono>), <Mono>scrollable?</Mono> (default false).</P>
      </Section>

      <Section title="Anchor">
        <P><Mono>starfish.overlay.Anchor</Mono> constants: <Mono>TOP_LEFT</Mono>, <Mono>TOP</Mono>, <Mono>TOP_RIGHT</Mono>, <Mono>LEFT</Mono>, <Mono>CENTER</Mono>, <Mono>RIGHT</Mono>, <Mono>BOTTOM_LEFT</Mono>, <Mono>BOTTOM</Mono>, <Mono>BOTTOM_RIGHT</Mono>.</P>
      </Section>
    </div>
  );
}
