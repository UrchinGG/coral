import { useEffect } from "react";
import { useLocation } from "react-router-dom";

import { focusOmnibox } from "../omnibox/omnibox-store";

export function NotFound() {
  const { pathname } = useLocation();

  useEffect(() => {
    focusOmnibox();
  }, [pathname]);

  return (
    <div className="flex flex-col gap-1">
      <h1 className="text-22 font-semibold">No page here</h1>
      <p className="m-0 max-w-[70ch] text-muted">
        Nothing is served at <span className="font-mono break-all text-fg">{pathname}</span>. Search for what you need in the box above.
      </p>
    </div>
  );
}
