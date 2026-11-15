import { call } from "@/lib/api";
import { el } from "@/lib/dom";
import { refLink } from "@/lib/refs";
import type { Reviews } from "@/lib/reviews/common";
import { smallButton } from "@/lib/server/common";
import { ago } from "@/lib/time";

const DIVIDER = /^\s*-{3,}\s*$/;
const INLINE = /(\*\*[^*]+\*\*|`[^`]+`)/g;
const BLOCKS: [string, string][] = [
  ["### ", "dg-h3"],
  ["## ", "dg-h2"],
  ["# ", "dg-h1"],
  ["-# ", "dg-small"],
  ["- ", "dg-li"],
  ["* ", "dg-li"],
  ["> ", "dg-quote"],
];
const BOT_NAME = "Urchin";
const PING_BUTTON = "Ping Me For Reviews";
const SYNTAX_HINT =
  "Discord markdown: # ## ### headings, **bold**, `code`, - lists, -# small text, and --- on its own line for a divider.";

let unsaved = false;
addEventListener("beforeunload", (event) => {
  if (unsaved) event.preventDefault();
});

export function guideTab(reviews: Reviews) {
  const saved = reviews.guide.content.body;
  const input = el("textarea", {
    className: "guide-input",
    spellcheck: false,
    value: saved,
  });
  input.setAttribute("aria-label", "Guide");
  const preview = el("div", { className: "dmsg-body" });
  const actions = el("span", { className: "actions" });
  const note = el("p", { className: "muted guide-note" });
  const update = () => {
    const dirty = input.value !== saved;
    unsaved = dirty;
    preview.replaceChildren(...renderGuide(input.value));
    note.textContent = dirty
      ? "Unsaved changes. Saving keeps them here; posting to Discord is a separate step."
      : SYNTAX_HINT;
    actions.replaceChildren(
      ...(dirty
        ? [
            smallButton(
              "Save",
              () => save(reviews, input.value),
              "button small primary",
            ),
            smallButton("Discard Changes", () => {
              input.value = saved;
              update();
            }),
          ]
        : []),
    );
  };
  input.addEventListener("input", update);
  update();
  return el(
    "div",
    { className: "stack" },
    publishNotice(reviews),
    el(
      "div",
      { className: "guide" },
      el(
        "section",
        { className: "panel guide-edit" },
        el("h2", { className: "panel-title" }, "Guide", actions),
        input,
        note,
      ),
      el(
        "section",
        { className: "panel guide-preview" },
        el("h2", { className: "panel-title" }, "Preview"),
        discordMessage(preview),
      ),
    ),
  );
}

async function save(reviews: Reviews, body: string) {
  try {
    await call("/moderation/guide", "PUT", { body });
    unsaved = false;
    reviews.report("Guide saved.");
    reviews.refresh();
  } catch (error) {
    reviews.report((error as Error).message, true);
  }
}

function publishNotice(reviews: Reviews) {
  const { status } = reviews.guide;
  const forum = reviews.directory?.channels.find(
    (channel) => channel.id === status.forum_channel_id,
  );
  const where = forum ? `#${forum.name}` : "the review forum";
  const publish = (label: string, question: string) =>
    smallButton(
      label,
      async () => {
        if (!confirm(question)) return;
        try {
          await call("/moderation/guide/publish", "POST", {});
          reviews.report("Guide posted and pinned.");
          reviews.refresh();
        } catch (error) {
          reviews.report((error as Error).message, true);
        }
      },
      "button small primary",
    );
  if (!status.posted)
    return el(
      "div",
      { className: "notice-row" },
      el(
        "span",
        {},
        `The guide hasn't been posted yet. Posting pins it as a thread in ${where}.`,
      ),
      publish(
        "Post Guide",
        `Post the saved guide as a pinned thread in ${where}?`,
      ),
    );
  const text = el(
    "span",
    {},
    `Posted and pinned in ${where} · ${ago(status.posted_at)}`,
    ...(status.posted_by
      ? [" by ", refLink(status.posted_by, reviews.guide.labels)]
      : []),
    status.up_to_date
      ? " · up to date"
      : " · the saved guide has changes that aren't posted yet",
  );
  return el(
    "div",
    { className: status.up_to_date ? "notice-row" : "notice-row warn" },
    text,
    ...(status.up_to_date
      ? []
      : [
          publish(
            "Update Posted Guide",
            "Edit the posted guide in Discord to match the saved one?",
          ),
        ]),
  );
}

function discordMessage(body: HTMLElement) {
  return el(
    "div",
    { className: "dmsg" },
    el(
      "span",
      { className: "dmsg-avatar", ariaHidden: "true" },
      BOT_NAME.charAt(0),
    ),
    el(
      "div",
      { className: "dmsg-main" },
      el(
        "div",
        { className: "dmsg-author" },
        el("b", {}, BOT_NAME),
        el("span", { className: "dmsg-app" }, "APP"),
      ),
      el(
        "div",
        { className: "dmsg-container" },
        body,
        el("span", { className: "dmsg-button" }, PING_BUTTON),
      ),
    ),
  );
}

export function renderGuide(body: string) {
  return body.split("\n").map((line) => {
    if (DIVIDER.test(line)) return el("hr", { className: "dg-hr" });
    const [prefix, className] = BLOCKS.find(([start]) =>
      line.startsWith(start),
    ) ?? ["", "dg-p"];
    return el("div", { className }, ...inline(line.slice(prefix.length)));
  });
}

function inline(text: string) {
  return text
    .split(INLINE)
    .filter(Boolean)
    .map((part) => {
      if (part.length > 4 && part.startsWith("**") && part.endsWith("**"))
        return el("b", {}, part.slice(2, -2));
      if (part.length > 2 && part.startsWith("`") && part.endsWith("`"))
        return el("code", {}, part.slice(1, -1));
      return part;
    });
}
