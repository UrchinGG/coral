import type { PlayerForm, PlayerRequestRow } from "../../api/types/players";
import type { Service, SubjectForm } from "../../api/types/refs";
import { SERVICE_LABEL } from "../../lib/refs";

const LISTED_FORM: Record<PlayerForm, SubjectForm | null> = {
  undashed: "undashed",
  dashed: "dashed",
  name: "name",
  former: "name",
  path: "path",
  body: null,
  display: "display",
};

export function matchedValue(row: PlayerRequestRow, uuid: string, names: readonly string[]): string | null {
  const form = LISTED_FORM[row.matched_form];
  if (form === null) return null;
  const subject = row.subjects.find((candidate) => candidate.form === form && (candidate.player === uuid || (candidate.name !== null && names.includes(candidate.name))));
  return subject?.raw ?? null;
}

export function serviceOf(row: PlayerRequestRow): Service | null {
  return row.requester.kind === "internal" ? row.requester.service : null;
}

export function viaLabel(service: Service): string {
  return service === "bot" ? "via bot" : `via ${SERVICE_LABEL[service]}`;
}
