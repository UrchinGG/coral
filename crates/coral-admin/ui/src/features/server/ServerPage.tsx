import { useMutation, useQuery, useQueryClient, type UseQueryResult } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";

import { keys } from "../../api/keys";
import { errorText } from "../../api/mutate";
import { directoryQuery, populationQuery, refreshDirectory, refreshPopulation, serverConfigQuery } from "../../api/server";
import type { Directory, Population, ServerConfig } from "../../api/types/platform";
import { Button } from "../../kit/Button";
import { KpiStrip, KpiTile } from "../../kit/KpiTile";
import { QueryContent } from "../../kit/QueryContent";
import { ErrorState, SkeletonLines } from "../../kit/States";
import { Tabs } from "../../kit/Tabs";
import { toast } from "../../kit/toast";
import { formatNumber } from "../../lib/format";
import { useUrlEnum } from "../../lib/url-state";
import { PageHead } from "../platform/PageHead";
import { When } from "../platform/When";
import { Autoroles } from "./Autoroles";
import { LinkSettings } from "./LinkSettings";
import { NicknameFormat } from "./NicknameFormat";
import { ActiveJobs, SyncJobs } from "./SyncJobs";

const SERVER_TABS = ["linking", "nicknames", "autoroles", "jobs"] as const;
type ServerTab = (typeof SERVER_TABS)[number];
const TAB_ITEMS = [
  { value: "linking", label: "Account linking" },
  { value: "nicknames", label: "Display names" },
  { value: "autoroles", label: "Autoroles" },
  { value: "jobs", label: "Bulk updates" },
] as const;

export function ServerPage() {
  const [tab, setTab] = useUrlEnum("tab", SERVER_TABS, "linking");
  const config = useQuery(serverConfigQuery());
  const directory = useQuery(directoryQuery());
  const population = useQuery(populationQuery());

  return (
    <>
      <PageHead
        title={directory.data?.guild.name ?? "Server"}
        lede="The home Discord server: account linking, display names, autoroles and the bulk updates they start."
        actions={<RefreshDiscord />}
      />
      <DiscordFacts config={config.data} directory={directory} population={population} />
      {tab !== "jobs" && <ActiveJobs />}
      <Tabs label="Server settings" value={tab} tabs={TAB_ITEMS} onChange={setTab} idPrefix="server" />
      <div id="server-panel" role="tabpanel" aria-labelledby={`server-tab-${tab}`} className="flex flex-col gap-4">
        {tab === "jobs" ? (
          <SyncJobs />
        ) : (
          <QueryContent query={config}>{(data) => <SettingsPanel tab={tab} config={data} directory={directory.data} memberCount={population.data?.member_count} />}</QueryContent>
        )}
      </div>
    </>
  );
}

function SettingsPanel({ tab, config, directory, memberCount }: { tab: Exclude<ServerTab, "jobs">; config: ServerConfig; directory: Directory | undefined; memberCount: number | undefined }) {
  switch (tab) {
    case "linking":
      return <LinkSettings config={config.config} directory={directory} memberCount={memberCount} />;
    case "nicknames":
      return <NicknameFormat key={config.config.nickname_template ?? ""} template={config.config.nickname_template} previewContext={config.preview_context} memberCount={memberCount} />;
    case "autoroles":
      return <Autoroles rules={config.rules} roles={directory?.roles} previewContext={config.preview_context} memberCount={memberCount} />;
  }
}

function DiscordFacts({ config, directory, population }: { config: ServerConfig | undefined; directory: UseQueryResult<Directory>; population: UseQueryResult<Population> }) {
  if (population.data === undefined && population.error === null) return <SkeletonLines widths={["30%"]} />;
  return (
    <div className="flex flex-col gap-2">
      {population.data !== undefined && (
        <KpiStrip compact>
          <KpiTile label="Members" value={formatNumber(population.data.member_count)} sub={<>counted <When at={population.data.as_of} date="auto" /></>} />
          <KpiTile label="Linked" value={formatNumber(population.data.linked_members)} sub="with a Minecraft account" />
          <KpiTile label="Server id" value={<span className="font-mono text-15">{config?.guild_id ?? directory.data?.guild.id ?? "—"}</span>} sub={directory.data === undefined ? undefined : <>roles read <When at={directory.data.as_of} date="auto" /></>} />
        </KpiStrip>
      )}
      {population.error !== null && <ErrorState error={population.error} title="Member counts unavailable" onRetry={() => void population.refetch()} retrying={population.isFetching} />}
      {directory.error !== null && <ErrorState error={directory.error} title="Roles and channels unavailable" onRetry={() => void directory.refetch()} retrying={directory.isFetching} />}
    </div>
  );
}

function RefreshDiscord() {
  const queryClient = useQueryClient();
  const refresh = useMutation({
    mutationFn: () => Promise.all([refreshDirectory(), refreshPopulation()]),
    onSuccess: ([directory, population]) => {
      queryClient.setQueryData(keys.server.directory(), directory);
      queryClient.setQueryData(keys.server.population(), population);
      toast.ok("Read the server's roles, channels and members again");
    },
    onError: (error) => toast.error(errorText(error)),
  });
  return (
    <Button onClick={() => refresh.mutate()} pending={refresh.isPending} icon={<RefreshCw className="size-3" />}>
      Read from Discord
    </Button>
  );
}
