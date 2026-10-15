import { RefreshCw } from "lucide-react";
import { isRouteErrorResponse, useRouteError } from "react-router-dom";

import { Button } from "../kit/Button";
import { ErrorState } from "../kit/States";
import { AppFailure } from "./AppFailure";
import { NotFound } from "./NotFound";

const NOT_FOUND = 404;
const STALE_CHUNK = /dynamically imported module|Importing a module script failed|error loading dynamically imported module/i;

export function RouteError() {
  const error = useRouteError();
  if (isRouteErrorResponse(error) && error.status === NOT_FOUND) return <NotFound />;
  if (isStaleChunk(error)) return <StaleBuild />;
  return (
    <div className="flex flex-col gap-3">
      <h1 className="text-22 font-semibold">This page failed to render</h1>
      <ErrorState error={reportable(error)} onRetry={() => window.location.reload()} />
    </div>
  );
}

export function ShellError() {
  const error = useRouteError();
  if (!isStaleChunk(error)) return <AppFailure title="The dashboard failed to render" error={reportable(error)} />;
  return (
    <main className="mx-auto flex min-h-dvh max-w-xl flex-col justify-center p-6">
      <StaleBuild />
    </main>
  );
}

function StaleBuild() {
  return (
    <div className="flex flex-col items-start gap-3">
      <h1 className="text-22 font-semibold">A new build is live</h1>
      <p className="m-0 max-w-[70ch] text-muted">This page's code could not be loaded, which happens after a deploy. Reload to get the current build.</p>
      <Button variant="primary" icon={<RefreshCw className="size-3" />} onClick={() => window.location.reload()}>
        Reload
      </Button>
    </div>
  );
}

function isStaleChunk(error: unknown): boolean {
  return error instanceof Error && STALE_CHUNK.test(error.message);
}

function reportable(error: unknown): unknown {
  return isRouteErrorResponse(error) ? new Error(`${error.status} ${error.statusText}`) : error;
}
