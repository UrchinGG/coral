# coral-admin

The staff dashboard's backend: an axum server (`src/`) that signs staff in, answers the dashboard's
JSON API under `/api`, and serves the dashboard site from `web/sites/admin`, embedded into the binary
with [`rust-embed`](https://docs.rs/rust-embed).

## Development

```bash
# terminal 1: the backend, restarted only for Rust changes
cargo run -p coral-admin

# terminal 2: the site, from web/, reloading on every save
npm run dev:admin
```

The site's dev server (http://localhost:4323) forwards `/api`, `/auth` and `/login` to
`http://localhost:8080`, so it behaves like the deployed dashboard.

## Production build

`rust-embed` reads `web/sites/admin/dist/` when coral-admin compiles, so build the site first:

```bash
(cd web && npm run build:admin)
cargo build -p coral-admin
```

The Docker build does this in its `admin-ui` stage. Cargo doesn't notice when only the site changed;
touch `src/routes/mod.rs`, where the embed lives, to make it re-embed.

## Layout

- `src/`: routes, sign-in, identity resolution and audit logging
- `../../web/sites/admin/`: the dashboard site, one Astro page per screen
