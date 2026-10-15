import type { EntityRef, Subject, SubjectForm } from "../../api/types/refs";
import type { SubjectMatch } from "../../api/types/requests";

export const FORM_LABEL: Record<SubjectForm, string> = {
  undashed: "undashed",
  dashed: "dashed",
  name: "name",
  display: "display",
  path: "path",
  body: "body",
};

export const MATCH_LABEL: Record<SubjectMatch, string> = {
  uuid: "uuid",
  resolved_name: "resolved name",
  name_only: "by name",
  display_name: "display name",
};

export function subjectRef(subject: Subject): EntityRef | null {
  if (subject.player !== null) return { kind: "player", uuid: subject.player };
  if (subject.name !== null) return { kind: "name", name: subject.name };
  return null;
}
