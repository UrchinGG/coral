import { $ } from "./dom";
import { stateOf, type Match } from "./match";
import { nickTag } from "./nicks";
import { showFace } from "./skins";

const STATE_NAMES = { alive: "", dead: "Dead", out: "Out" };

export const bindPill = (
  step: (direction: 1 | -1) => void,
  auto: { on: () => boolean; toggle: () => void },
) => {
  const pill = $("#pill");
  const face = $("#pill-face");
  const team = $("#pill-team");
  const name = $("#pill-name");
  const nick = $("#pill-nick");
  const state = $("#pill-state");
  const who = $("#pill-who");
  const showAuto = () => (who.ariaPressed = String(auto.on()));
  $("#pill-prev").addEventListener("click", () => step(-1));
  $("#pill-next").addEventListener("click", () => step(1));
  who.addEventListener("click", () => {
    auto.toggle();
    showAuto();
  });

  return (match: Match) =>
    (time: number, presence: Uint8Array, watched?: number) => {
      const player = watched === undefined ? undefined : match.players[watched];
      pill.hidden = !player;
      if (!player) return;
      showAuto();
      const now = stateOf(match, player, presence, time);
      pill.dataset.state = now;
      pill.style.setProperty("--color", player.team.color);
      showFace(face, player.skinHash);
      team.textContent = player.team.known ? player.team.name[0] : "";
      name.textContent = player.username;
      nick.textContent = nickTag(player);
      state.textContent = STATE_NAMES[now];
    };
};
