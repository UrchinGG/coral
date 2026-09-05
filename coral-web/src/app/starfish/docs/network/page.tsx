import type { Metadata } from "next";
import { Section, P, Mono, Signature, PropTable } from "@/components/starfish/DocsKit";

export const metadata: Metadata = {
  title: "Network - Starfish Docs",
  description: "HTTP, WebSocket, endpoint, and plugin-message API reference for Starfish plugins.",
};

export default function NetworkPage() {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">Network</h1>
      <p className="text-sm text-white/40 mb-10">Outbound HTTP/WebSocket requests, an inbound local HTTP server, and raw server plugin-channel messages.</p>

      <Section title="starfish.http">
        <Signature>{"starfish.http.get(url, callback)"}</Signature>
        <Signature>{"starfish.http.get(url, opts, callback)"}</Signature>
        <P>Queues a GET request. <Mono>opts.headers</Mono> is the only recognized option.</P>
        <Signature>{"starfish.http.getBinary(url, callback | opts, callback)"}</Signature>
        <P>Same as <Mono>get</Mono>, but the response's <Mono>binary</Mono> field is populated.</P>
        <Signature>{"starfish.http.post(url, callback)"}</Signature>
        <Signature>{"starfish.http.post(url, opts, callback)"}</Signature>
        <P><Mono>opts.body</Mono> (string, or table which is JSON-encoded) and <Mono>opts.headers</Mono>. Defaults <Mono>Content-Type: application/json</Mono> unless overridden.</P>
        <Signature>{"starfish.http.request(options, callback)"}</Signature>
        <PropTable rows={[
          ["url", "string", "Required"],
          ["method", "string", "Default GET"],
          ["headers", "table", ""],
          ["body", "string | table", "Tables are JSON-encoded"],
          ["binary", "boolean", "Default false"],
        ]} />
        <P>Every callback receives a single table:</P>
        <PropTable rows={[
          ["success", "boolean", ""],
          ["status", "number | nil", "Omitted on transport failure"],
          ["body", "string | nil", ""],
          ["data", "any | nil", "JSON-decoded body, if parseable"],
          ["binary", "string | nil", ""],
          ["size", "number | nil", ""],
          ["error", "string | nil", "Set when success is false"],
        ]} />
        <Signature>{"starfish.http.encodeUri(s) -> string"}</Signature>
        <Signature>{"starfish.http.decodeUri(s) -> string"}</Signature>
        <P>A truncated <Mono>%</Mono> escape passes through unchanged rather than erroring.</P>
      </Section>

      <Section title="starfish.websocket">
        <Signature>{"starfish.websocket.connect(url, opts?) -> Connection"}</Signature>
        <P>Opens an outbound connection; the handshake happens on a later tick, but a handle returns synchronously. <Mono>opts</Mono>: <Mono>headers</Mono>, <Mono>onOpen(conn)</Mono>, <Mono>onMessage(conn, data, isBinary)</Mono>, <Mono>onClose(conn, code?, reason?)</Mono>, <Mono>onError(conn, message)</Mono>.</P>
        <Signature>{"starfish.websocket.onWebSocket(path, handlers) -> nil"}</Signature>
        <P>Registers a handler set for inbound connections to <Mono>path</Mono>; re-registering replaces it. Same four callbacks as <Mono>connect</Mono>'s <Mono>opts</Mono>, except <Mono>onOpen(conn, request)</Mono> also receives <Mono>{"{path, headers, query}"}</Mono>.</P>
        <Signature>{"conn:send(text) -> boolean"}</Signature>
        <Signature>{"conn:sendBinary(data) -> boolean"}</Signature>
        <Signature>{"conn:close() -> boolean"}</Signature>
        <Signature>{"conn:isOpen() -> boolean"}</Signature>
        <P>Send/close return <Mono>false</Mono> if the connection isn't open.</P>
      </Section>

      <Section title="starfish.endpoint">
        <P>Registers handlers on the client's own local HTTP server, for external tools to call into a running plugin.</P>
        <Signature>{"starfish.endpoint.register(path, options, callback) -> nil"}</Signature>
        <P>Path is normalized to start with <Mono>/</Mono>; a trailing <Mono>/*</Mono> matches any sub-path. <Mono>options.methods</Mono> (default <Mono>{'{"GET","POST"}'}</Mono>).</P>
        <P><Mono>callback(request) -&gt; nil | string | table</Mono>, where <Mono>request</Mono> is <Mono>{"{path, method, headers, query, body?, data?}"}</Mono>. A returned string is a 200 body; a returned table may set <Mono>status</Mono>, <Mono>headers</Mono>, <Mono>body</Mono>.</P>
        <Signature>{"starfish.endpoint.unregister(path) -> boolean"}</Signature>
        <Signature>{"starfish.endpoint.list() -> string[]"}</Signature>
      </Section>

      <Section title="starfish.server">
        <Signature>{"starfish.server.sendPluginMessage(channel, data) -> nil"}</Signature>
        <P>Sends a raw plugin-channel message. <Mono>channel</Mono> must be <Mono>REGISTER</Mono>/<Mono>UNREGISTER</Mono> or <Mono>namespace:path</Mono>. <Mono>data</Mono> is a 1-indexed byte array.</P>
        <Signature>{"starfish.server.blockChannel(channel) -> nil"}</Signature>
        <P>Drops inbound plugin messages on that channel.</P>
        <Signature>{"starfish.server.onPluginMessage(channel, handler) -> Subscription"}</Signature>
        <P>Sugar over <Mono>starfish.events.on(&quot;server:pluginMessage&quot;, ...)</Mono>, filtered to one channel; the handler receives just the payload data.</P>
      </Section>
    </div>
  );
}
