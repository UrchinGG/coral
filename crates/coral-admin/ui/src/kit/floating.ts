import { useLayoutEffect, useState, type RefObject } from "react";

export type Placement = "below" | "above";

export type FloatingPosition = { top: number; left: number; placement: Placement };

const GAP_PX = 6;
const VIEWPORT_MARGIN_PX = 8;

export function useFloatingPosition(
  anchorRef: RefObject<HTMLElement | null>,
  floatingRef: RefObject<HTMLElement | null>,
  open: boolean,
  preferred: Placement,
): FloatingPosition | null {
  const [position, setPosition] = useState<FloatingPosition | null>(null);

  useLayoutEffect(() => {
    if (!open) return;
    const update = () => {
      const anchor = anchorRef.current;
      const floating = floatingRef.current;
      if (anchor === null || floating === null) return;
      setPosition(place(anchor.getBoundingClientRect(), floating.getBoundingClientRect(), preferred));
    };
    update();
    window.addEventListener("scroll", update, true);
    window.addEventListener("resize", update);
    return () => {
      window.removeEventListener("scroll", update, true);
      window.removeEventListener("resize", update);
    };
  }, [anchorRef, floatingRef, open, preferred]);

  return open ? position : null;
}

function place(anchor: DOMRect, floating: DOMRect, preferred: Placement): FloatingPosition {
  const fitsBelow = anchor.bottom + GAP_PX + floating.height <= window.innerHeight - VIEWPORT_MARGIN_PX;
  const fitsAbove = anchor.top - GAP_PX - floating.height >= VIEWPORT_MARGIN_PX;
  const placement: Placement = preferred === "below" ? (fitsBelow || !fitsAbove ? "below" : "above") : fitsAbove || !fitsBelow ? "above" : "below";
  const top = placement === "below" ? anchor.bottom + GAP_PX : anchor.top - GAP_PX - floating.height;
  const maxLeft = window.innerWidth - VIEWPORT_MARGIN_PX - floating.width;
  const left = Math.max(VIEWPORT_MARGIN_PX, Math.min(anchor.left, maxLeft));
  return { top, left, placement };
}
