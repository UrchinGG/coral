import { $ } from "./dom";

export const bindLibrary = (open: (input: File | string) => void) => {
  const stage = $("#stage");
  const drawer = $("#library");
  const toggle = $("#library-toggle");
  const picker = $<HTMLInputElement>("#file-input");
  const drop = $("#drop");

  const setOpen = (open: boolean) => {
    if (!open && stage.hasAttribute("data-empty")) return;
    if (!open && drawer.contains(document.activeElement)) toggle.focus();
    drawer.inert = !open;
    toggle.ariaExpanded = String(open);
  };

  const flip = () => {
    setOpen(drawer.inert);
    if (!drawer.inert) $("#file-button").focus();
  };

  toggle.addEventListener("click", flip);
  $("#file-button").addEventListener("click", () => picker.click());
  picker.addEventListener("change", () => {
    const file = picker.files?.[0];
    if (file) open(file);
    picker.value = "";
  });
  $("#url-form").addEventListener("submit", (event) => {
    event.preventDefault();
    open($<HTMLInputElement>("#url-input").value);
  });

  addEventListener("dragover", (event) => {
    if (!event.dataTransfer?.types.includes("Files")) return;
    event.preventDefault();
    drop.hidden = false;
  });
  addEventListener("dragleave", (event) => {
    if (!event.relatedTarget) drop.hidden = true;
  });
  addEventListener("drop", (event) => {
    event.preventDefault();
    drop.hidden = true;
    const file = event.dataTransfer?.files[0];
    if (file) open(file);
  });

  return { setOpen };
};
