# coral-admin

Internal moderation and diagnostics dashboard: an axum backend (`src/`) serving a React/TypeScript SPA (`ui/`) that's embedded into the compiled binary via [`rust-embed`](https://docs.rs/rust-embed).

## Fast dev loop

The old hand-rolled frontend was `include_str!`'d directly into the Rust binary, so any HTML/CSS/JS change required a full `cargo build` to see it. That's gone. The frontend now runs its own dev server with hot module replacement:

```bash
# terminal 1 — backend, only needs restarting for Rust changes
cargo run -p coral-admin

# terminal 2 — frontend, HMR on every save, no Rust involved
cd ui && npm run dev
```

`ui/vite.config.ts` proxies `/api` and `/auth` to `http://localhost:8080`, so the dev server (`http://localhost:5173` by default) behaves exactly like the production app — sign in once, then edit any `.tsx`/`.ts` file and see it update instantly without a page reload, let alone a Rust recompile.

## Production build

`rust-embed`'s `#[folder = "ui/dist/"]` macro reads the compiled frontend assets **at Rust compile time**, so `ui/dist/` has to exist and be current before `cargo build` runs:

```bash
cd ui && npm run build   # writes ui/dist/
cd .. && cargo build -p coral-admin
```

The Docker build does this automatically (see the root `Dockerfile`'s `admin-ui-deps`/`admin-ui-builder` stages, which run before the Rust `builder` stage). Locally, cargo does **not** know to invalidate its build cache when only `ui/dist/` changes — if you rebuild the frontend and the binary doesn't pick it up, touch a file under `src/` (e.g. `src/routes/mod.rs`, where the embed macro lives) to force a recompile.

## Layout

- `src/` — axum backend: routes, auth, identity resolution, audit logging
- `ui/` — Vite + React + TypeScript + TanStack Query/Table + react-router SPA
- `ui/dist/` — build output, gitignored, only exists after `npm run build`
