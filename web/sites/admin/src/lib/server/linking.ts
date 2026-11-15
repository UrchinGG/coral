import { el } from "@/lib/dom";
import {
  apply,
  channelSelect,
  membersPhrase,
  roleSelect,
  smallButton,
  type Server,
} from "@/lib/server/common";

export function linkingTab(server: Server) {
  const config = server.config.config;
  const swap = `Swap the role across ${membersPhrase(server)}?`;
  return el(
    "section",
    { className: "panel wide" },
    el("h2", { className: "panel-title" }, "Account Linking"),
    setting(
      "Linked role",
      "Given to members once their Minecraft account is linked.",
      roleSelect(server, config.link_role_id),
      (id) =>
        confirm(swap) &&
        apply(
          server,
          "/link-role",
          "PUT",
          { role_id: id },
          "Linked role saved",
        ),
    ),
    setting(
      "Unlinked role",
      "Held by members who haven't linked an account yet.",
      roleSelect(server, config.unlinked_role_id),
      (id) =>
        confirm(swap) &&
        apply(
          server,
          "/unlinked-role",
          "PUT",
          { role_id: id },
          "Unlinked role saved",
        ),
    ),
    setting(
      "Link channel",
      "Coral keeps a “Link your Minecraft account” prompt here. Changing it deletes the old prompt and posts a new one.",
      channelSelect(server, config.link_channel_id),
      (id) =>
        confirm("Move the link prompt to this channel?") &&
        apply(
          server,
          "/link-channel",
          "PUT",
          { channel_id: id },
          "Link channel saved",
        ),
    ),
  );
}

export function setting(
  label: string,
  description: string,
  control: HTMLSelectElement,
  save: (value: string | null) => unknown,
) {
  const saved = control.value;
  const button = smallButton(
    "Save",
    () => save(control.value || null),
    "button primary",
  );
  const reset = smallButton("Reset", () => {
    control.value = saved;
    sync();
  });
  const sync = () => {
    button.hidden = reset.hidden = control.value === saved;
  };
  control.addEventListener("change", sync);
  sync();
  return el(
    "div",
    { className: "setting" },
    el(
      "div",
      {},
      el("b", { className: "strong" }, label),
      el("p", { className: "muted" }, description),
    ),
    el("div", { className: "inline" }, control, button, reset),
  );
}
