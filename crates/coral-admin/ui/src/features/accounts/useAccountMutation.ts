import { useMutation, useQueryClient, type QueryClient, type QueryKey } from "@tanstack/react-query";

import { invalidateAccount } from "../../api/accounts";
import { keys } from "../../api/keys";
import type { AccountCore } from "../../api/types/accounts";
import { toast } from "../../kit/toast";

export type QueryPatch<V> = { apply: (client: QueryClient, variables: V) => Promise<() => void> };

type AccountMutation<V, R> = {
  run: (variables: V) => Promise<R>;
  success: (result: R, variables: V) => string;
  optimistic?: (core: AccountCore, variables: V) => AccountCore;
  patches?: readonly QueryPatch<V>[];
  inDialog?: boolean;
};

export function useAccountMutation<V = void, R = void>(discordId: string, { run, success, optimistic, patches = [], inDialog = false }: AccountMutation<V, R>) {
  const client = useQueryClient();
  const applied = optimistic === undefined ? patches : [patchQuery(keys.accounts.core(discordId), optimistic), ...patches];
  return useMutation({
    mutationFn: run,
    onMutate: (variables: V) => Promise.all(applied.map((patch) => patch.apply(client, variables))),
    onError: (error, _variables, rollbacks) => {
      for (const rollback of rollbacks ?? []) rollback();
      if (!inDialog) toast.error(error.message);
    },
    onSuccess: (result, variables) => {
      toast.ok(success(result, variables));
    },
    onSettled: () => invalidateAccount(client, discordId),
  });
}

export function patchQuery<T, V>(queryKey: QueryKey, change: (current: T, variables: V) => T): QueryPatch<V> {
  return {
    apply: async (client, variables) => {
      await client.cancelQueries({ queryKey });
      const previous = client.getQueryData<T>(queryKey);
      if (previous !== undefined) client.setQueryData(queryKey, change(previous, variables));
      return () => client.setQueryData(queryKey, previous);
    },
  };
}

export function withMember(core: AccountCore, change: (member: NonNullable<AccountCore["member"]>) => Partial<NonNullable<AccountCore["member"]>>): AccountCore {
  if (core.member === null) return core;
  return { ...core, member: { ...core.member, ...change(core.member) } };
}
