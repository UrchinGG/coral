import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Webhooks - Starfish Docs",
  description: "Webhook API reference for Starfish plugins.",
};

export default function WebhooksPage() {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">Webhooks</h1>
      <p className="text-sm text-white/40 mb-10">Register HTTP endpoints that external services can call. The Starfish client exposes a local HTTP server; plugins handle requests on registered paths.</p>

      <Section title="Registering a handler">
        <Signature>{"starfish.webhook.register(path, options, handler)"}</Signature>
        <P>Registers a handler for incoming requests to <Mono>path</Mono>. The <Mono>options</Mono> table lets you restrict which methods the handler accepts.</P>
        <Code>{`starfish.webhook.register("/ping", {
    methods = { "GET" }
}, function(req)
    return { status = 200, body = "pong" }
end)`}</Code>
        <PropTable rows={[
          ["methods", "table | nil", "Array of HTTP method names. Defaults to [\"GET\", \"POST\"]"],
        ]} />
        <P>Paths are normalized — leading slashes are added if missing, so <Mono>{"\"foo\""}</Mono> and <Mono>{"\"/foo\""}</Mono> register the same route.</P>
      </Section>

      <Section title="Wildcards">
        <P>A path ending in <Mono>{"/*"}</Mono> matches any sub-path. The full path is available on <Mono>req.path</Mono>.</P>
        <Code>{`starfish.webhook.register("/files/*", { methods = { "GET" } }, function(req)
    local name = req.path:sub(#"/files/" + 1)
    return { status = 200, body = "you asked for: " .. name }
end)`}</Code>
      </Section>

      <Section title="Request">
        <P>The handler is called with one argument — a <Mono>request</Mono> table.</P>
        <PropTable rows={[
          ["path", "string", "Full request path (after normalization)"],
          ["method", "string", "HTTP method, uppercase"],
          ["headers", "table", "Header map. Keys are case-sensitive as received"],
          ["query", "table", "Parsed query-string parameters"],
          ["body", "string | nil", "Raw body, if any"],
          ["data", "table | nil", "Parsed JSON body, if the body decoded successfully"],
        ]} />
      </Section>

      <Section title="Response">
        <P>The handler's return value determines the response. Three forms are accepted:</P>
        <PropTable rows={[
          ["nil", "—", "Empty 200 OK"],
          ["string", "—", "200 OK with the string as the body"],
          ["table", "—", "Full control — see fields below"],
        ]} />
        <P>When returning a table:</P>
        <PropTable rows={[
          ["status", "number", "HTTP status code. Defaults to 200"],
          ["headers", "table", "Response headers"],
          ["body", "string", "Response body"],
        ]} />
        <Code>{`starfish.webhook.register("/echo", { methods = { "POST" } }, function(req)
    return {
        status = 200,
        headers = { ["Content-Type"] = "application/json" },
        body = starfish.http.jsonEncode({ received = req.data })
    }
end)`}</Code>
      </Section>

      <Section title="Method filtering">
        <P>If a request reaches a registered path but uses a method outside the configured list, Starfish responds with <Mono>405 Method Not Allowed</Mono> automatically — your handler is not invoked.</P>
      </Section>

      <Section title="Errors">
        <P>If your handler throws, Starfish logs the error and responds with <Mono>500 Internal Server Error</Mono> including the error message. Catch errors yourself if you'd rather return a custom response.</P>
      </Section>

      <Section title="Unregistering">
        <Signature>{"starfish.webhook.unregister(path) -> boolean"}</Signature>
        <P>Removes a previously registered handler. Returns <Mono>true</Mono> if a handler was removed, <Mono>false</Mono> otherwise.</P>
        <P>Handlers are automatically unregistered when the plugin is disabled or reloaded, so you usually don't need to call this manually.</P>
      </Section>

      <Section title="Listing handlers">
        <Signature>{"starfish.webhook.list() -> table"}</Signature>
        <P>Returns an array of registered paths for the current plugin.</P>
      </Section>

      <Section title="Example">
        <P>A plugin that exposes a webhook for an external dashboard to query the player's current server.</P>
        <Code>{`plugin = {
    name = "server-info-webhook",
    version = "1.0.0"
}

function onEnable()
    starfish.webhook.register("/server", { methods = { "GET" } }, function(req)
        return {
            status = 200,
            headers = { ["Content-Type"] = "application/json" },
            body = starfish.http.jsonEncode({
                server = starfish.server.address(),
                player = starfish.players.name(),
            })
        }
    end)
end

function onDisable()
    starfish.webhook.unregister("/server")
end`}</Code>
      </Section>
    </div>
  );
}


function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mb-10">
      <h2 className="text-lg font-semibold tracking-tight mb-3">{title}</h2>
      {children}
    </section>
  );
}

function P({ children }: { children: React.ReactNode }) {
  return <p className="text-sm text-white/50 mb-3 leading-relaxed">{children}</p>;
}

function Mono({ children }: { children: React.ReactNode }) {
  return <code className="text-[13px] text-white/70 bg-white/[0.06] px-1.5 py-0.5 rounded">{children}</code>;
}

function Signature({ children }: { children: string }) {
  return (
    <div className="mb-3 px-3 py-2 rounded-md bg-white/[0.04] border border-white/[0.08] font-mono text-sm text-white/60">
      {children}
    </div>
  );
}

function Code({ children }: { children: string }) {
  return (
    <pre className="mb-4 px-4 py-3 rounded-md bg-white/[0.04] border border-white/[0.08] overflow-x-auto">
      <code className="text-[13px] text-white/55 leading-relaxed">{children}</code>
    </pre>
  );
}

function PropTable({ rows }: { rows: [string, string, string][] }) {
  return (
    <div className="mb-4 rounded-md border border-white/[0.08] overflow-hidden">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-white/[0.08] bg-white/[0.02]">
            <th className="text-left px-3 py-2 text-white/40 font-medium text-xs">Field</th>
            <th className="text-left px-3 py-2 text-white/40 font-medium text-xs">Type</th>
            <th className="text-left px-3 py-2 text-white/40 font-medium text-xs">Description</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(([field, type, desc]) => (
            <tr key={field} className="border-b border-white/[0.04] last:border-0">
              <td className="px-3 py-2 font-mono text-[13px] text-white/60">{field}</td>
              <td className="px-3 py-2 text-[13px] text-white/40">{type}</td>
              <td className="px-3 py-2 text-[13px] text-white/40">{desc}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
