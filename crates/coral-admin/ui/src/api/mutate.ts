import { useMutation, useQueryClient, type QueryKey } from "@tanstack/react-query";
import type { ReactNode } from "react";

import { toast } from "../kit/toast";

type ActionOptions<V, R> = {
  run: (variables: V) => Promise<R>;
  pending: (variables: V) => ReactNode;
  done: (result: R, variables: V) => ReactNode;
  invalidates: readonly QueryKey[];
  onDone?: (result: R, variables: V) => void;
};

type OptimisticOptions<V, T> = {
  run: (variables: V) => Promise<unknown>;
  target: QueryKey;
  apply: (current: T, variables: V) => T;
  done: (variables: V) => ReactNode;
  invalidates: readonly QueryKey[];
};

export function useAction<V, R>({ run, pending, done, invalidates, onDone }: ActionOptions<V, R>) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (variables: V) => {
      const progress = toast.pending(pending(variables));
      try {
        const result = await run(variables);
        progress.settle(done(result, variables));
        return result;
      } catch (error) {
        progress.settle(errorText(error), "error");
        throw error;
      }
    },
    onSuccess: async (result, variables) => {
      onDone?.(result, variables);
      await Promise.all(invalidates.map((queryKey) => queryClient.invalidateQueries({ queryKey })));
    },
  });
}

export function useOptimistic<V, T>({ run, target, apply, done, invalidates }: OptimisticOptions<V, T>) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: run,
    onMutate: async (variables: V) => {
      await queryClient.cancelQueries({ queryKey: target });
      const previous = queryClient.getQueryData<T>(target);
      if (previous !== undefined) queryClient.setQueryData<T>(target, apply(previous, variables));
      return { previous };
    },
    onSuccess: (_result, variables) => {
      toast.ok(done(variables));
    },
    onError: (error, _variables, context) => {
      if (context?.previous !== undefined) queryClient.setQueryData(target, context.previous);
      toast.error(errorText(error));
    },
    onSettled: async () => {
      await Promise.all([target, ...invalidates].map((queryKey) => queryClient.invalidateQueries({ queryKey })));
    },
  });
}

export function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
