export type DocEntry = { signature: string; description?: string };

export type DocTable = { columns?: [string, string, string]; rows: [string, string, string][] };

export type DocSection = {
  title: string;
  description?: string;
  entries?: DocEntry[];
  table?: DocTable;
};

export type DocPage = {
  slug: string;
  title: string;
  group: DocGroup;
  summary: string;
  sections: DocSection[];
  notes: string[];
};

export const DOC_GROUPS = ["Getting Started", "Communication", "Game State", "Network", "Core"] as const;

export type DocGroup = (typeof DOC_GROUPS)[number];

export type NavSection = { label: string; items: { title: string; href: string }[] };

export const INTRODUCTION =
  "A plugin is a Lua file in the `plugins` folder of Starfish's app-data directory (`%APPDATA%\\starfish\\plugins` on Windows), or a folder there containing an `init.lua`. Everything Starfish offers lives under the `starfish` global, and each plugin runs in its own Lua state.";

export const TEMPLATE_PLUGIN = `plugin = {
    name = "my-plugin",
    displayName = "My Plugin",
    prefix = "§7MP",
    version = "0.1.0",
    description = "",
    dependencies = {
        { name = "some-plugin", minVersion = "1.0.0", optional = true },
    },
}

-- Constants

-- Config schema

starfish.schema.section({
    key = "feature",
    label = "Feature Name",
    description = "",
    settings = {
        { key = "feature.enabled", type = "toggle", default = true, description = "" },
        { key = "feature.mode", type = "cycle", default = "a", description = "", values = {
            { text = "A", value = "a" },
            { text = "B", value = "b" },
        } },
    },
})

-- Feature

-- Event wiring

-- Commands

starfish.commands.register("command", {
    description = "",
    arguments = {
        { name = "value", type = "string", optional = true },
    },
}, function()
end)

-- Exports

-- Startup
`;

export const TEMPLATE_REFERENCE =
  "Start from `plugins/template/plugin.lua` in the Starfish repository. The same folder holds `starfish.d.lua`, the full API reference with every function's signature, which also gives editors with the Lua language server autocompletion.";

export const SHARED_TYPES: DocSection = {
  title: "Shared types",
  table: {
    columns: ["Name", "Accepts", "Description"],
    rows: [
      ["Positionable", "{x, y, z} | Entity | starfish.players.me()", "Anything with a position, wherever a function takes one"],
      ["MessageContent", "string | Component", "A §-coded string or a starfish.text component"],
      ["Subscription", "sub:off()", "Returned by every handler and timer registration; off() cancels it"],
      ["Identifier", "string", "A namespaced id such as \"minecraft:stone\""],
    ],
  },
};

export const DOC_PAGES: DocPage[] = [
  {
    slug: "manifest",
    title: "Plugin Manifest",
    group: "Getting Started",
    summary: "The plugin table every plugin declares, dependencies, and capabilities.",
    sections: [
      {
        title: "Layout",
        description:
          "A single-file plugin is `plugins/<name>.lua`. A folder plugin is `plugins/<name>/init.lua` and can `require` its own modules. New plugins are picked up automatically; reload an edited plugin from the Starfish app.",
      },
      {
        title: "plugin = { ... }",
        description: "Every plugin declares a global `plugin` table. Keep its values plain literals: the Starfish app edits them in place.",
        table: {
          rows: [
            ["name", "string", "Unique id: 3-64 chars, a lowercase letter then lowercase letters, digits, or dashes. Also the command word and config key. Defaults to the file name"],
            ["displayName", "string?", "Shown in the app. Defaults to name"],
            ["version", "string?", "Semantic version. Defaults to 1.0.0"],
            ["prefix", "string?", "Chat tag for chat.info/success/warning/error, shown as [Starfish-<prefix>]"],
            ["description", "string?", "One-line summary"],
            ["readme", "string?", "Longer markdown for the plugin's page"],
            ["credits", "string?", "People to thank"],
            ["command", "string?", "Replaces name as the command word"],
            ["dependencies", "table?", "Plugins this one calls: names, or {name, minVersion?, optional?}. Required ones load first"],
            ["requires", "string[]?", "Capabilities the plugin needs; it refuses to load without them"],
            ["onDisable", "function?", "Called when the plugin is disabled or reloaded"],
          ],
        },
      },
      {
        title: "Capabilities",
        description:
          "Named flags a plugin can check or require: `chat.rewriteIncoming` and `chat.rewriteOutgoing` (true), `world.minY` (0), `world.height` (256). Any other name is absent.",
        entries: [
          { signature: "starfish.capabilities.has(name) -> boolean" },
          { signature: "starfish.capabilities.get(name) -> any | nil" },
          { signature: "starfish.capabilities.require(name) -> nil", description: "Errors if the capability is missing." },
        ],
      },
    ],
    notes: [
      "A required dependency must be loaded, at `minVersion` or later, or the plugin stays disabled until it is. An optional dependency never blocks loading or affects load order; if it's loaded but older than `minVersion`, it counts as absent, so `starfish.plugins.optional` calls return nil.",
      "Everything a plugin registers through `starfish.*` is removed automatically when it unloads; use `onDisable` only for side effects Starfish can't see.",
      "Don't rely on an `author` field: the plugin registry credits the account that published the plugin and removes `author` from published copies.",
    ],
  },
  {
    slug: "chat",
    title: "Chat",
    group: "Communication",
    summary: "Intercept, send, and format chat messages.",
    sections: [
      {
        title: "Intercepting",
        entries: [
          {
            signature: "starfish.chat.onReceive(handler, opts?) -> Subscription",
            description:
              "Handles chat arriving from the server; `handler` receives a `ChatMessage`. `opts.priority` (higher runs first, default 0) and `opts.receiveCancelled` (also see messages an earlier handler cancelled) are optional.",
          },
          {
            signature: "starfish.chat.onSend(handler, opts?) -> Subscription",
            description: "Same as `onReceive`, for chat and commands you send.",
          },
          { signature: "starfish.chat.blockIncoming(text) -> nil", description: "Drops every incoming message containing `text` until the plugin unloads." },
          { signature: "starfish.chat.blockOutgoing(prefix) -> nil", description: "Drops every outgoing message starting with `prefix` until the plugin unloads." },
        ],
      },
      {
        title: "ChatMessage",
        description: "Passed to `onReceive` and `onSend` handlers.",
        table: {
          rows: [
            ["kind", "\"chat\" | \"system\" | \"actionBar\"", ""],
            ["legacy", "string", "The message as a §-coded string"],
            ["content", "Component", ""],
            ["cancelled", "boolean", ""],
          ],
        },
        entries: [
          { signature: "msg:cancel() -> nil", description: "Drops the message. Later handlers only see it if they set `receiveCancelled`." },
          { signature: "msg:setContent(content) -> nil", description: "Replaces the message with a `MessageContent`; `legacy` follows." },
          { signature: "msg:setLegacy(legacy) -> nil", description: "Replaces the message from a §-coded string; `content` follows." },
        ],
      },
      {
        title: "Sending",
        entries: [
          { signature: "starfish.chat.sendToClient(message) -> nil", description: "Shows a `MessageContent` in your chat as if the server sent it." },
          { signature: "starfish.chat.sendToServer(message) -> nil", description: "Sends a chat message or /command to the server as you." },
          { signature: "starfish.chat.requestTabComplete(text, position?) -> nil", description: "Asks the server for completions of `text`; they arrive as the `chat:tabComplete` event." },
          {
            signature: "starfish.chat.title(title, opts?) -> nil",
            description: "Shows a title. `opts`: `subtitle`, `fadeIn` (default 10), `stay` (default 70), `fadeOut` (default 20), in ticks.",
          },
          { signature: "starfish.chat.actionBar(text) -> nil" },
        ],
      },
      {
        title: "Prefixed messages",
        description:
          "Show a message under your plugin's `[Starfish-<prefix>]` tag, using the manifest `prefix` (just `[Starfish]` without one). `error`, `success`, and `warning` color the message red, green, and yellow unless it sets its own color.",
        entries: [
          { signature: "starfish.chat.info(message) -> nil" },
          { signature: "starfish.chat.error(message) -> nil" },
          { signature: "starfish.chat.success(message) -> nil" },
          { signature: "starfish.chat.warning(message) -> nil" },
        ],
      },
      {
        title: "Color",
        description:
          "`starfish.chat.Color` maps names to legacy § codes: the 16 colors (`BLACK` ... `WHITE`, e.g. `DARK_AQUA`) plus `BOLD`, `ITALIC`, `UNDERLINE`, `STRIKETHROUGH`, `OBFUSCATED`, and `RESET`.",
      },
    ],
    notes: [],
  },
  {
    slug: "commands",
    title: "Commands",
    group: "Communication",
    summary: "Register slash commands and build replies.",
    sections: [
      {
        title: "Registering",
        entries: [
          { signature: "starfish.commands.register(name, spec, handler) -> nil", description: "Registers `/<command word> <name>`, where the command word is the manifest `name` (or `command`)." },
          { signature: "starfish.commands.registerGlobal(name, spec, handler) -> nil", description: "Registers a top-level `/<name>` command." },
          { signature: "starfish.commands.unregister(name) -> boolean" },
          { signature: "starfish.commands.exists(name) -> boolean" },
          { signature: "starfish.commands.list() -> {name, description}[]", description: "This plugin's subcommands; global commands are not included." },
        ],
        table: {
          rows: [
            ["spec.description", "string?", "Shown in help output"],
            ["spec.arguments", "table?", "Array of argument specs"],
            ["argument.name", "string", "Key in ctx.args"],
            ["argument.type", "string?", "string (default), int, number, bool, player, choice, or greedy"],
            ["argument.description", "string?", ""],
            ["argument.optional", "boolean?", ""],
            ["argument.choices", "string[]?", "Required for choice"],
          ],
        },
      },
      {
        title: "Handler context",
        description:
          "Handlers receive a context and may return a string, a `Component`, a `UiReply`, or nil. Arguments are parsed and type-checked before the handler runs: `player` matches an online player case-insensitively and binds their exact name, `greedy` takes the rest of the input.",
        table: {
          rows: [
            ["ctx.args", "table", "Parsed arguments by name"],
            ["ctx.page", "integer", ""],
            ["ctx.pluginName", "string", ""],
            ["ctx.commandName", "string", ""],
          ],
        },
        entries: [
          { signature: "ctx:reply(value) -> nil", description: "Queues a reply. Replies are sent in order, followed by the handler's return value." },
        ],
      },
      {
        title: "starfish.ui",
        entries: [
          { signature: "starfish.ui.lines(lines) -> UiReply", description: "One chat line per string or component." },
          {
            signature: "starfish.ui.page(opts) -> UiReply",
            description: "A title, one page of `entries`, and a `Page N/Total` footer. `opts`: `entries`, `title?`, `perPage?` (default 10), `page?` (default 1).",
          },
        ],
      },
    ],
    notes: [
      "`UiReply` is opaque: it is only valid as a handler return value or `ctx:reply` argument.",
      "`ui.page` accepts a `command` option, but it doesn't render clickable pagination yet.",
    ],
  },
  {
    slug: "text",
    title: "Text",
    group: "Communication",
    summary: "Build rich text components with click and hover events.",
    sections: [
      {
        title: "Building",
        entries: [
          { signature: "starfish.text.of(text) -> Component", description: "Builds a component from a §-coded string." },
          { signature: "starfish.text.empty() -> Component" },
          { signature: "starfish.text.join(parts, separator?) -> Component", description: "Joins `MessageContent` parts with a string separator (default `\"\"`)." },
        ],
      },
      {
        title: "Converting",
        entries: [
          { signature: "starfish.text.legacy(content) -> string", description: "Flattens to a §-coded string, losing click and hover." },
          { signature: "starfish.text.plain(text) -> string", description: "Strips every §x code." },
          { signature: "starfish.text.json(content) -> string" },
          { signature: "starfish.text.fromJson(json) -> Component", description: "To and from Minecraft's JSON text format, keeping click and hover." },
        ],
      },
      {
        title: "Component methods",
        description: "Every method returns a new component and leaves the original unchanged, so calls chain.",
        entries: [
          { signature: "c:color(name) -> Component", description: "A color name such as `gold` or `dark_aqua`." },
          { signature: "c:bold() -> Component" },
          { signature: "c:italic() -> Component" },
          { signature: "c:underline() -> Component" },
          { signature: "c:strikethrough() -> Component" },
          { signature: "c:obfuscated() -> Component" },
          { signature: "c:hover(content) -> Component", description: "Shows `content` on hover." },
          { signature: "c:click(opts) -> Component", description: "`opts.action`: `runCommand`, `suggestCommand`, `openUrl`, or `copyToClipboard`; `opts.value`." },
          { signature: "c:run(command) -> Component" },
          { signature: "c:suggest(command) -> Component" },
          { signature: "c:url(url) -> Component" },
          { signature: "c:copy(text) -> Component", description: "Shorthands for `click` with the matching action." },
          { signature: "c:append(content) -> Component", description: "Same as `c .. content`." },
        ],
      },
    ],
    notes: [
      "`..` only works with a component on the left: `component .. \"text\"`, not `\"text\" .. component`.",
    ],
  },
  {
    slug: "display",
    title: "Display",
    group: "Communication",
    summary: "Tab-list name prefixes and suffixes, and header and footer lines.",
    sections: [
      {
        title: "Prefix and suffix",
        description:
          "Each plugin edits only its own contribution to a player's tab-list name, so plugins never overwrite each other. `text` is a §-coded string; `opts.priority` orders this plugin's contribution against others.",
        entries: [
          { signature: "starfish.display.setPrefix(uuid, text, opts?) -> nil" },
          { signature: "starfish.display.appendPrefix(uuid, text, opts?) -> nil" },
          { signature: "starfish.display.prependPrefix(uuid, text, opts?) -> nil" },
          { signature: "starfish.display.clearPrefix(uuid) -> nil" },
          { signature: "starfish.display.setSuffix(uuid, text, opts?) -> nil" },
          { signature: "starfish.display.appendSuffix(uuid, text, opts?) -> nil" },
          { signature: "starfish.display.prependSuffix(uuid, text, opts?) -> nil" },
          { signature: "starfish.display.clearSuffix(uuid) -> nil" },
        ],
      },
      {
        title: "Reading",
        entries: [
          { signature: "starfish.display.prefix(uuid) -> string" },
          { signature: "starfish.display.suffix(uuid) -> string", description: "The combined prefix or suffix from every plugin." },
          { signature: "starfish.display.othersPrefix(uuid) -> string" },
          { signature: "starfish.display.othersSuffix(uuid) -> string", description: "The combined prefix or suffix from every plugin except this one." },
          { signature: "starfish.display.isModified(uuid) -> boolean", description: "Whether any plugin has changed this player's name." },
        ],
      },
      {
        title: "Tab-list removal",
        entries: [
          { signature: "starfish.display.holdRemoval(uuid) -> nil", description: "Keeps a player's entry after the server removes it, until released." },
          { signature: "starfish.display.releaseRemoval(uuid) -> nil", description: "Releases this plugin's hold; the removal happens once no plugin holds it." },
        ],
      },
      {
        title: "Tab header and footer",
        description:
          "Works like prefixes and suffixes, without a player: each plugin edits its own header and footer text, and every plugin's text is joined in priority order and shown after the server's own. Include `\\n` in your text to start a new line.",
        entries: [
          { signature: "starfish.display.setTabHeader(text, opts?) -> nil" },
          { signature: "starfish.display.appendTabHeader(text, opts?) -> nil" },
          { signature: "starfish.display.prependTabHeader(text, opts?) -> nil" },
          { signature: "starfish.display.clearTabHeader() -> nil" },
          { signature: "starfish.display.tabHeader() -> string", description: "The combined header text from every plugin." },
          { signature: "starfish.display.othersTabHeader() -> string", description: "The combined header text from every plugin except this one." },
          { signature: "starfish.display.serverTabHeader() -> string", description: "The server's own header, §-coded." },
          { signature: "starfish.display.setTabFooter(text, opts?) -> nil" },
          { signature: "starfish.display.appendTabFooter(text, opts?) -> nil" },
          { signature: "starfish.display.prependTabFooter(text, opts?) -> nil" },
          { signature: "starfish.display.clearTabFooter() -> nil" },
          { signature: "starfish.display.tabFooter() -> string", description: "The combined footer text from every plugin." },
          { signature: "starfish.display.othersTabFooter() -> string", description: "The combined footer text from every plugin except this one." },
          { signature: "starfish.display.serverTabFooter() -> string", description: "The server's own footer, §-coded." },
        ],
      },
      {
        title: "Clearing",
        entries: [
          { signature: "starfish.display.clear(uuid) -> nil", description: "Clears this plugin's prefix and suffix for a player." },
          { signature: "starfish.display.clearAll() -> nil", description: "Clears this plugin's contributions for every player, and its header and footer text." },
        ],
      },
    ],
    notes: [
      "Display functions take §-coded strings only; components are rejected.",
    ],
  },
  {
    slug: "overlay",
    title: "Overlay",
    group: "Communication",
    summary: "Draw directly into the game's frame.",
    sections: [
      {
        title: "State",
        entries: [
          { signature: "starfish.overlay.supported() -> boolean", description: "Whether the in-game renderer is running." },
          { signature: "starfish.overlay.invalidate() -> nil", description: "Asks for a redraw: `onRender` runs again on the next frame." },
          { signature: "starfish.overlay.getViewport() -> (width, height)" },
          { signature: "starfish.overlay.isCursorVisible() -> boolean" },
          { signature: "starfish.overlay.measureText(text, size) -> (width, height)" },
        ],
      },
      {
        title: "Callbacks",
        entries: [
          { signature: "starfish.overlay.onRender(callback) -> Subscription", description: "Registers a draw callback. The drawing functions below only work inside it." },
          {
            signature: "starfish.overlay.onHit(callback) -> Subscription",
            description: "Mouse events for regions declared with `hitRect`: `{id, type, x, y, button?, dy?}` where `type` is `click`, `release`, `scroll`, `enter`, or `leave`.",
          },
        ],
      },
      {
        title: "Drawing",
        description:
          "Every shape takes `anchor?` (default top-left), `x?` and `y?` (offsets, default 0), and a size `w`, `h`. Colors are `{r?, g?, b?, a?}` with 0-255 channels, each defaulting to 255.",
        entries: [
          { signature: "starfish.overlay.rect(opts) -> nil", description: "Filled rectangle; `opts.color`." },
          { signature: "starfish.overlay.rectOutline(opts) -> nil", description: "Same, plus `thickness?` (default 1)." },
          {
            signature: "starfish.overlay.text(opts) -> nil",
            description: "`opts`: `anchor?`, `x?`, `y?`, `text`, `size`, `color`, `shadow?` (default false), and `align?` (`left`, `center`, `right`) with `alignWidth?`; alignment needs both.",
          },
          { signature: "starfish.overlay.textColored(opts) -> nil", description: "Like `text`, but honors § color codes in the text; `shadow?` defaults to true." },
        ],
      },
      {
        title: "Textures",
        entries: [
          { signature: "starfish.overlay.loadTexture(opts) -> id", description: "`opts`: `data` (raw RGBA8 bytes, `width * height * 4` long), `width`, `height`." },
          { signature: "starfish.overlay.unloadTexture(id) -> nil" },
          { signature: "starfish.overlay.texture(opts) -> nil", description: "`opts`: box fields, `texture` (id), `tint?`, `uv?` (`{u0, v0, u1, v1}`)." },
        ],
      },
      {
        title: "Clipping, offsets, hit regions",
        entries: [
          { signature: "starfish.overlay.pushClip(opts) -> nil" },
          { signature: "starfish.overlay.popClip() -> nil", description: "Push and pop a rectangular clip region (`anchor?, x?, y?, w, h`)." },
          { signature: "starfish.overlay.pushOffset(x, y) -> nil" },
          { signature: "starfish.overlay.popOffset() -> nil", description: "Push and pop an offset added to every draw position." },
          {
            signature: "starfish.overlay.hitRect(opts) -> nil",
            description: "Declares a mouse region that reports to `onHit`. `opts`: `id`, box fields, `cursor?` (`hand` or `text`, default arrow), `scrollable?` (default false).",
          },
        ],
      },
      {
        title: "Anchor",
        description:
          "`starfish.overlay.Anchor`: `TOP_LEFT`, `TOP`, `TOP_RIGHT`, `LEFT`, `CENTER`, `RIGHT`, `BOTTOM_LEFT`, `BOTTOM`, `BOTTOM_RIGHT`.",
      },
    ],
    notes: [
      "`onRender` does not run every frame. What you drew stays on screen, and `onRender` runs again only after `invalidate()` or a window resize, so call `invalidate()` whenever your content changes.",
      "`unloadTexture` also only works inside an `onRender` callback.",
      "An unknown `anchor` draws at the top-left instead of erroring.",
    ],
  },
  {
    slug: "players",
    title: "Players",
    group: "Game State",
    summary: "Read tab-list players and the local player.",
    sections: [
      {
        title: "Lookup",
        entries: [
          { signature: "starfish.players.all() -> Player[]", description: "Every player in the tab list." },
          { signature: "starfish.players.byName(name) -> Player | nil", description: "Case-insensitive." },
          { signature: "starfish.players.byUuid(uuid) -> Player | nil" },
          { signature: "starfish.players.count() -> integer" },
          { signature: "starfish.players.me() -> LocalPlayer | nil", description: "You, as a live handle. nil until your uuid and name are known." },
          { signature: "starfish.players.distance(a, b) -> number", description: "Distance between two `Positionable`s." },
        ],
      },
      {
        title: "Player fields",
        table: {
          rows: [
            ["uuid", "string", ""],
            ["name", "string", ""],
            ["displayName", "string", ""],
            ["ping", "integer", ""],
            ["gamemode", "string", "survival, creative, adventure, or spectator"],
            ["team", "Team | nil", "See Scoreboard"],
            ["entityId", "integer | nil", "nil while the player isn't in render distance"],
            ["properties", "table", "Keyed by property name, each {name, value, signature?}"],
          ],
        },
      },
      {
        title: "LocalPlayer fields",
        description: "Fields always read the current state, so a handle can be kept and read later.",
        table: {
          rows: [
            ["uuid", "string | nil", ""],
            ["name", "string | nil", ""],
            ["displayName", "string | nil", ""],
            ["team", "Team | nil", ""],
            ["entityId", "integer | nil", ""],
            ["position", "{x, y, z}", ""],
            ["rotation", "{yaw, pitch}", ""],
            ["health", "number", ""],
            ["food", "integer", ""],
            ["saturation", "number", ""],
            ["heldSlot", "integer", ""],
            ["experience", "{bar, level}", ""],
            ["gamemode", "string", ""],
          ],
        },
      },
    ],
    notes: [],
  },
  {
    slug: "entities",
    title: "Entities",
    group: "Game State",
    summary: "Read tracked entity state.",
    sections: [
      {
        title: "Lookup",
        entries: [
          { signature: "starfish.entities.all() -> Entity[]" },
          { signature: "starfish.entities.byId(id) -> Entity | nil" },
          { signature: "starfish.entities.byUuid(uuid) -> Entity | nil" },
          { signature: "starfish.entities.players() -> Entity[]", description: "Tracked player entities." },
          { signature: "starfish.entities.near(position, radius) -> Entity[]", description: "Entities within `radius` blocks of a `Positionable`." },
        ],
      },
      {
        title: "Events",
        entries: [
          {
            signature: "starfish.entities.onStateChange(handler) -> Subscription",
            description: "Fires when an entity's state flags change, with `{entity, changed, state}`; `changed` has every state key, true where it changed.",
          },
          { signature: "starfish.entities.onSpawn(handler) -> Subscription" },
          { signature: "starfish.entities.onDespawn(handler) -> Subscription" },
          { signature: "starfish.entities.onMove(handler) -> Subscription" },
          {
            signature: "starfish.entities.onEquipmentChange(handler) -> Subscription",
            description: "Shorthands for `entity:spawn`, `entity:despawn`, `entity:move`, and `entity:equipmentChange` (see Events).",
          },
        ],
      },
      {
        title: "Entity fields",
        table: {
          rows: [
            ["id", "integer", ""],
            ["kind", "string", "e.g. minecraft:player, minecraft:zombie"],
            ["uuid", "string | nil", ""],
            ["onGround", "boolean", ""],
            ["position", "{x, y, z}", ""],
            ["velocity", "{x, y, z}", ""],
            ["rotation", "{yaw, pitch}", ""],
            ["state", "table", "{sneaking, sprinting, usingItem, invisible, onFire}"],
            ["equipment", "table", "{hand, boots, leggings, chestplate, helmet}, each an Item or nil"],
          ],
        },
      },
    ],
    notes: [],
  },
  {
    slug: "world",
    title: "World",
    group: "Game State",
    summary: "Read block, chunk, and dimension state.",
    sections: [
      {
        title: "Blocks and chunks",
        entries: [
          { signature: "starfish.world.block(position) -> {id} | nil", description: "The block at a `Positionable`, coordinates floored." },
          { signature: "starfish.world.isChunkLoaded(chunkX, chunkZ) -> boolean" },
          { signature: "starfish.world.loadedChunks() -> {x, z}[]" },
          { signature: "starfish.world.chunkCount() -> integer" },
        ],
      },
      {
        title: "Dimension",
        entries: [
          { signature: "starfish.world.biome(x, z) -> Identifier | nil" },
          { signature: "starfish.world.dimension() -> Identifier | nil" },
          { signature: "starfish.world.minY() -> integer", description: "The lowest block Y: 0." },
          { signature: "starfish.world.height() -> integer", description: "The world height in blocks: 256." },
        ],
      },
    ],
    notes: [
      "Only chunks the server has sent you are known; `block` returns nil anywhere else.",
    ],
  },
  {
    slug: "inventory",
    title: "Inventory",
    group: "Game State",
    summary: "Read the local player's inventory and build items.",
    sections: [
      {
        title: "Reading",
        entries: [
          { signature: "starfish.inventory.slot(slot) -> Item | nil" },
          { signature: "starfish.inventory.heldItem() -> Item | nil" },
          { signature: "starfish.inventory.heldSlot() -> integer" },
          { signature: "starfish.inventory.armor() -> {helmet?, chestplate?, leggings?, boots?}", description: "Keys are present only when occupied." },
          { signature: "starfish.inventory.windowId() -> integer", description: "0 when no window is open." },
          { signature: "starfish.inventory.isWindowOpen() -> boolean" },
        ],
      },
      {
        title: "Building",
        entries: [
          {
            signature: "starfish.inventory.item(opts) -> Item",
            description: "Builds an item for `starfish.client` windows and entities. `opts`: `id`, `count?` (default 1), `name?`, `lore?`, `enchantments?` (`{id, level}[]`). Errors on an unknown id.",
          },
        ],
      },
      {
        title: "Item fields",
        table: {
          rows: [
            ["id", "string", ""],
            ["count", "integer", ""],
            ["name", "string | nil", ""],
            ["lore", "string[]", ""],
            ["durability", "{damage, max} | nil", ""],
            ["enchantments", "{id, level}[]", ""],
          ],
        },
      },
      {
        title: "WindowType",
        description:
          "`starfish.inventory.WindowType` holds the window type ids for `client.window.openWindow`: `CHEST`, `CRAFTING_TABLE`, `FURNACE`, `DISPENSER`, `ENCHANTING_TABLE`, `BREWING_STAND`, `VILLAGER`, `BEACON`, `ANVIL`, `HOPPER`, `DROPPER`, `HORSE`.",
      },
    ],
    notes: [
      "An enchanted book's `enchantments` is empty: only an item's applied enchantments are read, not the ones a book stores.",
    ],
  },
  {
    slug: "scoreboard",
    title: "Scoreboard",
    group: "Game State",
    summary: "Read teams, objectives, and scores.",
    sections: [
      {
        title: "Teams",
        entries: [
          { signature: "starfish.scoreboard.teams() -> Team[]" },
          { signature: "starfish.scoreboard.team(name) -> Team | nil" },
        ],
        table: {
          rows: [
            ["name", "string", ""],
            ["displayName", "string", ""],
            ["prefix", "string", ""],
            ["suffix", "string", ""],
            ["color", "string | nil", ""],
            ["nameTagVisibility", "string", ""],
            ["players", "string[]", ""],
          ],
        },
      },
      {
        title: "Objectives and scores",
        entries: [
          { signature: "starfish.scoreboard.objectives() -> Objective[]" },
          { signature: "starfish.scoreboard.objective(name) -> Objective | nil", description: "An `Objective` is `{name, displayName, type}`." },
          { signature: "starfish.scoreboard.scores(objective) -> {name, score}[]", description: "Highest score first." },
          { signature: "starfish.scoreboard.score(objective, entry) -> integer | nil" },
        ],
      },
      {
        title: "Display slots",
        entries: [
          { signature: "starfish.scoreboard.sidebar() -> {title, lines} | nil", description: "The sidebar objective; `lines` are `{name, score}`, highest first." },
          { signature: "starfish.scoreboard.displayed(slot) -> Objective | nil", description: "`slot` is `list`, `sidebar`, or `belowName`; anything else errors." },
        ],
      },
    ],
    notes: [],
  },
  {
    slug: "network",
    title: "Network",
    group: "Network",
    summary: "HTTP, WebSockets, a local HTTP server, and plugin-channel messages.",
    sections: [
      {
        title: "starfish.http",
        description: "Requests run in the background; the callback receives a response table on a later tick.",
        entries: [
          { signature: "starfish.http.get(url, callback) -> nil" },
          { signature: "starfish.http.get(url, opts, callback) -> nil", description: "`opts.headers` is the only option." },
          { signature: "starfish.http.getBinary(url, callback) -> nil" },
          { signature: "starfish.http.getBinary(url, opts, callback) -> nil", description: "Like `get`, with the raw bytes in `response.binary`." },
          { signature: "starfish.http.post(url, callback) -> nil" },
          {
            signature: "starfish.http.post(url, opts, callback) -> nil",
            description: "`opts.body` (a string, or a table sent as JSON) and `opts.headers`. `Content-Type` defaults to `application/json`.",
          },
          { signature: "starfish.http.request(opts, callback) -> nil", description: "`opts`: `url`, `method?` (default GET), `headers?`, `body?`, `binary?`." },
          { signature: "starfish.http.encodeUri(text) -> string" },
          { signature: "starfish.http.decodeUri(text) -> string" },
        ],
        table: {
          rows: [
            ["success", "boolean", ""],
            ["status", "integer | nil", "Missing when the request never reached the server"],
            ["body", "string | nil", ""],
            ["data", "any | nil", "The body decoded as JSON, when it is JSON"],
            ["binary", "string | nil", "For getBinary and binary requests"],
            ["size", "integer | nil", ""],
            ["error", "string | nil", "Set when success is false"],
          ],
        },
      },
      {
        title: "starfish.websocket",
        entries: [
          {
            signature: "starfish.websocket.connect(url, opts?) -> Connection",
            description:
              "Returns a handle immediately; `onOpen` fires once connected. `opts`: `headers`, `onOpen(conn)`, `onMessage(conn, message, isBinary)`, `onClose(conn, code?, reason?)`, `onError(conn, message)`.",
          },
          {
            signature: "starfish.websocket.onWebSocket(path, handlers) -> nil",
            description: "Accepts connections to `path` on Starfish's local server, with the same callbacks; `onOpen(conn, request)` also gets `{path, headers, query}`. Registering a path again replaces its handlers.",
          },
          { signature: "conn:send(text) -> boolean" },
          { signature: "conn:sendBinary(data) -> boolean" },
          { signature: "conn:close() -> boolean" },
          { signature: "conn:isOpen() -> boolean", description: "Send and close return false when the connection isn't open." },
        ],
      },
      {
        title: "starfish.endpoint",
        description: "Handlers on Starfish's local HTTP server, so external tools can call into a running plugin.",
        entries: [
          {
            signature: "starfish.endpoint.register(path, opts, handler) -> nil",
            description:
              "`path` gets a leading `/`; a trailing `/*` matches every sub-path. `opts.methods` defaults to GET and POST. `handler(request)` gets `{path, method, headers, query, body?, data?}` and returns nil (empty 200), a string (200 body), or `{status?, headers?, body?}`.",
          },
          { signature: "starfish.endpoint.unregister(path) -> boolean" },
          { signature: "starfish.endpoint.list() -> string[]" },
        ],
      },
      {
        title: "starfish.server",
        entries: [
          {
            signature: "starfish.server.sendPluginMessage(channel, payload) -> nil",
            description: "`channel` is lowercase `namespace:path`, `REGISTER`, or `UNREGISTER`; `payload` is a byte array.",
          },
          { signature: "starfish.server.blockChannel(channel) -> nil", description: "Hides messages on `channel` from the game." },
          { signature: "starfish.server.onPluginMessage(channel, handler) -> Subscription", description: "`handler(payload)` for messages on one channel." },
        ],
      },
    ],
    notes: [
      "`endpoint.register` takes `opts` positionally: pass `nil` for the defaults, as in `register(\"/ping\", nil, handler)`.",
    ],
  },
  {
    slug: "events",
    title: "Events",
    group: "Core",
    summary: "Subscribe to game events, emit your own, and run timers.",
    sections: [
      {
        title: "Subscription",
        entries: [{ signature: "sub:off() -> nil", description: "Cancels a handler or timer. Every `on...` registration and timer returns one." }],
      },
      {
        title: "starfish.events",
        entries: [
          {
            signature: "starfish.events.on(name, handler) -> Subscription",
            description:
              "`name` is `namespace:event`. The core namespaces (`chat`, `config`, `entity`, `inventory`, `player`, `plugin`, `scoreboard`, `server`, `session`, `team`, `world`) are reserved: a name under them must be a real core event, so a typo errors instead of never firing. Any other namespace is free for your own events.",
          },
          { signature: "starfish.events.once(name, handler) -> Subscription", description: "Like `on`, cancelled after the first call." },
          { signature: "starfish.events.off(subscription) -> nil", description: "Same as `subscription:off()`." },
          { signature: "starfish.events.emit(name, data?) -> nil", description: "Runs this plugin's handlers immediately; other plugins receive it on the next tick." },
        ],
      },
      {
        title: "starfish.timers",
        entries: [
          { signature: "starfish.timers.everyTick(callback) -> Subscription", description: "Every tick (5 ms)." },
          { signature: "starfish.timers.delay(ms, callback) -> Subscription", description: "Once, after `ms` milliseconds rounded up to a whole tick." },
          { signature: "starfish.timers.interval(ms, callback) -> Subscription", description: "Every `ms` milliseconds until cancelled." },
        ],
      },
      {
        title: "Core events",
        description: "Handlers receive one payload table. `entity` fields are references `{entityId, type, uuid}` (or nil); look them up with `starfish.entities.byId`.",
        table: {
          columns: ["Event", "Payload", "Notes"],
          rows: [
            ["chat:receive", "{message, json, kind}", ""],
            ["chat:send", "{message}", ""],
            ["chat:tabComplete", "{matches}", ""],
            ["chat:tabCompleteRequest", "{text, position?}", ""],
            ["chat:title", "{action, text?, message?, fadeIn, stay, fadeOut}", ""],
            ["config:changed", "{plugin, key, value}", "Only for your own plugin"],
            ["entity:animation", "{entity, animation}", ""],
            ["entity:attach", "{entity, vehicle?, leash}", ""],
            ["entity:attributes", "{entity, properties}", ""],
            ["entity:collect", "{collected, collector}", ""],
            ["entity:despawn", "{entityIds}", ""],
            ["entity:effectAdd", "{entity, effect?, amplifier, duration, hideParticles}", ""],
            ["entity:effectRemove", "{entity, effect}", ""],
            ["entity:equipmentChange", "{entity, isPlayer, slot, item?}", ""],
            ["entity:headLook", "{entity, headYaw}", ""],
            ["entity:metadata", "{entity, metadata}", ""],
            ["entity:move", "{entity, onGround, delta?, rotation?, position?}", ""],
            ["entity:nbt", "{entity, nbt}", ""],
            ["entity:spawn", "{entityId, kind, x, y, z, ...}", "Extra fields depend on kind"],
            ["entity:status", "{entity, status}", ""],
            ["entity:useBed", "{entity, position}", ""],
            ["entity:useItemStart", "{x, y, z, face, cursorX, cursorY, cursorZ}", "Sent by you"],
            ["entity:useItemStop", "{x, y, z, face}", "Sent by you"],
            ["entity:velocity", "{entity, velocity}", ""],
            ["inventory:creativeAction", "{slot, item}", "Sent by you"],
            ["inventory:enchantItem", "{windowId, enchantment}", "Sent by you"],
            ["inventory:mapData", "{mapId, scale, icons, columns, rows, x, y, data}", ""],
            ["inventory:slotUpdate", "{windowId, slot, item}", ""],
            ["inventory:transactionAck", "{windowId, action, accepted}", "Sent by you"],
            ["inventory:transactionConfirm", "{windowId, action, accepted}", ""],
            ["inventory:windowClick", "{windowId, slot, button, mode, actionNumber}", "Sent by you"],
            ["inventory:windowClose", "{windowId}", ""],
            ["inventory:windowCloseRequest", "{windowId}", "Sent by you"],
            ["inventory:windowItems", "{windowId, items}", ""],
            ["inventory:windowOpen", "{windowId, windowType, title, slotCount, entityId}", ""],
            ["inventory:windowProperty", "{windowId, property, value}", ""],
            ["player:abilities", "{invulnerable, flying, canFly, creative, flySpeed, fovModifier}", ""],
            ["player:action", "{action, jumpBoost}", "Sent by you"],
            ["player:camera", "{entity}", ""],
            ["player:combatEvent", "{event, duration, entity, victim, message, json}", ""],
            ["player:experienceUpdate", "{bar, level, total}", ""],
            ["player:healthUpdate", "{health, food, saturation}", ""],
            ["player:interact", "{entity, action, position?}", "Sent by you"],
            ["player:join", "{uuid, name, displayName, gamemode, ping}", "A player was added to the tab list"],
            ["player:listHeaderFooter", "{header, footer, headerJson, footerJson}", ""],
            ["player:listUpdate", "{action, players}", ""],
            ["player:move", "{position?, rotation?, onGround}", "Sent by you"],
            ["player:positionUpdate", "{x, y, z, yaw, pitch}", "The server moved you"],
            ["player:settings", "{locale, viewDistance, chatMode, chatColors, displayedSkinParts}", "Sent by you"],
            ["player:spectate", "{target}", "Sent by you"],
            ["player:statistics", "{entries}", ""],
            ["player:statusRequest", "{action}", "Sent by you"],
            ["player:steerVehicle", "{sideways, forward, jump, unmount}", "Sent by you"],
            ["player:swingArm", "{}", "Sent by you"],
            ["scoreboard:displayChange", "{position, objectiveName}", ""],
            ["scoreboard:objectiveUpdate", "{objectiveName, mode, displayName, type}", ""],
            ["scoreboard:scoreUpdate", "{scoreName, objectiveName, action, value}", ""],
            ["server:packetReceive", "{id}", "Every incoming packet"],
            ["server:pluginMessage", "{channel, data}", ""],
            ["server:pluginMessageOutgoing", "{channel, data}", "Sent by you"],
            ["server:resourcePackPrompt", "{url, hash}", ""],
            ["server:resourcePackStatus", "{hash, result}", "Sent by you"],
            ["session:attached", "{}", "Starfish hooked into the game's connection"],
            ["session:detached", "{}", "Starfish lost the game's connection"],
            ["session:join", "{entityId, gamemode, dimension, difficulty, maxPlayers, levelType}", "You joined a server"],
            ["team:update", "{mode, teamName, displayName, prefix, suffix, friendlyFire, nameTagVisibility, color, players}", ""],
            ["world:blockAction", "{position, byte1, byte2, blockType}", ""],
            ["world:blockBreakAnimation", "{entity, entityId, x, y, z, destroyStage}", ""],
            ["world:blockChange", "{x, y, z, blockId, meta}", ""],
            ["world:blockEntity", "{position, action, nbt}", ""],
            ["world:border", "{action, diameter, oldDiameter, newDiameter, speed, center?, ...}", ""],
            ["world:chunkLoad", "{chunkX, chunkZ}", ""],
            ["world:chunkUnload", "{chunkX, chunkZ}", ""],
            ["world:difficulty", "{difficulty}", ""],
            ["world:dimensionChange", "{from, to}", ""],
            ["world:effect", "{effectId, position, data, global}", ""],
            ["world:explosion", "{position, radius, affectedBlocks, playerMotion}", ""],
            ["world:gameState", "{reason, value}", ""],
            ["world:particle", "{particleId, longDistance, position, offset, speed, count}", ""],
            ["world:respawn", "{dimension, difficulty, gamemode}", ""],
            ["world:signEdit", "{position, lines}", "Sent by you"],
            ["world:sound", "{name, position, volume, pitch}", ""],
            ["world:spawnPosition", "{x, y, z}", ""],
            ["world:timeUpdate", "{worldAge, timeOfDay}", ""],
          ],
        },
      },
    ],
    notes: [],
  },
  {
    slug: "config",
    title: "Config",
    group: "Core",
    summary: "Saved settings and the settings UI shown in the Starfish app.",
    sections: [
      {
        title: "starfish.config",
        description: "Settings are saved per plugin and survive restarts.",
        entries: [
          { signature: "starfish.config.get(key, fallback?) -> any", description: "The saved value, else the schema default, else `config.defaults`, else `fallback`." },
          {
            signature: "starfish.config.set(key, value) -> boolean",
            description: "Saves a value and fires `config:changed`. Returns false, saving nothing, when the value can't be stored as JSON or doesn't match the setting's type.",
          },
          { signature: "starfish.config.defaults(values) -> nil", description: "Defaults for keys with no settings-UI entry. Each call replaces the last." },
          { signature: "starfish.config.onChange(key, handler) -> Subscription", description: "`handler(value, key)` when `key` changes." },
          { signature: "starfish.config.onChangeAny(handler) -> Subscription", description: "`handler(pluginName, key, value)` for every change to this plugin's settings." },
        ],
      },
      {
        title: "starfish.schema",
        description: "Declares the settings the Starfish app shows on the plugin's page.",
        entries: [
          { signature: "starfish.schema.section(section) -> nil", description: "Adds a section: `{key, label, description?, settings}`." },
          { signature: "starfish.schema.get() -> {sections} | nil" },
          { signature: "starfish.schema.clear() -> nil", description: "Removes every section this plugin declared." },
        ],
        table: {
          rows: [
            ["key", "string", "The config key"],
            ["type", "string", "toggle, soundToggle, cycle, button, or text"],
            ["label", "string?", ""],
            ["description", "string?", ""],
            ["default", "any?", "toggle: false, soundToggle: true, cycle: first value, text: empty"],
            ["values", "{text, value}[]", "Required for cycle"],
            ["text", "string?", "Button caption. Default Click"],
            ["setCommand", "string?", "For text settings"],
          ],
        },
      },
    ],
    notes: [
      "A `defaults` key inside a section is ignored with a warning: use each setting's `default`, or `config.defaults`.",
    ],
  },
  {
    slug: "input",
    title: "Input",
    group: "Core",
    summary: "Keybinds, and exclusive text and mouse capture for overlay UI.",
    sections: [
      {
        title: "Keybinds",
        entries: [
          {
            signature: "starfish.input.bind(key, opts) -> nil",
            description: "`opts`: `onPress?`, `onRelease?`, `passThrough?` (also let the game see the key, default false), `activeInMenu?` (default false). Replaces this plugin's earlier bind for the key.",
          },
          { signature: "starfish.input.unbind(key) -> nil" },
          { signature: "starfish.input.isHeld(key) -> boolean" },
        ],
      },
      {
        title: "Key names",
        description:
          "Case-insensitive: `A`-`Z`, `0`-`9`, `F1`-`F12`, `ESCAPE`, `TAB`, `CAPSLOCK`, `SPACE`, `ENTER`, `BACKSPACE`, `LSHIFT`, `RSHIFT`, `LCONTROL`, `RCONTROL`, `LALT`, `RALT`, `UP`, `DOWN`, `LEFT`, `RIGHT`, `INSERT`, `DELETE`, `HOME`, `END`, `PAGEUP`, `PAGEDOWN`, `GRAVE`, `MINUS`, `EQUALS`, `LBRACKET`, `RBRACKET`, `BACKSLASH`, `SEMICOLON`, `APOSTROPHE`, `COMMA`, `PERIOD`, `SLASH`, `NUMPAD0`-`NUMPAD9`, `NUMPADENTER`, `NUMPADADD`, `NUMPADSUBTRACT`, `NUMPADMULTIPLY`, `NUMPADDIVIDE`, `NUMPADDECIMAL`.",
      },
      {
        title: "Cursor and support",
        entries: [
          { signature: "starfish.input.getCursor() -> (x, y)", description: "Last known free-cursor position." },
          { signature: "starfish.input.supported() -> boolean", description: "Whether input capture is running in the game." },
        ],
      },
      {
        title: "Text capture",
        entries: [
          {
            signature: "starfish.input.captureText(opts) -> TextSession",
            description: "Starts capturing typing, ending any earlier session. `opts`: `initial?`, `onChange?(state)`, `onSubmit?(text)`, `onCancel?()`.",
          },
          { signature: "session:get() -> {text, cursor, selection?} | nil", description: "`selection` is `{start, finish}`; nil once the session has ended." },
          { signature: "session:set(text, cursor?) -> nil" },
          { signature: "session:cursorFromX(offsetPx, size, extend) -> nil", description: "Moves the cursor to the character under a pixel offset, measured at text `size`." },
          { signature: "session:stop() -> nil" },
        ],
      },
      {
        title: "Mouse capture",
        entries: [
          {
            signature: "starfish.input.captureMouse(opts) -> MouseSession",
            description: "Captures the mouse for overlay UI, ending any earlier session. `opts`: `onMiss?(event)` for clicks outside every hit region (`{kind, x, y, button?, dy?}`), `onCancel?()`.",
          },
          { signature: "session:stop() -> nil" },
        ],
      },
    ],
    notes: [
      "`stop()` never fires `onCancel`. Starfish ends sessions itself, and does fire `onCancel`, when a text session loses focus (a click outside every hit region, or the game taking the mouse back) or when Escape is pressed during mouse capture.",
    ],
  },
  {
    slug: "plugins",
    title: "Plugins",
    group: "Core",
    summary: "Export functions and call into other plugins.",
    sections: [
      {
        title: "starfish.plugin",
        description: "This plugin's own identity and exports.",
        entries: [
          { signature: "starfish.plugin.export(name, fn) -> nil", description: "Makes `fn` callable by other plugins." },
          { signature: "starfish.plugin.unexport(name) -> nil" },
          { signature: "starfish.plugin.list() -> string[]", description: "This plugin's exported names." },
          { signature: "starfish.plugin.name() -> string" },
          { signature: "starfish.plugin.version() -> string" },
          { signature: "starfish.plugin.manifest() -> table", description: "`{name, version, displayName, prefix, author, credits, description, dependencies, requires}`, with dependencies as names." },
        ],
      },
      {
        title: "starfish.plugins",
        description: "Calling into other plugins.",
        entries: [
          {
            signature: "starfish.plugins.call(plugin, fn, ...) -> ...",
            description: "Calls another plugin's export. Errors if the plugin or export doesn't exist or the dependency isn't declared.",
          },
          { signature: "starfish.plugins.has(plugin) -> boolean" },
          { signature: "starfish.plugins.exports(plugin) -> string[] | nil" },
          { signature: "starfish.plugins.list() -> string[]", description: "Every loaded plugin." },
          {
            signature: "starfish.plugins.require(plugin) -> table",
            description: "A proxy whose functions call `plugin`'s exports. Errors unless `plugin` is a declared dependency.",
          },
          {
            signature: "starfish.plugins.optional(plugin) -> table | nil",
            description: "Like `require`, but nil instead of an error. Calls through the proxy return nil while the plugin isn't loaded, is older than your dependency's `minVersion`, or doesn't export the function.",
          },
        ],
      },
    ],
    notes: [
      "Arguments and results cross between plugins as JSON values: functions, userdata, and metatables don't survive the trip.",
      "Cross-plugin calls can nest at most 100 deep.",
      "An optional dependency loaded but older than your `minVersion` counts as not loaded everywhere in `starfish.plugins`: `has`, `exports`, `list`, `call`, and `optional`.",
    ],
  },
  {
    slug: "client",
    title: "Client",
    group: "Core",
    summary: "Change what you see without telling the server.",
    sections: [
      {
        title: "starfish.client.player",
        entries: [
          { signature: "starfish.client.player.setHealth(opts) -> nil", description: "`{health, food, saturation}`, all required." },
          { signature: "starfish.client.player.setPosition(opts) -> nil", description: "`{x, y, z, yaw?, pitch?}`; yaw and pitch default to 0." },
          { signature: "starfish.client.player.setExperience(opts) -> nil", description: "`{bar, level, total?}`." },
          { signature: "starfish.client.player.setAbilities(opts) -> nil", description: "`{invulnerable?, flying?, allowFlying?, creative?, flySpeed?, walkSpeed?}`; speeds default to 0.05 and 0.1." },
          { signature: "starfish.client.player.setSpawn(position) -> nil" },
          { signature: "starfish.client.player.setHeldItemSlot(slot) -> nil" },
        ],
      },
      {
        title: "starfish.client.world",
        entries: [
          { signature: "starfish.client.world.setBlock(position, id) -> nil", description: "`id` such as `minecraft:stone`; unknown ids error." },
          { signature: "starfish.client.world.setBlocks(blocks) -> nil", description: "Each entry is `{x, y, z, id}`." },
          {
            signature: "starfish.client.world.spawnParticle(particleId, position, opts?) -> nil",
            description: "`particleId` from `starfish.client.world.Particle` (e.g. `Particle.HEART`). `opts`: `longDistance?`, `offsetX?`, `offsetY?`, `offsetZ?`, `speed?`, `count?` (default 1).",
          },
          { signature: "starfish.client.world.playSound(name, opts?) -> nil", description: "A sound name such as `note.pling`. `opts`: `position?` (default yours), `volume?`, `pitch?` (default 1)." },
          { signature: "starfish.client.world.setTime(worldAge, timeOfDay) -> nil" },
          { signature: "starfish.client.world.explosion(position, opts?) -> nil", description: "`opts`: `radius?` (default 1), `records?` (destroyed blocks as relative `{x, y, z}`), `motion?`." },
          {
            signature: "starfish.client.world.worldEvent(effect, position, opts?) -> nil",
            description: "`effect` is a numeric id or a name such as `smoke`, `blockBreak`, `fireworkShot`, or `ghastShoots`; unknown names error. `opts`: `data?` (default 0), `global?` (default false).",
          },
          {
            signature: "starfish.client.world.gameState(reason, value) -> nil",
            description: "`reason`: `invalidBed`, `endRaining`, `beginRaining`, `changeGamemode`, `enterCredits`, `demoMessage`, `arrowHitPlayer`, `fadeValue`, `fadeTime`, or `elderGuardian`.",
          },
        ],
      },
      {
        title: "starfish.client.window",
        entries: [
          { signature: "starfish.client.window.openWindow(windowId, windowType, title, slotCount) -> nil", description: "`windowType` from `starfish.inventory.WindowType`." },
          { signature: "starfish.client.window.closeWindow(windowId) -> nil" },
          { signature: "starfish.client.window.createChest(title, size?) -> windowId", description: "Opens a chest window; `size` defaults to 27." },
          { signature: "starfish.client.window.createHopper(title) -> windowId" },
          { signature: "starfish.client.window.createDispenser(title) -> windowId" },
          { signature: "starfish.client.window.setSlot(windowId, slot, item?) -> nil" },
          { signature: "starfish.client.window.setWindowItems(windowId, items) -> nil", description: "Items are read up to the first nil." },
          { signature: "starfish.client.window.fillWindow(windowId, item?) -> nil" },
          { signature: "starfish.client.window.clearWindow(windowId) -> nil" },
          { signature: "starfish.client.window.clickSlot(windowId, slot, button?, mode?, item?) -> nil", description: "`button` and `mode` default to 0." },
          { signature: "starfish.client.window.sendTransaction(windowId, action, accepted) -> nil" },
          { signature: "starfish.client.window.sendCraftProgress(windowId, property, value) -> nil" },
          { signature: "starfish.client.window.openSignEditor(position) -> nil" },
        ],
      },
      {
        title: "starfish.client.entity",
        entries: [
          { signature: "starfish.client.entity.spawnPlayer(opts) -> nil", description: "`{entityId, uuid, position, yaw?, pitch?, metadata?}`." },
          { signature: "starfish.client.entity.spawnObject(opts) -> nil", description: "`{entityId, entityType, position, yaw?, pitch?, data?}`." },
          { signature: "starfish.client.entity.spawnMob(opts) -> nil", description: "`{entityId, entityType, position, yaw?, pitch?, headPitch?, metadata?}`." },
          { signature: "starfish.client.entity.destroy(entityIds) -> nil" },
          { signature: "starfish.client.entity.teleport(entityId, position, opts?) -> nil", description: "`opts`: `yaw?`, `pitch?`, `onGround?` (default true)." },
          { signature: "starfish.client.entity.move(entityId, opts) -> nil", description: "`{dx, dy, dz, onGround?}` in protocol units (1/32 block)." },
          { signature: "starfish.client.entity.look(entityId, opts) -> nil", description: "`{yaw, pitch, onGround?}`." },
          { signature: "starfish.client.entity.moveLook(entityId, opts) -> nil", description: "`{dx, dy, dz, yaw, pitch, onGround?}`." },
          { signature: "starfish.client.entity.setVelocity(entityId, opts) -> nil", description: "`{x, y, z}` in protocol units (1/8000 block per tick)." },
          { signature: "starfish.client.entity.setHeadRotation(entityId, headYaw) -> nil" },
          {
            signature: "starfish.client.entity.setMetadata(entityId, entries) -> nil",
            description: "Each entry is `{index, type, value}`, `type` one of `byte`, `short`, `int`, `float`, `string`, `slot`, `position`, `rotation`.",
          },
          { signature: "starfish.client.entity.setEquipment(entityId, slot, item?) -> nil", description: "`slot`: `held`, `boots`, `leggings`, `chestplate`, or `helmet`." },
          { signature: "starfish.client.entity.animate(entityId, animation) -> nil" },
          { signature: "starfish.client.entity.setStatus(entityId, status) -> nil" },
          { signature: "starfish.client.entity.addEffect(entityId, effect, opts?) -> nil", description: "`effect` such as `minecraft:speed`. `opts`: `amplifier?` (0), `duration?` (200), `hideParticles?`." },
          { signature: "starfish.client.entity.removeEffect(entityId, effect) -> nil" },
          { signature: "starfish.client.entity.attach(entityId, vehicleId, leash?) -> nil" },
          { signature: "starfish.client.entity.collectItem(collectedId, collectorId) -> nil" },
        ],
      },
    ],
    notes: [
      "`setVelocity`, `addEffect`, `removeEffect`, and `attach` error when given your own entity id.",
    ],
  },
  {
    slug: "utilities",
    title: "Utilities",
    group: "Core",
    summary: "Logging, JSON, base64, byte encoding, and time.",
    sections: [
      {
        title: "starfish.log",
        entries: [
          { signature: "starfish.log.info(message) -> nil" },
          { signature: "starfish.log.warn(message) -> nil" },
          { signature: "starfish.log.error(message) -> nil" },
          { signature: "starfish.log.debug(message) -> nil", description: "Shown only while the plugin's `debug` setting is on." },
        ],
      },
      {
        title: "starfish.json",
        entries: [
          { signature: "starfish.json.encode(value) -> string", description: "Empty tables encode as `{}`. Errors on cycles." },
          { signature: "starfish.json.decode(text) -> any", description: "JSON `null` becomes nil." },
        ],
      },
      {
        title: "starfish.base64",
        entries: [
          { signature: "starfish.base64.encode(text) -> string" },
          { signature: "starfish.base64.decode(data) -> string | nil", description: "nil when `data` isn't base64 or doesn't decode to text." },
        ],
      },
      {
        title: "starfish.encoding",
        description: "Minecraft protocol values as byte arrays, e.g. for plugin-channel messages.",
        entries: [
          { signature: "starfish.encoding.writer() -> Writer", description: "Methods: `varint`, `string`, `bool`, `byte`, `bytes`, `uuid`, `clear`, then `build() -> bytes` and `len()`." },
          {
            signature: "starfish.encoding.reader(bytes) -> Reader",
            description: "Methods: `varint`, `string`, `bool`, `byte`, `uuid`, `optionalString`, `remaining`, `skip(count)`, `position`.",
          },
          { signature: "starfish.encoding.decodePng(data) -> string | nil", description: "PNG file data to raw RGBA8 pixels for `overlay.loadTexture`." },
        ],
      },
      {
        title: "starfish.time",
        entries: [
          { signature: "starfish.time.now() -> integer", description: "Unix time in milliseconds." },
          { signature: "starfish.time.monotonic() -> integer", description: "Milliseconds on a clock that never goes backwards." },
          { signature: "starfish.time.since(monotonic) -> integer", description: "Milliseconds since a `monotonic()` reading." },
        ],
      },
      {
        title: "Globals",
        entries: [
          { signature: "starfish.version", description: "The running Starfish version string." },
          { signature: "json", description: "The same table as `starfish.json`." },
          { signature: "print(...) -> nil", description: "Logs its arguments at info level." },
        ],
      },
    ],
    notes: [
      "`Writer` methods return nothing, so they don't chain: call them one per line.",
      "`base64.encode` only accepts text, not arbitrary binary data.",
    ],
  },
];

export function docPage(slug: string): DocPage | undefined {
  return DOC_PAGES.find((page) => page.slug === slug);
}

export function docsNavigation(): NavSection[] {
  return docPagesByGroup().map(({ group, pages }) => ({
    label: group,
    items: [
      ...(group === "Getting Started" ? [{ title: "Introduction", href: "/docs" }] : []),
      ...pages.map((page) => ({ title: page.title, href: `/docs/${page.slug}` })),
    ],
  }));
}

export function docPagesByGroup(): { group: DocGroup; pages: DocPage[] }[] {
  return DOC_GROUPS.map((group) => ({ group, pages: DOC_PAGES.filter((page) => page.group === group) }));
}
