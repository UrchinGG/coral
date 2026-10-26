# web

The Urchin websites: urchin.gg (the API docs and the replay viewer) and
starfish.urchin.gg.

Both are static sites built with [Astro](https://astro.build). Working on them needs
only Node 22; no Rust, database or Docker. Docs and patch notes come from the live
api.urchin.gg.

## Development

Run these from `web/`:

- `npm install` installs dependencies
- `npm run dev` starts urchin.gg on http://localhost:4321
- `npm run dev:starfish` starts starfish.urchin.gg on http://localhost:4322
- `npm run build` type-checks and builds both sites to `sites/*/dist/`
- `npm run check` type-checks both sites and checks formatting
- `npm run format` formats everything with Prettier

`CORAL_API_URL`, `URCHIN_URL` and `STARFISH_URL` override the default addresses.

Every pull request from a branch in this repository gets preview links,
`pr-<number>.urchin.gg` and `pr-<number>-starfish.urchin.gg`, which update on each push.
Merging to master publishes both sites.

## Replay viewer

`/replays` needs the viewer's wasm, which is not in this repository yet. Without it the
page loads but says the viewer could not start. To run it locally, build the wasm with
`npm run build:viewer` from a checkout of urchin-replay next to coral, and add the baked
textures and sounds to `sites/urchin/public/replay-assets`.

## Layout

```
shared/astro.ts         Astro config shared by both sites
shared/styles/          design tokens, reset and base styles
shared/layouts/         page shell (head, header, footer)
shared/components/      UI used by both sites
sites/urchin/           urchin.gg: pages, OpenAPI docs and the replay viewer
sites/starfish/         starfish.urchin.gg: pages and plugin docs
sites/*/public/         each site's logos and static files
```
