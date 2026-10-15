import { useEffect, useRef, useSyncExternalStore, type RefObject } from "react";

const listeners = new Set<() => void>();
let openDialogs: readonly HTMLDialogElement[] = [];

export function useModalDialog(open: boolean): RefObject<HTMLDialogElement | null> {
  const ref = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = ref.current;
    if (dialog === null || !open) return;
    if (!dialog.open) dialog.showModal();
    enterStack(dialog);
    return () => {
      leaveStack(dialog);
      if (dialog.open) dialog.close();
    };
  }, [open]);

  return ref;
}

export function useTopLayerHost(): HTMLElement {
  return useSyncExternalStore(subscribe, topmostHost);
}

function topmostHost(): HTMLElement {
  return openDialogs.at(-1) ?? document.body;
}

function enterStack(dialog: HTMLDialogElement): void {
  openDialogs = [...openDialogs, dialog];
  notify();
}

function leaveStack(dialog: HTMLDialogElement): void {
  openDialogs = openDialogs.filter((open) => open !== dialog);
  notify();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function notify(): void {
  listeners.forEach((listener) => listener());
}
