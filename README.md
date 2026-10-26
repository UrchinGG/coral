# Coral

Coral is a Hypixel stats and player-blacklist platform.

## Services

- `coral-api` — axum HTTP API backing everything else, serving OpenAPI docs at its root
- `coral-bot` — main Discord bot: stats, blacklist tags, evidence, reviews
- `coral-sync` — second bot handling account linking and Minecraft verification
- `coral-admin` — internal admin panel (Discord OAuth, audit log)
- `mc-verify` — 1.8 Minecraft server players join to prove ownership of an account
- `coral-web` — Next.js frontend

## Stack

Rust (tokio, axum, sqlx, serenity), Postgres, Redis, Next.js + Tailwind.

## Deployment

Every push to master runs CI on the self-hosted runners, which builds and deploys only
the services the change affects. Production's compose file, Caddyfile and tunnel routes
live in `deploy/`; secrets and private hosts stay in the server's `.env`.

## License

MIT licensed — see [LICENSE](LICENSE).
