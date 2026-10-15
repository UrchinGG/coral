import { useLayoutEffect, useRef, type ComponentProps, type RefObject } from "react";
import { createPortal } from "react-dom";

import { useTopLayerHost } from "./modal-stack";

type TopLayerProps = Omit<ComponentProps<"div">, "ref" | "popover"> & { layerRef?: RefObject<HTMLDivElement | null> };

const POPOVER_OPEN = ":popover-open";

export function TopLayer({ layerRef, ...props }: TopLayerProps) {
  const host = useTopLayerHost();
  const ownRef = useRef<HTMLDivElement>(null);
  const ref = layerRef ?? ownRef;

  useLayoutEffect(() => {
    const layer = ref.current;
    if (layer !== null && !layer.matches(POPOVER_OPEN)) layer.showPopover();
  }, [ref, host]);

  return createPortal(<div {...props} ref={ref} popover="manual" />, host);
}
