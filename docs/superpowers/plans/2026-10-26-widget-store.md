# Widget store Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Install data-only Home widgets from a signed catalog, and express today's esports widget as the first one, with live mode on the user's own PandaScore key.

**Architecture:** A new SDL-free workspace crate `widgets/` (`luma-widgets`) owns manifests, documents, the catalog's trust checks, request planning, jaq transforms and their child-process sandbox, and the authors' CLI `luma-widget`. Luma adds a widget worker thread (HTTP, cache, catalog, transforms through its own executable with `--widget-transform`), store/settings/secret state, `BlockKind::Widget(slot)`, and template renderers built on `row_header`/`card`/`track_cards`. The catalog and the esports widget live in a new public repo `eluz1ve/luma-widgets`, packed and signed by CI on merge.

**Tech Stack:** Rust 1.93, jaq-core 3 / jaq-std 3 / jaq-json 2, ed25519-compact 2, sha2, ureq 3 (rustls), libc (Unix rlimits), serde_json; GitHub Actions + Pages.

**Spec:** `docs/widgets.md`

## Global Constraints

- Manifest/index ≤ 256 KiB; ≤ 8 origins (`https://host[:port]`, no path/wildcard/IP/localhost); ≤ 12 settings; ≤ 200 strings; ≤ 4 sources; ≤ 8 steps; ≤ 16 requests per refresh; ≤ 8 headers.
- Reply ≤ 1 MiB (step `limit` may lower), ≤ 4 MiB into one transform; images ≤ 2 MiB, thumbnails 160×160 (logos, photos) and 300×450 (posters).
- Refresh clamped to 30…3600 s; back-off 60 s doubling to 15 min, ≥ 300 s after 429.
- Transform child: RLIMIT_CPU 2 s, RLIMIT_AS 256 MiB (Linux), 3 s wall clock then kill, output ≤ 512 KiB; programs ≤ 64 KiB each, ≤ 128 KiB with library; vars ≤ 1 KiB.
- Removed jq filters: `input`, `inputs`, `env`, `$ENV`, `now`, `debug`, `stderr`, `halt`, `halt_error`, `input_filename`, `include`, `import`. Added: `fmt($args)`.
- Document caps: 50 items, 200-char texts, 64-char ids, 8 chips, 7 tiles/slots, 2 lists × 12 rows × 4 extras, 2 sides, 512-char image URLs.
- Network: GET, HTTPS only, declared origins only (checked after substitution), no redirects, no proxy, 10 s timeout, `User-Agent: LumaNative`. Secrets only in headers, only to their `send_to` origins, never in URLs, logs, transforms or `preferences.json`; `secrets.json` mode 0600.
- Catalog: `https://eluz1ve.github.io/luma-widgets/`, `index.json` + `index.json.sig` (base64 ed25519 over exact bytes), `sequence` never decreases; at most 16 installed widgets and 16 slots.
- Luma version for the store: 2.9.0 (minor changeset). Every new UI string in en, de, zh, es, pt, it, ru.
- Never write a PandaScore key into a file, commit or build. Commits as `eluz1ve <8470376+eluz1ve@users.noreply.github.com>` on `feature/widget-store`, pushed to `fork` only.
- Don't run smoke tests; tests are `cargo test --locked -p luma -p luma-widgets`; checks are `pnpm check`.

## Review Focus

- A refresh in flight when the user changes settings, the secret or removes the widget → the stale result is dropped (generation), never shown on the new instance.
- An open dialog whose item disappears or moves in the next document → the dialog follows the item's `id` and closes when it is gone; focus never points past the end of the row.
- The transform child cannot be spawned (jail, missing exe) → the widget shows "stopped working", Luma keeps running; logged once.
- A newer manifest with a setting option removed → the instance's saved value falls back to the default, no panic, no empty `choices`.
- Catalog offline, or a store widget on Home while its manifest file is missing/corrupt → Home renders a ghost card, the store says why; nothing crashes.

---

### Task 1: `luma-widgets` crate: manifest and document types with validation

**Files:** Create `widgets/Cargo.toml`, `widgets/src/lib.rs`, `widgets/src/manifest.rs`, `widgets/src/document.rs`, `widgets/src/text.rs`, `widgets/tests/manifest.rs`, `widgets/tests/document.rs`; modify root `Cargo.toml` (workspace member, `luma-widgets = { path = "widgets" }`), `Cargo.lock`.
**Produces:** `Localized(BTreeMap<String,String>)` with `get(lang) -> &str` (falls back to `en`); `Manifest { schema, id, version: Version, luma: Version, name, description, authors, icon, origins: Vec<Origin>, size: Size, instances: u8, card: CardKind, strings, settings: Vec<Setting>, library: String, sources: Vec<Source> }`; `Setting { id, label, kind: SettingKind }` with `SettingKind::{Toggle{default}, Choice{options, default}, Choices{options, default, min, max}, Text{default, max_length, pattern}, Secret{send_to, max_length, pattern, hint}}`; `Source { id, when: When, refresh: Refresh{every,min}, requests: Vec<Step>, transform: String }`; `Step { id, for_each: Option<String>, url, headers: BTreeMap, vars: BTreeMap, max_age: Option<u64>, required: bool, limit: usize }`; `Manifest::parse(bytes) -> Result<Manifest, Error>` (strict, unknown fields rejected, every rule of spec §3 checked except jq compilation); `Manifest::settings_values(&self, saved: &serde_json::Map) -> serde_json::Map` (defaults, unknown options → default); `Manifest::access() -> Access { origins, secret_destinations }` and `Access::widens(&self, newer) -> bool`. `Document { title: Option<Text>, updated: Option<i64>, refresh: Option<u64>, empty: Option<Text>, items: Vec<Item> }`, `Item { id, card: Card, dialog: Option<Dialog> }`, `Card::{Scoreboard(..), Media(..), Ticker(..)}`, `Dialog { header, hero, tiles, lists, footer }` exactly as spec §4–5; `Text::{Plain(String), Start(String), Elapsed(String), AsOf(i64)}`; `Tone::{Neutral, Live, Gold, Up, Down}`; `Document::parse(value: &Value, card: CardKind, origins: &[Origin]) -> Result<Document, Error>` (lenient: unknown fields dropped, caps applied, foreign image URLs → None).

- [ ] Tests (fail first): esports-shaped manifest parses; rejects: id `Esports`, origin `https://a.com/path`, `http://`, IP, `localhost`, 9 origins, URL template with `{item}` in the host, `{secret.key}` in a URL, secret `send_to` not in origins, unknown field, `for_each` naming a non-`choices` setting, duplicate step ids. `settings_values` replaces a removed option with the default. `Access::widens` true when an origin is added, false when one is removed. Document: 60 items → 50; 300-char title → 200; image on another origin → None; `{"start": …}` → `Text::Start`; unknown card fields ignored; duplicate item ids → later dropped.
- [ ] Implement; `cargo test --locked -p luma-widgets`; commit "Widgets: manifests and documents".

### Task 2: jq transforms and the child-process sandbox

**Files:** Create `widgets/src/jq.rs`, `widgets/src/sandbox.rs`, `widgets/tests/jq.rs`, `widgets/tests/sandbox.rs`, `widgets/src/bin/luma-widget.rs` (only the `--widget-transform` child entry for now).
**Consumes:** Task 1 types.
**Produces:** `jq::check(program: &str) -> Result<(), Error>` (parse + compile with the restricted library; forbidden filters are simply not defined); `jq::run(program, input: Value, vars: &Vars) -> Result<Value, Error>` (exactly one output); `Vars { settings, t, lang, now, source, item: Option<Value> }`; `sandbox::Job { program, input, vars }` (serde); `sandbox::run(exe: &Path, flag: &str, job: &Job, limits: Limits) -> Result<Value, Error>` (spawn, write job JSON to stdin, read ≤ 512 KiB stdout, kill after 3 s); `sandbox::child_main() -> !` (set rlimits, read job from stdin, `jq::run`, write JSON, exit 0/1); `Limits::default()` = spec values.

- [ ] Tests: `fmt` fills placeholders; `now`, `input`, `env`, `debug` fail `check`; more than one output is an error; sandbox (using the `luma-widget` test binary via `env!("CARGO_BIN_EXE_luma-widget")`) returns a result for a normal job, and returns `Error::Limit` for `def f: f; f`, `repeat(1)` and `[range(1e9)]` within 4 s while the test process keeps running.
- [ ] Implement; run tests; commit "Widgets: jq transforms in a limited child process".

### Task 3: Catalog trust and request planning/execution

**Files:** Create `widgets/src/catalog.rs`, `widgets/src/plan.rs`, `widgets/src/fetch.rs`, `widgets/src/refresh.rs`, `widgets/tests/catalog.rs`, `widgets/tests/refresh.rs`.
**Produces:** `catalog::Index { schema, sequence, published, widgets: Vec<Entry> }`, `Entry { id, version, luma, name, description, icon, authors, origins, secrets, url, size, sha256 }`; `catalog::verify(index: &[u8], sig_b64: &str, keys: &[[u8;32]], last_sequence: u64) -> Result<Index, Error>`; `catalog::check_manifest(entry: &Entry, bytes: &[u8]) -> Result<Manifest, Error>` (size, SHA-256, same id/version/access, full validation incl. `jq::check` of library+each program); `catalog::sign(index: &[u8], secret_key) -> String`; `plan::url(template, ctx: &Placeholders) -> Result<String, Error>` (percent-encode, origin re-check), `plan::headers(..)` (secrets only to `send_to`); `fetch::Fetch` trait `fn get(&self, url: &str, headers: &[(String,String)], limit: usize) -> Result<Value, FetchError>` with `FetchError::{Refused, Busy, Unavailable(String)}`; `fetch::Http` (ureq agent: rustls, no proxy, 0 redirects, 10 s); `refresh::Cache` (by URL+headers hash, `max_age`); `refresh::run(manifest, settings, secrets, lang, now, fetch: &dyn Fetch, cache: &mut Cache, transform: &dyn Fn(Job) -> Result<Value, Error>) -> Result<(Document, String /*source id*/), WidgetError>` with `WidgetError::{NoSource{secret_label}, KeyRefused, Busy, Unavailable, Broken(String)}`.

- [ ] Tests: good signature verifies; flipped byte, other key, lower sequence fail; manifest with wrong SHA-256 or extra origin vs entry fails. Planning: `{settings.games}` list joins with `%2C`, `a b&c` encodes to `a%20b%26c`, step fans out over `games`, var giving `null` skips the step, 17 requests → `Unavailable("too many requests")`, cache reuses a reply within `max_age` and refetches after. Refresh with a fake `Fetch`: 401 on a secret-carrying step → `KeyRefused`; optional step failure → `null` reply; no source applies → `NoSource`; secret never appears in a URL or in the transform's input.
- [ ] Implement; run tests; commit "Widgets: catalog signatures and request chains".

### Task 4: `luma-widget` CLI and the catalog repo with the esports widget

**Files:** Modify `widgets/src/bin/luma-widget.rs` (`check`, `run`, `test`, `pack`, `sign`, `keygen`); create `widgets/src/fixtures.rs`; new repo `/Users/eluzive/lg/luna/luma-widgets` (`widgets/esports/{manifest.json, library.jq, shared.jq, live.jq, fixtures/…, expected/…, README.md}`, `widgets/_template/`, `README.md` (authors' guide), `.github/workflows/{check,publish}.yml`, `.github/CODEOWNERS`, `.github/pull_request_template.md`).
**Consumes:** Tasks 1–3.
**Produces:** packed `out/index.json`, `out/index.json.sig`, `out/manifests/esports-1.0.0.json`; `fixtures::Fixtures` implementing `Fetch` from `fixtures/<source>/<step>[.<item>].json`.

- [ ] Fixtures from a real `esports/csgo.json` snapshot and PandaScore-shaped running/upcoming/teams replies (synthesised from the snapshot; no key used). Expected documents written by hand from today's rendering rules, then `luma-widget test widgets/esports` passes: shared → items sorted live first, tier filter, `updated`; live → map names merged by id+position, substitutes marked, `refresh` 30 while live.
- [ ] Create GitHub repo `eluz1ve/luma-widgets` (public, Pages from Actions), environment `publish` with secret `CATALOG_SIGNING_KEY` (generated by `luma-widget keygen`, never printed; public key into Task 5), push.
- [ ] Commit in both repos ("Widgets: the authors' tool" / "Esports widget, catalog checks and publishing").

### Task 5: Luma store state: preferences, files, secrets, worker

**Files:** Create `src/widget_store.rs`, `src/widget_worker.rs`, `src/widget-tests.rs`; modify `src/model.rs` (Preferences.widgets), `src/home_grid.rs` (`BlockKind::Widget(u8)`), `src/state.rs`, `src/main.rs` (child entry first thing in `main`, flags `--widget-catalog`, `--widget-catalog-key`, `--widget-dev`), `Cargo.toml` (dep), `src/feeds.rs` (image request with an allow-list and size).
**Produces:** `WidgetPrefs { catalog_sequence: u64, installed: BTreeMap<String, Installed{version, sha256}>, slots: BTreeMap<u8, Slot{widget, settings: Map}> }`; `widget_store::Store { dir, manifests: HashMap<String, Arc<Manifest>>, secrets: Secrets }` with `load(prefs_path)`, `install(&mut self, prefs, manifest, bytes)`, `remove(&mut self, prefs, id)`, `Secrets::{get, set, forget}` (0600 file); `WidgetRequest::{Catalog, Install(Entry), Refresh{slot, generation, ..}, Image{url, origins, poster: bool}}`, `WidgetResponse::{Catalog(Result<Index,_>), Installed(Result<(Manifest, Vec<u8>),_>), Document{slot, generation, result}, Image(..)}`; `State` fields `widgets: WidgetState { store, docs: HashMap<u8, SlotDoc{doc, error, generation, next_at}>, catalog: CatalogState }`.

- [ ] Tests: prefs round-trip with a slot; `BlockKind::Widget(3)` serialises as `{"widget":3}`; install writes the file and record, remove drops blocks/slots/secrets/file; secrets file is 0600 and absent from saved preferences; a document for an old generation is ignored; an installed widget appears in `missing_blocks` until `instances` are used; daily update rule installs a non-widening update and holds a widening one.
- [ ] Implement; run tests; commit "Widgets: install, remove and refresh store widgets".

### Task 6: Templates and the dialog

**Files:** Create `src/widget_view.rs`; modify `src/view.rs` (dispatch `BlockKind::Widget`, `Overlay::WidgetDialog`; make `row_header`, `card`, `ghost`, `track_cards`, `chip`, `live_badge`, `emblem`, `fit`, `local_time`, `name_tint`, `row_position` `pub(crate)`), `src/state.rs` (`Target::WidgetItem(u8, usize)`, `Overlay::WidgetDialog{slot, id, restore}`, `row_of`, `wanted_images`), `src/design.rs`/`docs/design.md` if a token is missing.
**Produces:** `widget_view::row(out, state, assets, theme, slot, block, rect, now)`, `widget_view::dialog(out, state, assets, slot, id, now)`.

- [ ] Tests: a scoreboard document renders one control per visible card with `Target::WidgetItem`, Right steps through `row_of`, OK opens the dialog by id and it closes when the id disappears; media and ticker render with `WIDE_W`/`TILE_W`; ghost messages for loading/key refused/no source/empty; Text `{start}` / `{elapsed}` / `{as_of}` format like `local_time` / "12 min" / "Data as of".
- [ ] Implement by porting `match_card`/`match_dialog`/premieres card onto document fields; commit "Widgets: scoreboard, media and ticker rows and the dialog".

### Task 7: Store UI, settings pages and secret entry

**Files:** Modify `src/state.rs` (`Target::{StoreEntry(usize), StoreAction(usize), WidgetSetting(usize), WidgetOption(usize)}`, `Overlay::{Store{restore}, StoreEntry{index, restore}}`, `Subpage::{Widget(u8), WidgetOptions(u8, usize)}`, `TextField::{Secret, WidgetText}`, generic phone key), `src/view.rs` / `src/widget_view.rs` (store list, entry page, settings page), `src/phone_key.rs` (`PageTexts` from the setting, validation by `max_length`/`pattern`), `assets/messages.json` (7 languages), `src/main.rs` (catalog fetch on store open and daily, installs), `.changeset/*.md`.

- [ ] Tests: "Widget store…" is the last Add widget row; store shows loading → list from a fake index; entry shows origins and the secret's destination; Install adds the block and focuses it; settings page rows follow the manifest; toggling saves; choices checklist keeps ≥ `min`; typing a secret stores it in `Secrets` not prefs; phone page rejects a value failing `pattern`; Remove deletes everything.
- [ ] Implement; `cargo test --locked -p luma -p luma-widgets`, `pnpm check`; commit "Widgets: the store, widget settings and secrets".

### Task 8: Desktop proof and publishing

- [ ] Merge the esports widget to `luma-widgets` main → publish workflow signs and deploys; verify `index.json.sig` against the compiled key with `luma-widget`.
- [ ] Desktop screenshots (scratch preferences, `--time` fixed): built-in esports row and dialog vs the store-installed one from the shared feed; then live with a key the user supplies at run time (phone page), scratch directory deleted afterwards.
- [ ] Push `feature/widget-store` to `fork`; build the ARM IPK for a TV check.
