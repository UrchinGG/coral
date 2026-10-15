import { useQuery } from "@tanstack/react-query";
import { ExternalLink } from "lucide-react";
import { useParams } from "react-router-dom";

import { pluginQuery } from "../../api/plugins";
import type { PluginDetail } from "../../api/types/platform";
import { EntityLink } from "../../kit/EntityLink";
import { KpiStrip, KpiTile } from "../../kit/KpiTile";
import { QueryContent } from "../../kit/QueryContent";
import { CoverageBanner } from "../../kit/States";
import { formatNumber } from "../../lib/format";
import { PageHead } from "../platform/PageHead";
import { When } from "../platform/When";
import { PluginActions } from "./PluginActions";
import { PluginFlags } from "./PluginFlags";
import { Releases } from "./Releases";
import { Reviews } from "./Reviews";

export function PluginPage() {
  const { slug = "" } = useParams();
  const plugin = useQuery(pluginQuery(slug));
  return <QueryContent query={plugin}>{(data) => <PluginView slug={slug} detail={data} fetching={plugin.isFetching} />}</QueryContent>;
}

function PluginView({ slug, detail, fetching }: { slug: string; detail: PluginDetail; fetching: boolean }) {
  const { plugin } = detail;
  return (
    <>
      <PageHead
        title={plugin.display_name}
        lede={plugin.description}
        actions={<PluginFlags official={plugin.official} unlisted={plugin.unlisted} disabled={plugin.disabled} />}
      />
      {plugin.disabled && (
        <CoverageBanner tone="warning">
          Disabled <When at={plugin.disabled_at} />: {plugin.disabled_reason ?? "no reason recorded"}
        </CoverageBanner>
      )}
      <KpiStrip compact>
        <KpiTile label="Slug" value={<span className="font-mono text-15">{plugin.slug}</span>} />
        <KpiTile label="Owner" value={<span className="text-15">{detail.owner === null ? "unknown" : <EntityLink entity={detail.owner} />}</span>} />
        <KpiTile label="Installs, 30 days" value={formatNumber(detail.installs_30d)} sub={`${formatNumber(detail.installs_total)} in total`} />
        <KpiTile label="Created" value={<When at={plugin.created_at} />} sub={<>updated <When at={plugin.updated_at} /></>} />
      </KpiStrip>
      <div className="flex flex-wrap items-center gap-3 text-12">
        <span className="font-mono wrap-anywhere">{plugin.repo}</span>
        {plugin.homepage !== null && (
          <a href={plugin.homepage} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 text-accent hover:underline">
            {plugin.homepage}
            <ExternalLink className="size-3" />
          </a>
        )}
        {plugin.unlisted_at !== null && (
          <span className="text-muted">
            unlisted <When at={plugin.unlisted_at} />
          </span>
        )}
      </div>
      <PluginActions slug={slug} plugin={plugin} />
      <Releases slug={slug} releases={detail.releases} fetching={fetching} />
      <Reviews slug={slug} reviews={detail.reviews} />
    </>
  );
}

