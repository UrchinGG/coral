import { useInfiniteQuery } from "@tanstack/react-query";
import { useId, useState, type FormEvent } from "react";

import { auditQuery } from "../../api/moderation";
import type { AuditRow } from "../../api/types/platform";
import { Button } from "../../kit/Button";
import { ClampText } from "../../kit/ClampText";
import { EntityLink } from "../../kit/EntityLink";
import { Input } from "../../kit/Input";
import type { Column } from "../../kit/Table";
import { useUrlParam } from "../../lib/url-state";
import { PagedTable } from "../platform/PagedTable";
import { When } from "../platform/When";

const KNOWN_ACTIONS = [
  "add_autorole_rule",
  "add_strike",
  "add_tag",
  "cancel_sync_job",
  "create_dev_key",
  "delete_dev_key",
  "delete_plugin",
  "delete_plugin_review",
  "delete_release",
  "extension",
  "lock_member",
  "lock_player",
  "publish_review_guide",
  "recompute_rollups",
  "regenerate_api_key",
  "remove_autorole_rule",
  "remove_tag",
  "reset_nicknames",
  "reset_rate_limit",
  "revoke_starfish_sessions",
  "revoke_strike",
  "set_access_level",
  "set_dev_key_locked",
  "set_dev_key_permissions",
  "set_dev_key_rate_limit",
  "set_license_status",
  "set_link_channel",
  "set_link_role",
  "set_nickname_template",
  "set_plugin_disabled",
  "set_plugin_official",
  "set_plugin_unlisted",
  "set_review_ping_roles",
  "set_tagging_disabled",
  "set_unlinked_role",
  "start_maintenance_job",
  "strip_role",
  "unlock_member",
  "unlock_player",
  "unyank_release",
  "update_autorole_rule",
  "update_review_guide_content",
  "update_review_rules",
  "yank_release",
] as const;

const COLUMNS: readonly Column<AuditRow>[] = [
  { id: "ts", header: "When", cell: (row) => <When at={row.ts} /> },
  { id: "actor", header: "Actor", cell: (row) => <EntityLink entity={row.actor} /> },
  { id: "action", header: "Action", cell: (row) => <span className="font-mono text-12">{row.action}</span> },
  { id: "target", header: "Target", cell: (row) => (row.target_ref === null ? <span className="font-mono text-12 wrap-anywhere">{row.target}</span> : <EntityLink entity={row.target_ref} />) },
  { id: "details", header: "Details", cell: (row) => <ClampText className="font-mono text-12 text-muted">{JSON.stringify(row.details)}</ClampText>, className: "min-w-[240px]", priority: 2 },
];

export function AuditTab() {
  const [action, setAction] = useUrlParam("action");
  const listId = useId();
  const [draft, setDraft] = useState(action ?? "");
  const actions = useInfiniteQuery(auditQuery(action ?? ""));
  const submit = (event: FormEvent) => {
    event.preventDefault();
    setAction(draft.trim());
  };
  return (
    <>
      <form onSubmit={submit} className="flex flex-wrap items-end gap-2">
        <Input aria-label="Action" placeholder="Every action" list={listId} value={draft} onChange={(event) => setDraft(event.target.value)} mono className="w-72 max-w-full" />
        <datalist id={listId}>
          {KNOWN_ACTIONS.map((known) => (
            <option key={known} value={known} />
          ))}
        </datalist>
        <Button type="submit">Filter</Button>
        {action !== null && (
          <Button
            variant="ghost"
            onClick={() => {
              setDraft("");
              setAction(null);
            }}
          >
            Show every action
          </Button>
        )}
      </form>
      <PagedTable title="Audit log" label="Admin actions" query={actions} columns={COLUMNS} rowKey={(row) => String(row.id)} empty={action === null ? "No admin action has been recorded." : `No ${action} action has been recorded.`} />
    </>
  );
}
