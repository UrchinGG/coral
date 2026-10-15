import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

type HotkeysModule = typeof import("./hotkeys");
type ElementKind = "plain" | "input" | "button";
type FakeKeyboardEvent = {
  key: string;
  target: FakeElement;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  isComposing: boolean;
  defaultPrevented: boolean;
  preventDefault: () => void;
};

const SEQUENCE_TIMEOUT_MS = 900;
const EDITABLE_MARKER = "textarea";
const ACTIVATABLE_MARKER = "a[href]";

class FakeElement {
  readonly kind: ElementKind;

  constructor(kind: ElementKind) {
    this.kind = kind;
  }

  closest(selector: string): FakeElement | null {
    if (this.kind === "input" && selector.includes(EDITABLE_MARKER)) return this;
    if (this.kind === "button" && selector.includes(ACTIVATABLE_MARKER)) return this;
    return null;
  }
}

let hotkeys: HotkeysModule;
let keydown: (event: KeyboardEvent) => void;
let modalOpen: boolean;
let now: number;

function press(key: string, init: Partial<FakeKeyboardEvent> = {}): FakeKeyboardEvent {
  const event: FakeKeyboardEvent = {
    key,
    target: new FakeElement("plain"),
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    isComposing: false,
    defaultPrevented: false,
    preventDefault() {
      this.defaultPrevented = true;
    },
    ...init,
  };
  keydown(event as unknown as KeyboardEvent);
  return event;
}

beforeEach(async () => {
  modalOpen = false;
  now = 0;
  vi.spyOn(performance, "now").mockImplementation(() => now);
  vi.stubGlobal("Element", FakeElement);
  vi.stubGlobal("document", {
    addEventListener: (_type: string, listener: (event: KeyboardEvent) => void) => {
      keydown = listener;
    },
    querySelector: () => (modalOpen ? {} : null),
  });
  vi.resetModules();
  hotkeys = await import("./hotkeys");
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("hotkeys", () => {
  it("runs a single-key binding and prevents the browser default", () => {
    const run = vi.fn();
    hotkeys.bindHotkey("?", run);
    const event = press("?");
    expect(run).toHaveBeenCalledTimes(1);
    expect(event.defaultPrevented).toBe(true);
  });

  it("runs a two-key sequence only when the second key follows in time", () => {
    const run = vi.fn();
    hotkeys.bindHotkey("g r", run);
    press("g");
    now = SEQUENCE_TIMEOUT_MS - 1;
    press("r");
    expect(run).toHaveBeenCalledTimes(1);

    now = 10_000;
    press("g");
    now += SEQUENCE_TIMEOUT_MS + 1;
    press("r");
    expect(run).toHaveBeenCalledTimes(1);
  });

  it("ignores bindings while typing unless they allow inputs", () => {
    const blocked = vi.fn();
    const allowed = vi.fn();
    hotkeys.bindHotkey("j", blocked);
    hotkeys.bindHotkey("Escape", allowed, { allowInInputs: true });
    press("j", { target: new FakeElement("input") });
    press("Escape", { target: new FakeElement("input") });
    expect(blocked).not.toHaveBeenCalled();
    expect(allowed).toHaveBeenCalledTimes(1);
  });

  it("ignores bindings while a modal dialog is open unless they allow it", () => {
    const blocked = vi.fn();
    const allowed = vi.fn();
    hotkeys.bindHotkey("c", blocked);
    hotkeys.bindHotkey("mod+k", allowed, { allowInModal: true });
    modalOpen = true;
    press("c");
    press("k", { ctrlKey: true });
    expect(blocked).not.toHaveBeenCalled();
    expect(allowed).toHaveBeenCalledTimes(1);
  });

  it("leaves Enter on a focused button to the button", () => {
    const run = vi.fn();
    hotkeys.bindHotkey("Enter", run);
    const event = press("Enter", { target: new FakeElement("button") });
    expect(run).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });

  it("treats Ctrl and Cmd alike and leaves AltGr combinations alone", () => {
    const run = vi.fn();
    hotkeys.bindHotkey("mod+k", run);
    press("K", { metaKey: true });
    press("k", { ctrlKey: true });
    press("k", { ctrlKey: true, altKey: true });
    expect(run).toHaveBeenCalledTimes(2);
  });

  it("lets the latest binding win and restores the earlier one when it unbinds", () => {
    const page = vi.fn();
    const dialog = vi.fn();
    hotkeys.bindHotkey("Escape", page);
    const unbind = hotkeys.bindHotkey("Escape", dialog);
    press("Escape");
    unbind();
    press("Escape");
    expect(dialog).toHaveBeenCalledTimes(1);
    expect(page).toHaveBeenCalledTimes(1);
  });

  it("skips events another handler already took", () => {
    const run = vi.fn();
    hotkeys.bindHotkey("j", run);
    press("j", { defaultPrevented: true });
    expect(run).not.toHaveBeenCalled();
  });
});
