export type FlagLevel = "info" | "warning" | "danger";

export type ViewTone = "neutral" | "subtle" | "ok" | "info" | "warning" | "limited" | "danger";

export type ExtensionFlag = {
  key: string;
  level: FlagLevel;
  summary: string;
};

export type ExtensionView = {
  title: string;
  subtitle: string | null;
  refresh_s: number | null;
  sections: ViewSection[];
};

export type ViewSection =
  | { type: "notice"; level: FlagLevel; text: string; actions: ViewAction[] }
  | { type: "stats"; items: ViewStat[] }
  | { type: "table"; id: string; title: string; columns: ViewColumn[]; rows: ViewRow[] }
  | { type: "form"; id: string; title: string; fields: ViewField[]; submit: ViewAction }
  | { type: "series"; id: string; title: string; unit: string | null; lines: SeriesLine[] };

export type ViewStat = {
  label: string;
  value: string;
  detail: string | null;
};

export type ViewColumn = {
  key: string;
  label: string;
  numeric: boolean;
};

export type ViewRow = {
  key: string;
  cells: Record<string, ViewCell>;
  actions: ViewAction[];
};

export type ViewCell =
  | { kind: "text"; text: string; tone: ViewTone }
  | { kind: "mono"; text: string }
  | { kind: "number"; value: number; suffix: string | null; tone: ViewTone }
  | { kind: "pill"; text: string; tone: ViewTone }
  | { kind: "meter"; used: number; limit: number }
  | { kind: "time"; at: string }
  | { kind: "secret"; value: string };

export type ViewField = {
  name: string;
  label: string;
  required: boolean;
} & FieldInput;

export type FieldInput =
  | { kind: "text"; placeholder: string | null; value: string | null }
  | { kind: "secret"; placeholder: string | null }
  | { kind: "select"; options: FieldOption[]; value: string | null };

export type FieldOption = {
  value: string;
  label: string;
};

export type ViewAction = {
  action: string;
  label: string;
  danger: boolean;
  input: Record<string, unknown>;
  fields: ViewField[];
  confirm: ViewConfirm | null;
};

export type ViewConfirm = {
  title: string;
  text: string;
  phrase: string | null;
};

export type SeriesLine = {
  label: string;
  tone: ViewTone;
  points: SeriesPoint[];
};

export type SeriesPoint = {
  t: string;
  value: number;
};

export type ActionResponse = {
  message: string;
  view: ExtensionView;
};
