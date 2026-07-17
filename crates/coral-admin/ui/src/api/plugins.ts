import { useQuery } from "@tanstack/react-query";
import { apiDelete, apiGet, apiPost } from "./client";
import { useAdminMutation } from "./useAdminMutation";
import type { PluginDetailResponse, PluginListResponse } from "./types";

export function usePlugins(search: string, offset: number, limit: number) {
  const params = new URLSearchParams();
  params.set("limit", String(limit));
  params.set("offset", String(offset));
  if (search) params.set("search", search);

  return useQuery({
    queryKey: ["plugins", "list", params.toString()],
    queryFn: () => apiGet<PluginListResponse>(`/plugins?${params}`),
  });
}

export function usePlugin(slug: string) {
  return useQuery({
    queryKey: ["plugins", "detail", slug],
    queryFn: () => apiGet<PluginDetailResponse>(`/plugins/${slug}`),
    enabled: !!slug,
  });
}

const pluginKeys = (slug: string) => [["plugins", "detail", slug], ["plugins", "list"]];

export function useSetOfficial(slug: string) {
  return useAdminMutation((official: boolean) => apiPost(`/plugins/${slug}/official`, { official }), {
    successMessage: "Official status updated",
    invalidateKeys: pluginKeys(slug),
  });
}

export function useSetUnlisted(slug: string) {
  return useAdminMutation((unlisted: boolean) => apiPost(`/plugins/${slug}/unlisted`, { unlisted }), {
    successMessage: "Listing status updated",
    invalidateKeys: pluginKeys(slug),
  });
}

export function useSetDisabled(slug: string) {
  return useAdminMutation(
    (req: { disabled: boolean; reason?: string }) => apiPost(`/plugins/${slug}/disabled`, req),
    { successMessage: "Disabled status updated", invalidateKeys: pluginKeys(slug) },
  );
}

export function useDeletePlugin(slug: string) {
  return useAdminMutation<void, unknown>(() => apiDelete(`/plugins/${slug}`), {
    successMessage: "Plugin deleted",
    invalidateKeys: [["plugins", "list"]],
  });
}

export function useYankRelease(slug: string) {
  return useAdminMutation(
    (req: { version: string; reason?: string }) => apiPost(`/plugins/${slug}/releases/${req.version}/yank`, { reason: req.reason }),
    { successMessage: "Release yanked", invalidateKeys: pluginKeys(slug) },
  );
}

export function useUnyankRelease(slug: string) {
  return useAdminMutation((version: string) => apiPost(`/plugins/${slug}/releases/${version}/unyank`), {
    successMessage: "Release unyanked",
    invalidateKeys: pluginKeys(slug),
  });
}

export function useDeleteRelease(slug: string) {
  return useAdminMutation((version: string) => apiDelete(`/plugins/${slug}/releases/${version}`), {
    successMessage: "Release deleted",
    invalidateKeys: pluginKeys(slug),
  });
}

export function useDeleteReview(slug: string) {
  return useAdminMutation((userId: number) => apiDelete(`/plugins/${slug}/reviews/${userId}`), {
    successMessage: "Review deleted",
    invalidateKeys: pluginKeys(slug),
  });
}
