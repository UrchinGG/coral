import { useState } from "react";
import { usePostGuide, useReviewGuide, useSaveGuideContent, useSetPingRoles } from "../api/reviewGuide";
import type { GuideContent, ReviewGuideResponse } from "../api/types";
import { Badge } from "../components/Badge";
import { ConfirmButton } from "../components/ConfirmButton";
import { Panel } from "../components/Panel";
import { PickerSelect, type PickerItem } from "../components/PickerSelect";
import { fmtDate, fmtNum } from "../format";

export function ReviewGuide() {
  const guide = useReviewGuide();

  if (guide.isLoading) {
    return <div className="text-sm text-gray-500">Loading…</div>;
  }
  if (!guide.data) {
    return <div className="text-sm text-gray-500">Review guide unavailable.</div>;
  }

  return <ReviewGuideLoaded data={guide.data} />;
}

function ReviewGuideLoaded({ data }: { data: ReviewGuideResponse }) {
  const [content, setContent] = useState<GuideContent>(data.content);
  const [tab, setTab] = useState<"edit" | "preview">("edit");
  const save = useSaveGuideContent();

  const dirty = content.body !== data.content.body;

  return (
    <div className="grid grid-cols-1 gap-5 xl:grid-cols-3">
      <Panel
        className="xl:col-span-2"
        title="Guide content"
        description="Edits are saved here first, then published to Discord from the Publish panel."
        action={
          <div className="flex items-center gap-2">
            <div className="flex overflow-hidden rounded-md border border-white/8">
              {(["edit", "preview"] as const).map((t) => (
                <button
                  key={t}
                  onClick={() => setTab(t)}
                  className={`px-2.5 py-1 text-xs font-medium ${t === tab ? "bg-accent/15 text-accent" : "text-gray-400 hover:bg-white/5"}`}
                >
                  {t === "edit" ? "Edit" : "Preview"}
                </button>
              ))}
            </div>
            <button
              disabled={!dirty || save.isPending}
              onClick={() => save.mutate(content)}
              className="rounded-md bg-accent/15 px-3 py-1 text-xs font-medium text-accent hover:bg-accent/25 disabled:opacity-40"
            >
              {save.isPending ? "Saving…" : "Save"}
            </button>
          </div>
        }
      >
        {tab === "edit" ? <ContentEditor content={content} onChange={setContent} /> : <GuidePreview content={content} />}
      </Panel>

      <div className="flex flex-col gap-5">
        <PublishPanel data={data} dirty={dirty} />
        <PingRolesPanel data={data} />
      </div>
    </div>
  );
}

function ContentEditor({ content, onChange }: { content: GuideContent; onChange: (c: GuideContent) => void }) {
  return (
    <div className="flex flex-col gap-2">
      <textarea
        rows={22}
        className="w-full resize-y rounded-md border border-white/10 bg-black/30 px-3 py-2 font-mono text-xs leading-5"
        value={content.body}
        onChange={(e) => onChange({ body: e.target.value })}
      />
      <div className="text-xs text-gray-500">
        Discord markdown: <code>## heading</code>, <code>**bold**</code>, <code>`code`</code>, <code>-# </code> for
        small text, and <code>---</code> on its own line for a divider.
      </div>
    </div>
  );
}

function GuidePreview({ content }: { content: GuideContent }) {
  return (
    <div className="rounded-md border border-white/8 bg-black/30 p-4">
      <MarkdownLite text={content.body} />
      <span className="mt-2 inline-block cursor-not-allowed rounded-md border border-white/10 bg-white/5 px-3 py-1.5 text-xs text-gray-400">
        Ping Me For Reviews
      </span>
    </div>
  );
}

function MarkdownLite({ text }: { text: string }) {
  return (
    <div className="flex flex-col gap-0.5">
      {text.split("\n").map((line, i) => {
        if (/^-{3,}$/.test(line.trim())) {
          return <div key={i} className="my-2 border-t border-white/10" />;
        }
        if (line.startsWith("-# ")) {
          return (
            <div key={i} className="text-xs text-gray-500">
              <InlineMarkdown text={line.slice(3)} />
            </div>
          );
        }
        return (
          <div key={i} className="text-sm text-gray-300">
            <InlineMarkdown text={line} />
          </div>
        );
      })}
    </div>
  );
}

function InlineMarkdown({ text }: { text: string }) {
  const parts = text.split(/(\*\*[^*]+\*\*|`[^`]+`)/g);
  return (
    <>
      {parts.map((part, i) => {
        if (part.startsWith("**") && part.endsWith("**")) {
          return (
            <strong key={i} className="font-semibold text-gray-200">
              {part.slice(2, -2)}
            </strong>
          );
        }
        if (part.startsWith("`") && part.endsWith("`") && part.length > 1) {
          return (
            <code key={i} className="rounded bg-black/40 px-1 font-mono text-[0.9em]">
              {part.slice(1, -1)}
            </code>
          );
        }
        return <span key={i}>{part}</span>;
      })}
    </>
  );
}

function PublishPanel({ data, dirty }: { data: ReviewGuideResponse; dirty: boolean }) {
  const post = usePostGuide();
  const { status } = data;

  return (
    <Panel title="Publish" description="Posts a pinned, locked thread in the review forum. If one is already posted, this edits it in place.">
      <div className="flex flex-col gap-2 text-sm">
        {status.posted ? (
          <>
            <div className="flex flex-wrap items-center gap-2">
              <Badge label="Posted" tone="ok" />
              <span className="text-gray-300">in #{status.forum_channel_name}</span>
            </div>
            <div className="text-xs text-gray-500">
              Posted {fmtDate(status.posted_at)}
              {status.posted_by_username ? ` by ${status.posted_by_username}` : ""}
            </div>
            {status.up_to_date ? (
              <Badge label="Up to date" tone="ok" />
            ) : (
              <div>
                <Badge label="Unpublished changes" tone="warning" />
                <div className="mt-1 text-xs text-gray-500">
                  The saved guide differs from the posted thread. Update to publish.
                </div>
              </div>
            )}
          </>
        ) : (
          <div className="text-gray-500">The guide has not been posted yet.</div>
        )}
        {dirty && (
          <div className="text-xs text-warning">You have unsaved edits — save first, posting publishes saved content.</div>
        )}
        <div className="mt-1">
          <ConfirmButton
            label={status.posted ? "Update posted guide" : "Post guide"}
            confirmLabel={status.posted ? "Edit posted thread" : "Post pinned thread"}
            onConfirm={() => post.mutate()}
            pending={post.isPending}
          />
        </div>
      </div>
    </Panel>
  );
}

function PingRolesPanel({ data }: { data: ReviewGuideResponse }) {
  const setPingRoles = useSetPingRoles();
  const { ping_roles } = data;

  const roleItems: PickerItem[] = data.home_roles.map((role) => ({
    id: role.id,
    label: role.name,
    color: role.color,
    disabled: !role.assignable,
    hint: role.managed ? "managed" : !role.assignable ? "not manageable" : undefined,
  }));

  return (
    <Panel
      title="Review ping roles"
      description="Members opt in and out themselves via the guide's button in Discord. Coral pings these roles from review threads."
    >
      <div className="flex flex-col gap-4">
        <PingRoleRow
          label="All reviews"
          description="Pinged for every new tag submission."
          optIns={ping_roles.review_opt_ins}
          items={roleItems}
          value={ping_roles.review_role_id}
          onChange={(id) => setPingRoles.mutate({ review_role_id: id, dispute_role_id: ping_roles.dispute_role_id })}
        />
        <PingRoleRow
          label="Disputes only"
          description="Pinged when votes disagree and a moderator call is needed."
          optIns={ping_roles.dispute_opt_ins}
          items={roleItems}
          value={ping_roles.dispute_role_id}
          onChange={(id) => setPingRoles.mutate({ review_role_id: ping_roles.review_role_id, dispute_role_id: id })}
        />
      </div>
    </Panel>
  );
}

function PingRoleRow({
  label,
  description,
  optIns,
  items,
  value,
  onChange,
}: {
  label: string;
  description: string;
  optIns: number;
  items: PickerItem[];
  value: string | null;
  onChange: (id: string | null) => void;
}) {
  return (
    <div>
      <div className="flex items-center gap-2">
        <span className="text-sm font-medium text-gray-200">{label}</span>
        <span className="text-xs text-gray-500">{fmtNum(optIns)} opted in</span>
      </div>
      <div className="mt-0.5 mb-1.5 text-xs text-gray-500">{description}</div>
      <PickerSelect items={items} value={value} onChange={onChange} placeholder="Select a role" prefix="@" />
    </div>
  );
}
