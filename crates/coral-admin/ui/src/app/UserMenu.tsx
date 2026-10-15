import { Keyboard, LogOut } from "lucide-react";

import { useLabel } from "../api/labels";
import { EntityLink } from "../kit/EntityLink";
import { Popover } from "../kit/Popover";
import { useBoot } from "./boot";

const LOGOUT_PATH = "/auth/logout";

export function UserMenu({ onShowShortcuts }: { onShowShortcuts: () => void }) {
  const boot = useBoot();
  const name = useLabel({ space: "discord", id: boot.discord_id });
  const initial = (name ?? "?").slice(0, 1).toUpperCase();
  return (
    <Popover
      label="Account menu"
      buttonClassName="inline-flex size-7 shrink-0 items-center justify-center rounded-full bg-[#2a2f55] text-12 font-semibold text-[#c7cbff]"
      panelClassName="w-64 p-1.5"
      button={initial}
    >
      {(close) => (
        <div className="flex flex-col">
          <div className="flex flex-col gap-1 border-b border-border px-2.5 pt-1.5 pb-2.5 text-12">
            <span className="text-11 text-subtle">Signed in as</span>
            <span onClick={close}>
              <EntityLink entity={{ kind: "account", discord_id: boot.discord_id, member_id: null }} />
            </span>
            <span className="text-11 text-subtle">
              build <span className="font-mono text-muted">{boot.build ?? "unset"}</span>
            </span>
          </div>
          <button
            type="button"
            onClick={() => {
              close();
              onShowShortcuts();
            }}
            className="mt-1 flex h-8 items-center gap-2 rounded-md px-2.5 text-left text-12 text-muted hover:bg-surface-2 hover:text-fg"
          >
            <Keyboard className="size-3.5" />
            Keyboard shortcuts
            <span className="ml-auto font-mono text-11 text-subtle">?</span>
          </button>
          <form method="post" action={LOGOUT_PATH}>
            <button type="submit" className="flex h-8 w-full items-center gap-2 rounded-md px-2.5 text-left text-12 text-muted hover:bg-surface-2 hover:text-fg">
              <LogOut className="size-3.5" />
              Log out
            </button>
          </form>
        </div>
      )}
    </Popover>
  );
}
