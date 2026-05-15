import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "HTTP - Starfish Docs",
  description: "HTTP API reference for Starfish plugins.",
};

export default function HttpPage() {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">HTTP</h1>
      <p className="text-sm text-white/40 mb-10">Make HTTP requests from your plugin. All requests are asynchronous; results are delivered to a callback.</p>

      <Section title="GET">
        <Signature>{"starfish.http.get(url, callback)"}</Signature>
        <Signature>{"starfish.http.get(url, headers, callback)"}</Signature>
        <P>Issues a GET request. Headers are optional. The callback receives a <Mono>response</Mono> table.</P>
        <Code>{`starfish.http.get("https://api.example.com/users", function(res)
    if not res.success then
        starfish.log("request failed: " .. (res.error or "unknown"))
        return
    end
    starfish.log("user: " .. res.data.name)
end)`}</Code>
        <P>For binary responses (images, audio, etc.), use <Mono>getBinary</Mono>. The callback's <Mono>response.binary</Mono> field is set instead of <Mono>response.body</Mono>.</P>
        <Signature>{"starfish.http.getBinary(url, callback)"}</Signature>
        <Signature>{"starfish.http.getBinary(url, headers, callback)"}</Signature>
      </Section>

      <Section title="POST">
        <Signature>{"starfish.http.post(url, body, callback)"}</Signature>
        <Signature>{"starfish.http.post(url, body, headers, callback)"}</Signature>
        <P>Sends a POST request. If <Mono>body</Mono> is a table, it is serialized as JSON and the <Mono>Content-Type</Mono> header defaults to <Mono>application/json</Mono>.</P>
        <Code>{`starfish.http.post("https://api.example.com/events", {
    name = "joined",
    timestamp = os.time(),
}, function(res)
    if res.status == 201 then
        starfish.log("event recorded")
    end
end)`}</Code>
      </Section>

      <Section title="request">
        <Signature>{"starfish.http.request(options, callback)"}</Signature>
        <P>For full control over the method, headers, and body. Useful for PUT, PATCH, DELETE, or any custom method.</P>
        <PropTable rows={[
          ["url", "string", "Target URL (required)"],
          ["method", "string", "HTTP method. Defaults to GET"],
          ["headers", "table", "Optional header map"],
          ["body", "string | table", "Request body. Tables are JSON-encoded"],
          ["binary", "boolean", "Treat the response body as binary"],
        ]} />
        <Code>{`starfish.http.request({
    url = "https://api.example.com/users/42",
    method = "PATCH",
    headers = { ["Authorization"] = "Bearer abc123" },
    body = { display_name = "Hexze" },
}, function(res)
    starfish.log("updated: " .. tostring(res.success))
end)`}</Code>
      </Section>

      <Section title="Response">
        <P>Every callback receives a single table with these fields:</P>
        <PropTable rows={[
          ["success", "boolean", "True if the request completed and a response was received"],
          ["status", "number | nil", "HTTP status code (omitted on transport failure)"],
          ["body", "string | nil", "Response body, if textual"],
          ["data", "table | nil", "Parsed JSON, if the body decoded successfully"],
          ["binary", "string | nil", "Response bytes, when using getBinary or binary = true"],
          ["size", "number | nil", "Length of binary bytes"],
          ["error", "string | nil", "Failure reason, when success is false"],
        ]} />
      </Section>

      <Section title="JSON helpers">
        <Signature>{"starfish.http.jsonEncode(table) -> string"}</Signature>
        <P>Serialize a Lua table to a JSON string. Arrays (sequential integer keys starting at 1) are encoded as JSON arrays; everything else becomes an object.</P>
        <Signature>{"starfish.http.jsonDecode(string) -> table"}</Signature>
        <P>Parse a JSON string into a Lua table. Throws on malformed input.</P>
      </Section>

      <Section title="URL helpers">
        <Signature>{"starfish.http.encodeUri(string) -> string"}</Signature>
        <P>Percent-encode a string for safe inclusion in a URL.</P>
        <Signature>{"starfish.http.decodeUri(string) -> string"}</Signature>
        <P>Decode a percent-encoded string. <Mono>+</Mono> is treated as a space.</P>
        <Code>{`local query = starfish.http.encodeUri("hello world")
starfish.http.get("https://example.com/search?q=" .. query, function(res) ... end)`}</Code>
      </Section>

      <Section title="Example">
        <P>A plugin that fetches the player's Hypixel profile on enable.</P>
        <Code>{`plugin = {
    name = "hypixel-greeter",
    version = "1.0.0"
}

local API_KEY = "your-api-key"

function onEnable()
    starfish.http.get(
        "https://api.hypixel.net/v2/player?uuid=" .. starfish.players.uuid(),
        { ["API-Key"] = API_KEY },
        function(res)
            if not res.success or not res.data then return end
            local name = res.data.player and res.data.player.displayname
            if name then
                starfish.chat.display("welcome back, " .. name)
            end
        end
    )
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
