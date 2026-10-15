import { useQuery } from "@tanstack/react-query";

import { maintenanceQuery } from "../../api/maintenance";
import type { Maintenance } from "../../api/types/platform";
import { QueryContent } from "../../kit/QueryContent";
import { Section } from "../../kit/Section";
import { CoverageBanner } from "../../kit/States";
import { Table, type Column } from "../../kit/Table";
import { PageHead } from "../platform/PageHead";
import { When } from "../platform/When";
import { JobCard } from "./JobCard";
import { JOB_DESCRIPTIONS } from "./jobs";

type Watermark = Maintenance["rollups"]["watermarks"][number];

const WATERMARK_COLUMNS: readonly Column<Watermark>[] = [
  { id: "name", header: "Pass", cell: (row) => <span className="font-mono text-12">{row.name}</span> },
  { id: "at", header: "Done through", cell: (row) => <When at={row.at} /> },
];

export function MaintenancePage() {
  const maintenance = useQuery(maintenanceQuery());
  return (
    <>
      <PageHead title="Maintenance" lede="Background jobs the owner starts by hand, and the rollup job that runs on its own. Backfills that read the request log run only 16:00–07:00 UTC and pause while a database dump is connected." />
      <QueryContent query={maintenance}>{(data) => <MaintenanceView data={data} fetching={maintenance.isFetching} />}</QueryContent>
    </>
  );
}

function MaintenanceView({ data, fetching }: { data: Maintenance; fetching: boolean }) {
  return (
    <>
      {data.window_open ? (
        <CoverageBanner>The backfill window is open: started backfills run now.</CoverageBanner>
      ) : (
        <CoverageBanner tone="warning">The backfill window is closed: a started backfill waits until 16:00 UTC, or until the database dump finishes.</CoverageBanner>
      )}
      <div className="grid gap-4 xl:grid-cols-2">
        {data.jobs.map((job) => (
          <JobCard key={job.name} job={job} description={JOB_DESCRIPTIONS[job.name] ?? null} pending={data.pending} fetching={fetching} />
        ))}
      </div>
      <Section title="Rollup watermarks" count={data.rollups.dirty_ranges === 0 ? "no dirty ranges" : `${data.rollups.dirty_ranges} dirty ranges queued`} flush>
        <Table label="Rollup watermarks" columns={WATERMARK_COLUMNS} rows={data.rollups.watermarks} rowKey={(row) => row.name} empty="No rollup pass has run yet." />
        <div className="border-t border-border px-3.5 py-2 text-12 text-muted">
          Oldest retained log partition: <When at={data.rollups.log_from} empty="none" />
        </div>
      </Section>
    </>
  );
}
