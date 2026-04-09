export type UrchinTag = {
  type: string;
  reason?: string;
  added_by_id?: number;
  added_by_username?: string;
  added_on?: string;
};

export type UrchinResponse = {
  uuid: string;
  tags: UrchinTag[];
};

export function getUrchinLabel(type?: string): string | undefined {
  switch (type) {
    case "caution":
      return "Caution";
    case "confirmed_cheater":
      return "Confirmed Cheater";
    case "blatant_cheater":
      return "Blatant Cheater";
    case "closet_cheater":
      return "Closet Cheater";
    case "sniper":
      return "Sniper";
    case "legit_sniper":
      return "Legit Sniper";
    case "possible_sniper":
      return "Possible Sniper";
    case "account":
      return "Account";
    case "info":
      return "Info";
    default:
      return undefined;
  }
}

export function getUrchinIconPath(
  type?: string
): string | undefined {
  return type ? `/urchin/${type}.webp` : undefined;
}

export function getUrchinPriority(type?: string): number {
  switch (type) {
    case "sniper":
      return 1;
    case "possible_sniper":
      return 2;
    case "legit_sniper":
      return 3;
    case "confirmed_cheater":
      return 4;
    case "blatant_cheater":
      return 5;
    case "closet_cheater":
      return 6;
    case "caution":
      return 7;
    case "account":
      return 8;
    case "info":
      return 9;
    default:
      return 100;
  }
}

export function sortUrchinTags(tags: UrchinTag[]): UrchinTag[] {
  return [...tags].sort(
    (a, b) => getUrchinPriority(a.type) - getUrchinPriority(b.type)
  );
}
