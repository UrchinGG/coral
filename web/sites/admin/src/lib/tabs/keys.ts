import { el, panel, row } from "@/lib/dom";
import { FORMATS, titleCase } from "@/lib/format";
import { day } from "@/lib/time";

const LABEL_MAX_CHARS = 64;
const MAX_RATE_LIMIT = 100_000;
const DEFAULT_RATE_LIMIT = 600;
const HIDDEN_SECRET = "••••••••••••••••";

const PERMISSIONS: [number, string][] = [
  [1 << 0, "Player Data"],
  [1 << 1, "Hypixel"],
  [1 << 2, "All Sessions"],
  [1 << 3, "Link Accounts"],
  [1 << 4, "Tags"],
];

export interface DevKey {
  key: string;
  label: string;
  permissions: number;
  rate_limit: number;
  request_count: number;
  locked: boolean;
  created_at: string;
}

export interface WasteStanding {
  allowance: number;
  refill_per_day: number;
  spent: number;
  blocked_until: string | null;
}

export interface Budget {
  limiter: string;
  used: number;
  limit: number | null;
  window_s: number;
}

export interface BlockState {
  keyLocked: boolean;
  taggingDisabled: boolean;
  developer: DevKey | null;
  waste: WasteStanding;
  limits: Budget[];
}

type Change = (path: string, method: string, body?: unknown) => void;

export function masked(secret: string) {
  const value = el("span", { className: "mono" }, HIDDEN_SECRET);
  const eye = button("Show", () => {
    const hidden = eye.textContent === "Show";
    value.textContent = hidden ? secret : HIDDEN_SECRET;
    eye.textContent = hidden ? "Hide" : "Show";
  });
  return el("span", { className: "inline" }, value, eye);
}

export function permissionNames(bits: number) {
  return (
    PERMISSIONS.filter(([bit]) => bits & bit)
      .map(([, name]) => name)
      .join(", ") || "None"
  );
}

export function exhausted(limits: Budget[]) {
  return limits.filter(
    (budget) => budget.limit !== null && budget.used >= budget.limit,
  );
}

export function developerPanel(
  key: DevKey | null,
  change: Change,
  chart?: Node,
) {
  if (!key)
    return panel(
      "Developer Key",
      row(
        "None",
        "No developer key",
        button("Create", () => create(change)),
      ),
    );
  return panel(
    "Developer Key",
    row("Label", key.label),
    row("Key", masked(key.key)),
    row(
      "Permissions",
      permissionNames(key.permissions),
      button("Edit", () => editPermissions(key, change)),
    ),
    row(
      "Rate limit",
      `${FORMATS.number(key.rate_limit)} per 5 min`,
      button("Edit", () => editRateLimit(key, change)),
    ),
    row("Requests", FORMATS.number(key.request_count)),
    ...(chart ? [chart] : []),
    row("Created", day(key.created_at)),
    row(
      "State",
      key.locked ? "Locked" : "Active",
      button(key.locked ? "Unlock" : "Lock", () =>
        change("/dev-key/lock", "POST", { locked: !key.locked }),
      ),
      button(
        "Delete",
        () =>
          confirm(
            `Delete the developer key "${key.label}"? Apps using it stop working.`,
          ) && change("/dev-key", "DELETE"),
        "button small danger",
      ),
    ),
  );
}

export function blocksPanel(state: BlockState, change: Change) {
  const { waste } = state;
  const capped = exhausted(state.limits);
  const blocks = [
    state.keyLocked &&
      row(
        "API key",
        "Locked",
        button("Unlock", () => change("/unlock", "POST")),
      ),
    state.developer?.locked &&
      row(
        "Developer key",
        "Locked",
        button("Unlock", () =>
          change("/dev-key/lock", "POST", { locked: false }),
        ),
      ),
    waste.blocked_until &&
      row(
        "Hypixel waste",
        `Blocked until ${day(waste.blocked_until)}`,
        button(
          "Lift",
          () =>
            confirm("Lift the block for wasting Hypixel calls?") &&
            change("/waste-block", "DELETE"),
        ),
      ),
    state.taggingDisabled &&
      row(
        "Tagging",
        "Disabled",
        button("Enable", () => change("/tagging", "POST", { disabled: false })),
      ),
    capped.length > 0 &&
      row(
        "Rate limits",
        capped.map((budget) => titleCase(budget.limiter)).join(", "),
        button("Reset", () => change("/limits/reset", "POST")),
      ),
  ].filter((block): block is HTMLDivElement => Boolean(block));
  return panel(
    "Blocks",
    ...(blocks.length ? blocks : [row("None", "Nothing is blocked")]),
    row(
      "Waste allowance",
      `${FORMATS.number(waste.spent)} of ${FORMATS.number(waste.allowance)} spent`,
      el("span", {
        className: "dim",
        textContent: `refills ${FORMATS.number(waste.refill_per_day)} a day`,
      }),
    ),
  );
}

function button(label: string, run: () => unknown, className = "button small") {
  const element = el("button", {
    type: "button",
    className,
    textContent: label,
  });
  element.addEventListener("click", run);
  return element;
}

async function create(change: Change) {
  const form = await ask(
    "Create Developer Key",
    "Create",
    labelField(),
    rateLimitField(DEFAULT_RATE_LIMIT),
    permissionFields(0),
  );
  if (form)
    change("/dev-key", "POST", {
      label: String(form.get("label")).trim(),
      rate_limit: Number(form.get("rate_limit")),
      permissions: permissionBits(form),
    });
}

async function editPermissions(key: DevKey, change: Change) {
  const form = await ask(
    "Developer Key Permissions",
    "Save",
    permissionFields(key.permissions),
  );
  if (form)
    change("/dev-key/permissions", "POST", {
      permissions: permissionBits(form),
    });
}

async function editRateLimit(key: DevKey, change: Change) {
  const form = await ask(
    "Developer Key Rate Limit",
    "Save",
    rateLimitField(key.rate_limit),
  );
  if (form)
    change("/dev-key/rate-limit", "POST", {
      rate_limit: Number(form.get("rate_limit")),
    });
}

function labelField() {
  return el(
    "label",
    {},
    "Label",
    el("input", {
      className: "field",
      name: "label",
      maxLength: LABEL_MAX_CHARS,
      required: true,
      pattern: ".*\\S.*",
    }),
  );
}

function rateLimitField(value: number) {
  return el(
    "label",
    {},
    "Requests per 5 minutes",
    el("input", {
      className: "field",
      name: "rate_limit",
      type: "number",
      min: "1",
      max: String(MAX_RATE_LIMIT),
      value: String(value),
      required: true,
    }),
  );
}

function permissionFields(bits: number) {
  return el(
    "fieldset",
    { className: "checks" },
    el("legend", {}, "Permissions"),
    ...PERMISSIONS.map(([bit, name]) =>
      el(
        "label",
        { className: "check" },
        el("input", {
          type: "checkbox",
          name: "permission",
          value: String(bit),
          checked: Boolean(bits & bit),
        }),
        name,
      ),
    ),
  );
}

function permissionBits(form: FormData) {
  return form.getAll("permission").reduce((bits, bit) => bits | Number(bit), 0);
}

function ask(title: string, confirmLabel: string, ...fields: Node[]) {
  const form = el(
    "form",
    { method: "dialog", className: "dialog-form" },
    el("h2", {}, title),
    ...fields,
    el(
      "div",
      { className: "actions" },
      el(
        "button",
        { className: "button", value: "cancel", formNoValidate: true },
        "Cancel",
      ),
      el(
        "button",
        { className: "button primary", value: "confirm" },
        confirmLabel,
      ),
    ),
  );
  const dialog = el("dialog", {}, form);
  document.body.append(dialog);
  dialog.showModal();
  return new Promise<FormData | null>((resolve) =>
    dialog.addEventListener(
      "close",
      () => {
        resolve(dialog.returnValue === "confirm" ? new FormData(form) : null);
        dialog.remove();
      },
      { once: true },
    ),
  );
}
