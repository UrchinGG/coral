import { call, load } from "@/lib/api";
import { el } from "@/lib/dom";
import {
  apply,
  membersPhrase,
  smallButton,
  type Server,
} from "@/lib/server/common";

interface Source {
  id: string;
  name: string;
  base_url: string;
  max_age_secs: number;
  last_seq: number | null;
  feed: "connecting" | "connected" | "none" | "disconnected";
  error: string | null;
}

export async function sourcesTab(server: Server) {
  const sources = await load<Source[]>("/server/sources");
  const used = new Set(server.config.source_ids);
  const chosen = new Set(used);
  const save = smallButton(
    `Apply to ${membersPhrase(server)}`,
    () =>
      apply(
        server,
        "/sources/used",
        "PUT",
        { source_ids: [...chosen] },
        "Data sources saved",
      ),
    "button primary",
  );
  const sync = () =>
    (save.hidden =
      chosen.size === used.size && [...used].every((id) => chosen.has(id)));
  sync();
  const rows = sources.map((source) => {
    const box = el("input", { type: "checkbox", checked: used.has(source.id) });
    box.addEventListener("change", () => {
      if (box.checked) chosen.add(source.id);
      else chosen.delete(source.id);
      sync();
    });
    return el(
      "div",
      { className: "setting" },
      el(
        "label",
        { className: "check source" },
        box,
        el(
          "span",
          {},
          el("b", { className: "mono" }, source.name),
          el("span", { className: "muted block" }, source.base_url),
          el("span", { className: "muted block" }, freshness(source)),
          ...(source.error
            ? [el("span", { className: "bad muted block" }, source.error)]
            : []),
        ),
      ),
      smallButton(
        "Remove",
        () =>
          confirm(
            `Remove ${source.name}? Formats and rules that read it stop matching.`,
          ) &&
          apply(
            server,
            `/sources/${source.id}`,
            "DELETE",
            undefined,
            `Removed ${source.name}`,
          ),
        "button small danger",
      ),
    );
  });
  return el(
    "div",
    { className: "stack" },
    el(
      "section",
      { className: "panel wide" },
      el(
        "h2",
        { className: "panel-title" },
        `Data Sources · ${sources.length}`,
        save,
      ),
      ...(rows.length
        ? rows
        : [
            el(
              "p",
              { className: "muted" },
              "No data sources yet. Formats and autoroles read a source's fields as name.field.",
            ),
          ]),
    ),
    addSource(server),
  );
}

function addSource(server: Server) {
  const field = (
    label: string,
    name: string,
    extra: Partial<HTMLInputElement> = {},
  ) =>
    el(
      "label",
      { className: "ffield" },
      el("span", { className: "label" }, label),
      el("input", { className: "field", name, ...extra }),
    );
  const form = el(
    "form",
    { className: "inline-form" },
    field("Name", "name", { required: true, placeholder: "stats" }),
    field("Base URL", "base_url", {
      required: true,
      type: "url",
      placeholder: "https://example.com/coral/v1",
    }),
    field("Key", "api_key", { type: "password", placeholder: "Optional" }),
    el("button", { className: "button primary" }, "Add source"),
  );
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const values = new FormData(form);
    const name = String(values.get("name")).trim();
    try {
      await call("/server/sources", "POST", {
        name,
        base_url: String(values.get("base_url")).trim(),
        api_key: String(values.get("api_key")) || null,
      });
      server.report(`Added ${name}.`);
      server.refresh();
    } catch (error) {
      server.report((error as Error).message, true);
    }
  });
  return el(
    "section",
    { className: "panel wide" },
    el("h2", { className: "panel-title" }, "Add a Data Source"),
    form,
  );
}

function freshness(source: Source) {
  const kept = `values kept for ${Math.round(source.max_age_secs / 60)} min`;
  switch (source.feed) {
    case "connected":
      return source.last_seq === null
        ? "Feed connected"
        : `Feed connected; last change #${source.last_seq}`;
    case "none":
      return `No change feed; ${kept}`;
    case "connecting":
      return "Connecting to the feed…";
    case "disconnected":
      return `Feed disconnected; ${kept} until it is back`;
  }
}
