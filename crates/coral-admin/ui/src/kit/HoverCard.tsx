import { useEffect, useId, useRef, useState, type ReactNode } from "react";

import { HOVER_QUERY, useMediaQuery } from "../lib/media";
import { useFloatingPosition, type Placement } from "./floating";
import { TopLayer } from "./TopLayer";

type HoverCardProps = {
  content: () => ReactNode;
  children: ReactNode;
  delayMs?: number;
  onOpen?: () => void;
};

type TooltipProps = {
  text: ReactNode;
  children: ReactNode;
  placement?: Placement;
};

const HOVER_CARD_DELAY_MS = 350;
const TOOLTIP_DELAY_MS = 400;
const CLOSE_GRACE_MS = 120;

export function HoverCard({ content, children, delayMs = HOVER_CARD_DELAY_MS, onOpen }: HoverCardProps) {
  const canHover = useMediaQuery(HOVER_QUERY);
  const { open, anchorRef, floatingRef, position, show, hide } = useDelayedFloating(delayMs, "below", onOpen);

  return (
    <span ref={anchorRef} className="inline-flex max-w-full" onPointerEnter={canHover ? show : undefined} onPointerLeave={canHover ? hide : undefined}>
      {children}
      {open && (
        <TopLayer
          layerRef={floatingRef}
          onPointerEnter={show}
          onPointerLeave={hide}
          style={{ top: position?.top ?? 0, left: position?.left ?? 0, visibility: position === null ? "hidden" : "visible" }}
          className="fixed right-auto bottom-auto m-0 w-max max-w-[min(360px,calc(100vw-16px))] overflow-visible rounded-lg border border-border-strong bg-overlay p-3 text-12 text-fg shadow-[0_16px_40px_rgba(0,0,0,0.5)]"
        >
          {content()}
        </TopLayer>
      )}
    </span>
  );
}

export function Tooltip({ text, children, placement = "above" }: TooltipProps) {
  const tooltipId = useId();
  const { open, anchorRef, floatingRef, position, show, hide } = useDelayedFloating(TOOLTIP_DELAY_MS, placement);
  return (
    <span
      ref={anchorRef}
      aria-describedby={open ? tooltipId : undefined}
      className="inline-flex"
      onPointerEnter={show}
      onPointerLeave={hide}
      onFocus={show}
      onBlur={hide}
    >
      {children}
      {open && (
        <TopLayer
          layerRef={floatingRef}
          id={tooltipId}
          role="tooltip"
          style={{ top: position?.top ?? 0, left: position?.left ?? 0, visibility: position === null ? "hidden" : "visible" }}
          className="pointer-events-none fixed right-auto bottom-auto m-0 max-w-[280px] overflow-visible rounded-md border border-border-strong bg-overlay px-2 py-1 text-11 text-fg shadow-lg"
        >
          {text}
        </TopLayer>
      )}
    </span>
  );
}

function useDelayedFloating(delayMs: number, placement: Placement, onOpen?: () => void) {
  const [open, setOpen] = useState(false);
  const anchorRef = useRef<HTMLSpanElement>(null);
  const floatingRef = useRef<HTMLDivElement>(null);
  const timer = useRef<number | null>(null);
  const position = useFloatingPosition(anchorRef, floatingRef, open, placement);

  const clear = () => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = null;
  };
  const show = () => {
    clear();
    timer.current = window.setTimeout(
      () => {
        onOpen?.();
        setOpen(true);
      },
      open ? 0 : delayMs,
    );
  };
  const hide = () => {
    clear();
    timer.current = window.setTimeout(() => setOpen(false), CLOSE_GRACE_MS);
  };

  useEffect(() => clear, []);

  return { open, anchorRef, floatingRef, position, show, hide };
}
