import { useSyncExternalStore, type ReactNode } from "react";

export type ToastTone = "ok" | "error" | "pending";

export type ToastAction = { label: string; run: () => void };

export type ToastItem = {
  id: number;
  tone: ToastTone;
  message: ReactNode;
  action: ToastAction | null;
};

export type ToastHandle = {
  settle: (message: ReactNode, tone?: Exclude<ToastTone, "pending">) => void;
  dismiss: () => void;
};

type ToastOptions = { action?: ToastAction };

const TOAST_MS = 4_600;

const listeners = new Set<() => void>();
let items: readonly ToastItem[] = [];
let nextId = 1;

export const toast = {
  ok: (message: ReactNode, options?: ToastOptions) => show("ok", message, options),
  error: (message: ReactNode, options?: ToastOptions) => show("error", message, options),
  pending: (message: ReactNode) => show("pending", message),
};

export function useToasts(): readonly ToastItem[] {
  return useSyncExternalStore(subscribe, () => items);
}

export function dismissToast(id: number): void {
  items = items.filter((item) => item.id !== id);
  notify();
}

function show(tone: ToastTone, message: ReactNode, options: ToastOptions = {}): ToastHandle {
  const id = nextId++;
  items = [...items, { id, tone, message, action: options.action ?? null }];
  notify();
  if (tone !== "pending") scheduleDismiss(id);
  return {
    settle: (settled, settledTone = "ok") => {
      items = items.map((item) => (item.id === id ? { ...item, tone: settledTone, message: settled } : item));
      notify();
      scheduleDismiss(id);
    },
    dismiss: () => dismissToast(id),
  };
}

function scheduleDismiss(id: number): void {
  window.setTimeout(() => dismissToast(id), TOAST_MS);
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function notify(): void {
  listeners.forEach((listener) => listener());
}
