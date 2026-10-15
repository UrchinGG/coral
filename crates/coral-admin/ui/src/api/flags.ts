import { queryOptions, useMutation, useQueryClient } from "@tanstack/react-query";

import { toast } from "../kit/toast";
import { postJson } from "./http";
import { keys } from "./keys";
import { getQuery, liveQuery, POLL_FLAGS_MS } from "./queryClient";
import type { Dismissal, Flags } from "./types/flags";

type DismissContext = { previous: Flags | undefined };

export function flagsQuery() {
  return queryOptions({
    queryKey: keys.flags(),
    queryFn: getQuery<Flags>("/flags"),
    ...liveQuery(POLL_FLAGS_MS),
  });
}

export function useDismissFlag() {
  const client = useQueryClient();
  return useMutation<Dismissal, Error, string, DismissContext>({
    mutationFn: (key) => postJson<Dismissal>(`/flags/${encodeURIComponent(key)}/dismiss`, {}),
    onMutate: async (key) => {
      await client.cancelQueries({ queryKey: keys.flags() });
      const previous = client.getQueryData<Flags>(keys.flags());
      if (previous !== undefined) {
        client.setQueryData<Flags>(keys.flags(), { ...previous, flags: previous.flags.filter((flag) => flag.key !== key) });
      }
      return { previous };
    },
    onError: (error, _key, context) => {
      if (context?.previous !== undefined) client.setQueryData(keys.flags(), context.previous);
      toast.error(`Could not dismiss the flag: ${error.message}`);
    },
    onSettled: () => client.invalidateQueries({ queryKey: keys.flags() }),
  });
}
