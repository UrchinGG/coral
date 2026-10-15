import { useQuery } from "@tanstack/react-query";
import { useState } from "react";

import { keys } from "../../api/keys";
import { guideQuery, saveGuide } from "../../api/moderation";
import { useAction } from "../../api/mutate";
import type { Guide, GuideContent } from "../../api/types/platform";
import { Button } from "../../kit/Button";
import { Textarea } from "../../kit/Input";
import { QueryContent } from "../../kit/QueryContent";
import { Section } from "../../kit/Section";
import { Segmented } from "../../kit/Segmented";
import { useUrlEnum } from "../../lib/url-state";
import { discardDraft, draftOver, keepDraft } from "./guide-draft";
import { GuidePreview } from "./GuidePreview";
import { PingRolesPanel, PublishPanel } from "./GuidePublish";
import { ReviewRulesPanel } from "./ReviewRules";

const EDITOR_VIEWS = ["edit", "preview"] as const;
const GUIDE_BUTTON = "Ping Me For Reviews";
const EDITOR_ROWS = 22;
const VIEW_OPTIONS = [
  { value: "edit", label: "Edit" },
  { value: "preview", label: "Preview" },
] as const;

type GuideEditorProps = { guide: Guide; content: GuideContent; onEdit: (body: string) => void };

export function GuideTab() {
  const guide = useQuery(guideQuery());
  return (
    <div className="grid grid-cols-1 gap-4 xl:grid-cols-3">
      <QueryContent query={guide}>{(data) => <GuideWorkspace key={data.content.body} guide={data} />}</QueryContent>
    </div>
  );
}

function GuideWorkspace({ guide }: { guide: Guide }) {
  const [content, setContent] = useState<GuideContent>(() => draftOver(guide.content) ?? guide.content);
  const edit = (body: string) => {
    const next = { body };
    setContent(next);
    keepDraft(guide.content, next);
  };
  return (
    <>
      <div className="xl:col-span-2">
        <GuideEditor guide={guide} content={content} onEdit={edit} />
      </div>
      <div className="flex flex-col gap-4">
        <PublishPanel guide={guide} dirty={content.body !== guide.content.body} />
        <PingRolesPanel roles={guide.ping_roles} />
        <ReviewRulesPanel />
      </div>
    </>
  );
}

function GuideEditor({ guide, content, onEdit }: GuideEditorProps) {
  const [view, setView] = useUrlEnum("editor", EDITOR_VIEWS, "edit");
  const save = useAction({
    run: saveGuide,
    pending: () => "Saving the guide…",
    done: () => "Guide content saved",
    invalidates: [keys.moderation.guide()],
    onDone: discardDraft,
  });
  const dirty = content.body !== guide.content.body;

  return (
    <Section
      title="Guide content"
      actions={
        <>
          <Segmented label="Guide view" value={view} options={VIEW_OPTIONS} onChange={setView} />
          <Button variant="primary" size="sm" disabled={!dirty} pending={save.isPending} onClick={() => save.mutate(content)}>
            Save
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-2">
        <p className="m-0 text-12 text-muted">Edits are saved here first, then published to Discord from the Publish panel.{dirty && " Unsaved edits are kept in this tab until you save."}</p>
        {view === "edit" ? (
          <>
            <Textarea aria-label="Guide content" mono rows={EDITOR_ROWS} value={content.body} onChange={(event) => onEdit(event.target.value)} className="w-full" />
            <span className="text-12 text-subtle">
              Discord markdown: <code>## heading</code>, <code>**bold**</code>, <code>`code`</code>, <code>-# </code> for small text, and <code>---</code> on its own line for a divider.
            </span>
          </>
        ) : (
          <GuidePreview body={content.body} button={GUIDE_BUTTON} />
        )}
      </div>
    </Section>
  );
}
