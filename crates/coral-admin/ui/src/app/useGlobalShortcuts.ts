import { useNavigate } from "react-router-dom";

import { useHotkey } from "../lib/hotkeys";
import { openPalette, togglePalette } from "../omnibox/omnibox-store";
import { GO_SHORTCUTS } from "./nav";

export function useGlobalShortcuts(showShortcuts: () => void): void {
  const navigate = useNavigate();
  useHotkey("mod+k", togglePalette, { allowInInputs: true, allowInModal: true });
  useHotkey("/", openPalette);
  useHotkey("?", showShortcuts);
  useHotkey("g h", () => navigate(GO_SHORTCUTS.h));
  useHotkey("g r", () => navigate(GO_SHORTCUTS.r));
  useHotkey("g t", () => navigate(GO_SHORTCUTS.t));
  useHotkey("g a", () => navigate(GO_SHORTCUTS.a));
  useHotkey("g p", () => navigate(GO_SHORTCUTS.p));
  useHotkey("g m", () => navigate(GO_SHORTCUTS.m));
}
