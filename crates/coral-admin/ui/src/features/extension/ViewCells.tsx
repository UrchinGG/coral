import { Eye, EyeOff } from "lucide-react";
import { useState } from "react";

import type { ViewCell } from "../../api/types/extension";
import { Pill } from "../../kit/Badges";
import { IconButton } from "../../kit/Button";
import { CopyButton } from "../../kit/Copy";
import { Meter } from "../../kit/Meter";
import { TONE_TEXT } from "../../kit/tone";
import { cx } from "../../lib/cx";
import { formatNumber } from "../../lib/format";
import { When } from "../platform/When";
import { VIEW_TONE } from "./tones";

const SECRET_MASK = "•";
const HIDDEN_CHARACTER = /[^-]/g;


export function ViewCellValue({ cell }: { cell: ViewCell | undefined }) {
  if (cell === undefined) return null;
  switch (cell.kind) {
    case "text":
      return <span className={cx("wrap-anywhere", cell.tone === "subtle" ? "text-subtle" : cell.tone === "neutral" ? undefined : TONE_TEXT[VIEW_TONE[cell.tone]])}>{cell.text}</span>;
    case "mono":
      return <span className="font-mono text-12 wrap-anywhere">{cell.text}</span>;
    case "number":
      return (
        <span className={cx("font-mono", cell.tone === "neutral" ? undefined : TONE_TEXT[VIEW_TONE[cell.tone]])}>
          {formatNumber(cell.value)}
          {cell.suffix}
        </span>
      );
    case "pill":
      return <Pill tone={VIEW_TONE[cell.tone]}>{cell.text}</Pill>;
    case "meter":
      return <Meter used={cell.used} limit={cell.limit} inline />;
    case "time":
      return <When at={cell.at} />;
    case "secret":
      return <SecretValue value={cell.value} />;
  }
}

function SecretValue({ value }: { value: string }) {
  const [shown, setShown] = useState(false);
  return (
    <span className="inline-flex min-w-0 items-center gap-1">
      <span className={cx("font-mono text-12 break-all", shown ? "text-entity-key" : "text-subtle")}>{shown ? value : value.replace(HIDDEN_CHARACTER, SECRET_MASK)}</span>
      {shown && <CopyButton text={value} />}
      <IconButton label={shown ? "Hide" : "Reveal"} size="sm" onClick={() => setShown(!shown)}>
        {shown ? <EyeOff className="size-3" /> : <Eye className="size-3" />}
      </IconButton>
    </span>
  );
}
