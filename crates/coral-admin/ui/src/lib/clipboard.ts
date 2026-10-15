export type CopyOutcome = "copied" | "manual";

export async function copyText(text: string): Promise<CopyOutcome> {
  if (await writeWithClipboardApi(text)) return "copied";
  return copyWithSelection(text) ? "copied" : "manual";
}

async function writeWithClipboardApi(text: string): Promise<boolean> {
  if (!window.isSecureContext || navigator.clipboard === undefined) return false;
  return navigator.clipboard.writeText(text).then(
    () => true,
    () => false,
  );
}

function copyWithSelection(text: string): boolean {
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.append(area);
  area.select();
  const copied = document.execCommand("copy");
  area.remove();
  return copied;
}
