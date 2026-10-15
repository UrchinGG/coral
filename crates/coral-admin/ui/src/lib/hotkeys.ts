import { useEffect, useEffectEvent } from "react";

export type HotkeyOptions = {
  enabled?: boolean;
  allowInInputs?: boolean;
  allowInModal?: boolean;
};

type Binding = {
  combo: string;
  run: (event: KeyboardEvent) => void;
  allowInInputs: boolean;
  allowInModal: boolean;
};

type PendingPrefix = { combo: string; at: number };

const SEQUENCE_TIMEOUT_MS = 900;
const SEQUENCE_SEPARATOR = " ";
const EDITABLE_SELECTOR = [
  "textarea",
  "select",
  "[contenteditable]:not([contenteditable='false'])",
  "input:not([type=checkbox]):not([type=radio]):not([type=button]):not([type=submit]):not([type=reset])",
].join(", ");
const MODAL_SELECTOR = "dialog:modal";
const ACTIVATION_KEYS = new Set(["Enter", " "]);
const ACTIVATABLE_SELECTOR = "a[href], button, summary, [role=button], [role=tab], [role=option], [role=menuitem]";

const bindings: Binding[] = [];
let pendingPrefix: PendingPrefix | null = null;
let listening = false;

export function useHotkey(combo: string, handler: (event: KeyboardEvent) => void, options: HotkeyOptions = {}): void {
  const run = useEffectEvent(handler);
  const { enabled = true, allowInInputs = false, allowInModal = false } = options;

  useEffect(() => {
    if (!enabled) return;
    return bindHotkey(combo, (event) => run(event), { allowInInputs, allowInModal });
  }, [combo, enabled, allowInInputs, allowInModal]);
}

export function bindHotkey(combo: string, run: (event: KeyboardEvent) => void, options: Omit<HotkeyOptions, "enabled"> = {}): () => void {
  const { allowInInputs = false, allowInModal = false } = options;
  return registerBinding({ combo, run, allowInInputs, allowInModal });
}

export function isModalOpen(): boolean {
  return document.querySelector(MODAL_SELECTOR) !== null;
}

function registerBinding(binding: Binding): () => void {
  bindings.push(binding);
  ensureListening();
  return () => {
    const index = bindings.indexOf(binding);
    if (index >= 0) bindings.splice(index, 1);
  };
}

function ensureListening(): void {
  if (listening) return;
  document.addEventListener("keydown", dispatch);
  listening = true;
}

function dispatch(event: KeyboardEvent): void {
  if (event.defaultPrevented || event.isComposing) return;
  if (ACTIVATION_KEYS.has(event.key) && isActivatable(event.target)) return;
  const typing = isEditable(event.target);
  const modal = isModalOpen();
  const key = comboOf(event);
  const combo = consumePrefix(key) ?? key;
  const binding = bindings.findLast(
    (candidate) => candidate.combo === combo && (!typing || candidate.allowInInputs) && (!modal || candidate.allowInModal),
  );
  if (binding) {
    event.preventDefault();
    binding.run(event);
    return;
  }
  if (!typing && !modal && startsSequence(key)) pendingPrefix = { combo: key, at: performance.now() };
}

function consumePrefix(key: string): string | null {
  const prefix = pendingPrefix;
  pendingPrefix = null;
  if (prefix === null || performance.now() - prefix.at > SEQUENCE_TIMEOUT_MS) return null;
  return `${prefix.combo}${SEQUENCE_SEPARATOR}${key}`;
}

function startsSequence(key: string): boolean {
  return bindings.some((binding) => binding.combo.startsWith(`${key}${SEQUENCE_SEPARATOR}`));
}

function comboOf(event: KeyboardEvent): string {
  const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
  const mod = (event.metaKey || event.ctrlKey) && !event.altKey;
  return mod ? `mod+${key}` : key;
}

function isActivatable(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest(ACTIVATABLE_SELECTOR) !== null;
}

function isEditable(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest(EDITABLE_SELECTOR) !== null;
}
