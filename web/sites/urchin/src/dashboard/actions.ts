export type Actions = Record<string, (button: HTMLElement) => void>;

export function bindActions(root: HTMLElement, actions: Actions) {
  root.addEventListener("click", (event) => {
    const button = (event.target as HTMLElement).closest<HTMLElement>(
      "[data-action]",
    );
    if (!button) return;
    if (button.dataset.confirm) {
      button.textContent = button.dataset.confirm;
      button.dataset.sure = "";
      delete button.dataset.confirm;
      return;
    }
    actions[button.dataset.action!]?.(button);
  });
}

export const copy = (button: HTMLElement) =>
  navigator.clipboard.writeText(button.dataset.copy!).then(
    () => {
      button.dataset.copied = "";
      setTimeout(() => delete button.dataset.copied, 1500);
    },
    () => {},
  );

export const closeDialog = (button: HTMLElement) =>
  button.closest("dialog")!.close();
