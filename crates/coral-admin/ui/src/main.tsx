import { QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { RouterProvider } from "react-router-dom";

import "./styles/index.css";
import { queryClient } from "./api/queryClient";
import { AppFailure } from "./app/AppFailure";
import { BootContext, loadBoot } from "./app/boot";
import { router } from "./router";

const ROOT_ELEMENT_ID = "root";

const container = document.getElementById(ROOT_ELEMENT_ID);
if (container === null) throw new Error(`index.html has no #${ROOT_ELEMENT_ID} element`);
const root = createRoot(container);

loadBoot().then(
  (boot) =>
    root.render(
      <StrictMode>
        <BootContext value={boot}>
          <QueryClientProvider client={queryClient}>
            <RouterProvider router={router} />
          </QueryClientProvider>
        </BootContext>
      </StrictMode>,
    ),
  (error: unknown) => root.render(<AppFailure title="Coral admin could not start" error={error} />),
);
