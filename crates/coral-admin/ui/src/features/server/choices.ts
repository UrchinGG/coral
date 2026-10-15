import type { DiscordChannel, DiscordRole } from "../../api/types/platform";

export type PickerChoice = { id: string; label: string; color?: string | null; hint?: string; disabled?: boolean };

export type RoleManager = "sync" | "bot";

type RoleChoiceOptions = { taken?: ReadonlySet<string>; manager?: RoleManager };

const NO_TAKEN_ROLES: ReadonlySet<string> = new Set();

export function roleChoices(roles: readonly DiscordRole[] | undefined, { taken = NO_TAKEN_ROLES, manager = "sync" }: RoleChoiceOptions = {}): PickerChoice[] | undefined {
  return roles?.map((role) => {
    const assignable = assignableBy(role, manager);
    return {
      id: role.id,
      label: role.name,
      color: role.color,
      disabled: assignable !== true || taken.has(role.id),
      hint: roleHint(role, assignable, taken),
    };
  });
}

export function channelChoices(channels: readonly DiscordChannel[] | undefined): PickerChoice[] | undefined {
  return channels?.filter((channel) => channel.kind === "text").map((channel) => ({ id: channel.id, label: channel.name, hint: channel.category ?? undefined }));
}

function assignableBy(role: DiscordRole, manager: RoleManager): boolean | null {
  return manager === "sync" ? role.assignable : role.bot_assignable;
}

function roleHint(role: DiscordRole, assignable: boolean | null, taken: ReadonlySet<string>): string | undefined {
  if (taken.has(role.id)) return "has rule";
  if (role.managed) return "managed";
  if (assignable === null) return "bot not configured";
  return assignable ? undefined : "not manageable";
}
