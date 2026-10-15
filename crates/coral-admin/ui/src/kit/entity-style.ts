import {
  ArrowLeftRight,
  Bot,
  Box,
  Globe,
  Hash,
  KeyRound,
  Package,
  Text,
  User,
  UserRound,
  type LucideIcon,
} from "lucide-react";

import type { EntityKind } from "../api/types/refs";

export const ENTITY_ICON: Record<EntityKind, LucideIcon> = {
  account: User,
  player: Box,
  name: Text,
  request: ArrowLeftRight,
  ip: Globe,
  key: KeyRound,
  guild: Hash,
  plugin: Package,
  internal: Bot,
  anonymous: UserRound,
};

export const ENTITY_TEXT: Record<EntityKind, string> = {
  account: "text-entity-account",
  player: "text-entity-player",
  name: "text-entity-name",
  request: "text-entity-request",
  ip: "text-entity-ip",
  key: "text-entity-key",
  guild: "text-entity-guild",
  plugin: "text-entity-plugin",
  internal: "text-entity-account",
  anonymous: "text-subtle",
};

export const ENTITY_SOFT: Record<EntityKind, string> = {
  account: "bg-entity-account/14 text-entity-account",
  player: "bg-entity-player/12 text-entity-player",
  name: "bg-entity-name/12 text-entity-name",
  request: "bg-entity-request/12 text-entity-request",
  ip: "bg-entity-ip/12 text-entity-ip",
  key: "bg-entity-key/12 text-entity-key",
  guild: "bg-entity-guild/12 text-entity-guild",
  plugin: "bg-entity-plugin/12 text-entity-plugin",
  internal: "bg-entity-account/10 text-entity-account",
  anonymous: "bg-surface-2 text-subtle",
};
