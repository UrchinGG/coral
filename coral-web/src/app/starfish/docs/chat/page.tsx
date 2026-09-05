import type { Metadata } from "next";
import { Section, P, Mono, Signature } from "@/components/starfish/DocsKit";

export const metadata: Metadata = {
  title: "Chat - Starfish Docs",
  description: "Chat API reference for Starfish plugins.",
};

export default function ChatPage() {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">Chat</h1>
      <p className="text-sm text-white/40 mb-10">Intercept, send, and format chat messages.</p>

      <Section title="Intercepting">
        <Signature>{"starfish.chat.onReceive(handler, opts?) -> Subscription"}</Signature>
        <P>Registers an inbound chat handler. <Mono>opts.priority</Mono> (higher runs first) and <Mono>opts.receiveCancelled</Mono> (see a message another handler already cancelled) are optional. The handler receives a <Mono>ChatMessage</Mono>.</P>
        <Signature>{"starfish.chat.onSend(handler, opts?) -> Subscription"}</Signature>
        <P>Same as <Mono>onReceive</Mono>, for outbound (client → server) chat.</P>
        <Signature>{"starfish.chat.blockIncoming(value) -> nil"}</Signature>
        <P>Drops inbound chat containing <Mono>value</Mono>.</P>
        <Signature>{"starfish.chat.blockOutgoing(value) -> nil"}</Signature>
        <P>Drops outbound chat starting with <Mono>value</Mono>.</P>
      </Section>

      <Section title="ChatMessage">
        <P>Passed to <Mono>onReceive</Mono>/<Mono>onSend</Mono> handlers. Fields: <Mono>kind</Mono>, <Mono>legacy</Mono>, <Mono>content</Mono>, <Mono>signed</Mono>, <Mono>rewritable</Mono>, <Mono>cancelled</Mono>, <Mono>sender</Mono> (<Mono>{"{uuid, name}"}</Mono> or nil).</P>
        <Signature>{"msg:cancel() -> nil"}</Signature>
        <P>Drops the message. Later handlers without <Mono>receiveCancelled = true</Mono> are skipped.</P>
        <Signature>{"msg:setContent(component) -> nil"}</Signature>
        <P>Replaces content from a <Mono>starfish.text</Mono> component.</P>
        <Signature>{"msg:setLegacy(legacy) -> nil"}</Signature>
        <P>Replaces content from a legacy <Mono>§</Mono>-coded string.</P>
      </Section>

      <Section title="Sending">
        <Signature>{"starfish.chat.sendToClient(message) -> nil"}</Signature>
        <P>Injects a message as if received from the server.</P>
        <Signature>{"starfish.chat.sendToServer(message) -> nil"}</Signature>
        <P>Sends a raw chat/command string to the server.</P>
        <Signature>{"starfish.chat.requestTabComplete(text, position?) -> nil"}</Signature>
        <P>Requests server tab-completion for <Mono>text</Mono>.</P>
        <Signature>{"starfish.chat.title(title, opts?) -> nil"}</Signature>
        <P>Shows a title/subtitle. <Mono>opts</Mono>: <Mono>subtitle</Mono>, <Mono>fadeIn</Mono> (default 10), <Mono>stay</Mono> (default 70), <Mono>fadeOut</Mono> (default 20).</P>
        <Signature>{"starfish.chat.actionBar(text) -> nil"}</Signature>
        <P>Shows an action-bar message.</P>
      </Section>

      <Section title="Prefixed helpers">
        <P>Send under a <Mono>[Starfish-&lt;plugin&gt;]</Mono> tag: <Mono>info(message)</Mono>, <Mono>error(message)</Mono> (red), <Mono>success(message)</Mono> (green), <Mono>warning(message)</Mono> (yellow).</P>
      </Section>

      <Section title="Color">
        <P><Mono>starfish.chat.Color</Mono> is a table of legacy <Mono>§</Mono> codes: all 16 colors plus <Mono>BOLD</Mono>, <Mono>STRIKETHROUGH</Mono>, <Mono>UNDERLINE</Mono>, <Mono>ITALIC</Mono>, <Mono>OBFUSCATED</Mono>, <Mono>RESET</Mono>. E.g. <Mono>starfish.chat.Color.DARK_AQUA</Mono>.</P>
      </Section>
    </div>
  );
}
