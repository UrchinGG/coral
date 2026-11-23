# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Urchin is a community anti-cheat for Hypixel: players tag cheaters through a Discord bot, and staff review the tags. Starfish is its paid desktop companion. Most Starfish buyers are already Urchin users: they know the bot, the Discord and the blacklist before they reach the purchase flow, so they arrive having decided and want to pay and start playing.

The web surfaces serve three jobs: learning what Starfish and the Urchin API do (`urchin.gg`, `urchin.gg/starfish`), buying a Starfish license, and managing an Urchin account (`urchin.gg/dashboard`): linked Minecraft accounts, API keys and Starfish downloads.

## Product Purpose

Starfish is an injectable game overlay and Lua script loader that attaches to a running Minecraft 1.8.9 process on Windows, macOS and Linux. It brings Urchin into the game: tablist stats, blacklist alerts and tags, cheater detection, nick resolving and BedWars utilities, plus a plugin registry anyone can publish to.

## Positioning

Starfish is the only client add-on wired to the Urchin blacklist: tags staff confirm in Discord show up on nametags and in the tablist in-game, and players can tag without leaving the game. It injects into whatever 1.8.9 client the player already uses rather than replacing it.

## Operating Context

- Sign-in is Discord OAuth, started at `urchin.gg/auth/discord`; one session cookie covers every urchin.gg site.
- A Starfish license belongs to a Discord account and is used on a bound device; device changes are limited.
- The desktop app signs in with Discord and refuses to run without an active license.

## Capabilities and Constraints

- Starfish costs $14.99, paid once: a lifetime license that never expires.
- Payments go through PayNow, the merchant of record and authorized reseller. Checkout is PayNow's hosted page; the site never handles card or wallet details.
- Buyers can pay by card, PayPal, Apple and Google Pay, regional methods and crypto. Crypto takes longer to confirm, and a wrong amount or network cannot be recovered.
- A refund deactivates the license; a chargeback suspends it until resolved.
- PayNow requires every storefront page to link its Terms of Use, Buyer Agreement and Privacy Policy and to name PayNow as the authorized seller.
- Downloads are served per platform (Windows, macOS, Linux) from `/api/v1/download/latest`.

## Brand Commitments

- Names: Urchin, Starfish. The Starfish wordmark is set in Minecraft formatting colours (§6S§eta§fr§bfi§3sh).
- The sites share one visual system across urchin.gg, the Starfish site and the dashboard; new pages stay consistent with it (owner, 2026-11-23).
- Not affiliated with Hypixel Inc., Mojang AB or Microsoft, and the footer says so.

## Evidence on Hand

- In-game showcase data and components on the Starfish homepage (tablist, chat alert, process window).
- No testimonials, user counts or reviews exist; do not invent them.

## Product Principles

- Buyers have already decided: the purchase flow removes steps rather than selling again.
- Say exactly what happens to money and access: price, one-time, who the seller is, when the license turns on.
- One account everywhere: Discord identity is shown wherever a license or purchase is tied to it.
