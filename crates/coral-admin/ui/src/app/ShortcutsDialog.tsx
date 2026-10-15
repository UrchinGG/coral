import { Kbd } from "../kit/Badges";
import { Button } from "../kit/Button";
import { Dialog } from "../kit/Dialog";

type Shortcut = { keys: readonly string[]; action: string };
type ShortcutGroup = { title: string; shortcuts: readonly Shortcut[] };

const GROUPS: readonly ShortcutGroup[] = [
  {
    title: "Anywhere",
    shortcuts: [
      { keys: ["⌘K", "Ctrl K", "/"], action: "Search everything" },
      { keys: ["g h"], action: "Home" },
      { keys: ["g r"], action: "Requests" },
      { keys: ["g t"], action: "Traffic" },
      { keys: ["g a"], action: "Accounts" },
      { keys: ["g p"], action: "Players" },
      { keys: ["g m"], action: "Moderation" },
      { keys: ["?"], action: "This list" },
    ],
  },
  {
    title: "Lists",
    shortcuts: [
      { keys: ["j", "k"], action: "Next or previous row" },
      { keys: ["Enter"], action: "Preview the selected row" },
      { keys: ["o"], action: "Open the selected row" },
      { keys: ["Esc"], action: "Close the preview" },
    ],
  },
  {
    title: "Detail pages",
    shortcuts: [
      { keys: ["[", "]"], action: "Previous or next item of the list you came from" },
      { keys: ["c"], action: "Copy the main identifier" },
    ],
  },
];

export function ShortcutsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Keyboard shortcuts"
      width="lg"
      actions={
        <Button variant="primary" onClick={onClose}>
          Done
        </Button>
      }
    >
      <div className="grid gap-4 sm:grid-cols-2">
        {GROUPS.map((group) => (
          <section key={group.title} className="flex flex-col gap-1.5">
            <h3 className="text-11 font-semibold tracking-[0.08em] text-subtle uppercase">{group.title}</h3>
            <dl className="m-0 grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-3 gap-y-1.5 text-12">
              {group.shortcuts.map((shortcut) => (
                <div key={shortcut.action} className="contents">
                  <dt className="flex flex-wrap gap-1">
                    {shortcut.keys.map((key) => (
                      <Kbd key={key}>{key}</Kbd>
                    ))}
                  </dt>
                  <dd className="m-0 text-fg">{shortcut.action}</dd>
                </div>
              ))}
            </dl>
          </section>
        ))}
      </div>
    </Dialog>
  );
}
