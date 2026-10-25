# Home grid, widgets and layout themes — design

Status: agreed with the fork owner on 2026-10-25 after a clickable prototype
(`prototype/home-grid.html` outside the repo). Phase 1 is built in the fork; the catalog is a
later phase.

## Intent

Home becomes a user-arranged grid of blocks instead of a fixed page. Built-in blocks (greeting,
weather, favourites, inputs) and widgets (crypto prices, season premieres) can be moved, resized,
removed and added with the remote. A **theme** is a ready-made arrangement of blocks plus a look
(wallpaper, accent, card surface). Later, themes and widgets are shared through a reviewed catalog
repository.

## Goals (phase 1)

- 12-column grid on the logical 1920×1080 layout: columns 124 px, gaps 24 px, rows 100 px, left
  edge 84 px. Blocks occupy whole cells and never overlap.
- Block kinds: `greeting`, `weather`, `favorites`, `inputs`, `crypto`, `premieres`; at most one of
  each. Each kind declares minimum, maximum and default size; its content adapts to its size
  (more coins, more posters, larger greeting).
- Editing with the remote: hold OK on any block opens a block menu: Move, Resize, Remove (not for
  greeting/favourites), and for favourites also "Reorder apps" (the existing move mode).
  Arrows move by one cell or grow/shrink from the bottom-right corner; blocks in the way are pushed
  down, reflowed from the layout at the start of the edit so they return when the edited block
  moves on. OK keeps, Back restores.
- "+ Add widget" below the last block lists the kinds not on Home and adds one at the first free
  place.
- Themes: Classic (today's Home), Cinema, Minimal, Dashboard, Gaming, Forest. Choosing one in
  General › Home theme replaces the layout and wallpaper and sets the accent and card surface.
- Crypto: Binance public 24 h mini tickers (USDT pairs), fallback `api.binance.us`, every 5 min.
- Season premieres: TVmaze web and US schedules from 3 days ago to 10 days ahead, episode 1 of
  Scripted/Animation shows with weight ≥ 95, de-duplicated by show, posters from
  `static.tvmaze.com`, refreshed every 6 h. OK on a poster opens a dialog: title, season, date,
  and "Open in …" with installed video apps (launch; the title stays in a notice).

## Constraints

- Network as for weather: ureq/rustls, fixed HTTPS origins, bounded replies, timeouts, no
  redirects, a worker thread, cached results; nothing on the SDL thread.
- Layout and theme live in the preferences (`home_layout`, `home_theme`); unknown or broken values
  fall back to the Classic layout without affecting other preferences.
- No on-screen data credits; attribution in docs (TVmaze CC BY-SA, Binance, Open-Meteo).

## Non-goals (phase 1)

TMDB movies (needs a per-user key), per-widget settings, deep-link search inside apps, the catalog
and declarative third-party widgets, free (non-grid) placement.

## Later phases

1. TMDB "Now playing / Upcoming" with a user key; per-widget settings (coins, country).
2. Catalog repository (`luma-catalog`): reviewed PRs with `themes/` and `widgets/`, a generated
   index with checksums; Luma reads one fixed index URL. Widgets are declarative (data source with
   declared origins, field mapping, filters, a built-in view kind), never code.
3. App "search recipes" in the catalog so "Open in …" can pass the title to apps that support it.
