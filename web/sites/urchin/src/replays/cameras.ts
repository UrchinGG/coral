import type { Camera } from "../viewer/pkg/replay_viewer.js";

export const CAMERAS: { id: Camera; name: string; tip: string }[] = [
  {
    id: "map",
    name: "Map",
    tip: "Whole map: drag to turn, right-drag to move, scroll to zoom",
  },
  { id: "follow", name: "Follow", tip: "Follow a player from behind" },
  { id: "pov", name: "POV", tip: "See through the player's eyes" },
  { id: "fly", name: "Fly", tip: "Fly freely with mouse and keyboard" },
];
