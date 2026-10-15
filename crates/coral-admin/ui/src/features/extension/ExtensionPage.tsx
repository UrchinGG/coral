import { useQuery } from "@tanstack/react-query";
import { useParams } from "react-router-dom";

import { extensionViewQuery } from "../../api/extensions";
import type { ExtensionView } from "../../api/types/extension";
import { QueryContent } from "../../kit/QueryContent";
import { PageHead } from "../platform/PageHead";
import { useExtensionAction } from "./useExtensionAction";
import { ViewSectionBlock } from "./ViewSections";

export function ExtensionPage() {
  const { id = "" } = useParams();
  const view = useQuery(extensionViewQuery(id));
  return <QueryContent query={view}>{(data) => <ExtensionScreen id={id} view={data} />}</QueryContent>;
}

function ExtensionScreen({ id, view }: { id: string; view: ExtensionView }) {
  const action = useExtensionAction(id);
  const runner = { pending: action.isPending ? (action.variables?.action ?? null) : null, run: action.mutateAsync };
  const forms = view.sections.filter((section) => section.type === "form");
  const rest = view.sections.filter((section) => section.type !== "form");
  return (
    <>
      <PageHead title={view.title} lede={view.subtitle ?? undefined} />
      {rest.map((section, index) => (
        <ViewSectionBlock key={sectionKey(section, index)} section={section} runner={runner} />
      ))}
      {forms.length > 0 && (
        <div className="grid gap-4 xl:grid-cols-2">
          {forms.map((section, index) => (
            <ViewSectionBlock key={sectionKey(section, index)} section={section} runner={runner} />
          ))}
        </div>
      )}
    </>
  );
}

function sectionKey(section: ExtensionView["sections"][number], index: number): string {
  return "id" in section ? section.id : `${section.type}:${index}`;
}
