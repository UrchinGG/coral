import { useMutation, useQueryClient, type QueryKey } from "@tanstack/react-query";
import { useToast } from "../components/Toast";

type AdminMutationOptions = {
  successMessage: string;
  invalidateKeys?: QueryKey[];
};

export function useAdminMutation<TVariables, TData>(
  mutationFn: (vars: TVariables) => Promise<TData>,
  options: AdminMutationOptions,
) {
  const { notify } = useToast();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn,
    onSuccess: () => {
      notify(options.successMessage, "success");
      for (const key of options.invalidateKeys ?? []) {
        queryClient.invalidateQueries({ queryKey: key });
      }
    },
    onError: (error: unknown) => {
      notify(error instanceof Error ? error.message : "Action failed", "error");
    },
  });
}
