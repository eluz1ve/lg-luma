# lg-luma-plus

## 1.0.0

### Major Changes

- First release of LG Luma+, built on [Luma](https://github.com/ecklf/lg-luma) 2.16.2. Luma+ has its own app ID, `com.eluz1ve.lumaplus`, installs beside Luma with its own settings, and updates from this repository's releases. Luma's history up to 2.16.2 is in [its changelog](https://github.com/ecklf/lg-luma/blob/main/CHANGELOG.md).
- Add the Today card beside the greeting: live and upcoming CS2 matches, new episodes and coin prices, opened with OK into a tab for each widget.
- Add the Lampa widget: what Lampa viewers are watching now, or Continue watching from a CUB account signed in with a QR code. OK opens the card in Lampa through the Luma+ Lampa plugin (`lampa/luma.js`), with instructions in Widgets › Lampa. Outside Russian, series show their English posters from TVmaze, found by IMDb id, and CUB's otherwise.
- Add CS2 Score: Counter-Strike 2 matches on Home from Liquipedia, live first, with team logos, scores of finished matches, and a match dialog with the round, the current map, each map's rounds and both lineups.
- Add crypto prices: up to four coins, from the usual eight or any Binance coin found by search, with their own marks, in the Today card or beside the greeting.
- Replace the tab bar with a tvOS-style side menu, glass panels and a softer focus with a subtle lift, zoom and glow.
- Redesign Settings around a live preview of Home that follows every change.
- Add General › Interface scale: Normal, Large (1.25×) or Extra large (1.5×) for the clock, app categories, search, app names and input cards.
- Add Apple TV's aerial stills as a background that changes every fifteen minutes, and dynamic wallpapers that follow the sun at the weather location.
- Add built-in LG AirPlay to Inputs with shared visibility and rename settings.
- Add a Close all button to the app switcher, closing the displayed apps one at a time, and read and close running apps directly from the rooted Home worker.
- Add System › Luma+ as Home screen, which runs the takeover through Homebrew Channel's root service, and keep Home-replacement preload launches hidden.
- Add an optional setting that keeps Homebrew apps such as Moonlight current, verified by the repository checksum.
- Focus the first visible favorite when opening Home with the Home button or returning with Back.
- Draw text from a glyph atlas, which makes Home much cheaper to draw on the TV; show weather details, and look the weather place up again after a language change so its name follows the interface language.
