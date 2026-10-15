import { lazy, Suspense, type ComponentProps } from "react";

import { SkeletonLines } from "./States";

const JsonView = lazy(() => import("./JsonView").then((module) => ({ default: module.JsonView })));

export function LazyJsonView(props: ComponentProps<typeof JsonView>) {
  return (
    <Suspense fallback={<SkeletonLines />}>
      <JsonView {...props} />
    </Suspense>
  );
}
