import { useMutation, useQueryClient } from "@tanstack/react-query";

import { runExtensionAction } from "../../api/extensions";
import { keys } from "../../api/keys";
import { errorText } from "../../api/mutate";
import type { ViewAction } from "../../api/types/extension";
import { toast } from "../../kit/toast";

export type ActionRun = { action: ViewAction; values: Record<string, string> };

export function useExtensionAction(id: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ action, values }: ActionRun) => runExtensionAction(id, action.action, { ...action.input, ...values }),
    onSuccess: (response) => {
      queryClient.setQueryData(keys.extensions.view(id), response.view);
      void queryClient.invalidateQueries({ queryKey: keys.flags() });
      toast.ok(response.message);
    },
    onError: (error) => toast.error(errorText(error)),
  });
}
