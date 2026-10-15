import { Pill } from "../../kit/Badges";

type PluginFlagsProps = { official: boolean; unlisted: boolean; disabled: boolean };

export function PluginFlags({ official, unlisted, disabled }: PluginFlagsProps) {
  if (!official && !unlisted && !disabled) return <span className="text-subtle">listed</span>;
  return (
    <span className="flex flex-wrap gap-1">
      {official && <Pill tone="ok">official</Pill>}
      {unlisted && <Pill tone="warning">unlisted</Pill>}
      {disabled && <Pill tone="danger">disabled</Pill>}
    </span>
  );
}
