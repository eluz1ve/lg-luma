//! The Top Picks widget's feed: shows streaming this week, from TVmaze's public API (no key).
//! Network/TLS and JSON parsing happen on a worker thread, never on the SDL thread.
use crate::model::RecType;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single trending title shown as a poster card.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Default)]
pub struct Recommendation {
    /// TMDB id, used to fetch details (genre, seasons/runtime).
    pub id: i64,
    /// Display title (movie `title` or show `name`).
    pub title: String,
    /// TMDB poster path, e.g. `/abc123.jpg`; combine with the image base to get a URL.
    pub poster_path: String,
    /// `true` for a TV show, `false` for a movie (used for the badge and, later, deep-linking).
    pub is_show: bool,
    /// Release/first-air year (from the trending feed), empty when unknown.
    pub year: String,
    /// Primary genre name (from the detail request), empty until detailed.
    pub genre: String,
    /// Seasons ("2 seasons") for shows or runtime ("1h 30m") for movies; empty until detailed.
    pub detail: String,
    /// True when it carries TMDB's Animation genre (id 16) — i.e. cartoons/anime.
    pub is_anime: bool,
    /// Streaming service display names where it's available (flatrate), best first, e.g. ["Disney+"].
    /// The app to launch is resolved against the TV's installed apps at click time.
    pub services: Vec<String>,
    /// The installed app id to launch, resolved at fetch time against the TV's apps (Free My TV's
    /// approach). Empty when no installed app provides it — such items are dropped from the feed.
    pub app_id: String,
    /// Display name of the resolved installed service (e.g. "Netflix"), for the details line.
    pub service_name: String,
    /// Whether the detail request has already filled genre/detail.
    pub detailed: bool,
}

/// A TV app as the worker needs it: just the id and title used to match streaming services.
/// (The full `model::App` isn't available on the worker thread, so we pass this lean copy.)
#[derive(Clone, Debug)]
pub struct InstalledApp {
    pub id: String,
    pub title: String,
}

/// Maps a TMDB provider name (lowercased substring match) to a display name and the webOS app id
/// A streaming service: how its TMDB provider name is detected, its display name, and substrings
/// to match against installed apps (by id or title) so we launch whatever the TV actually has.
/// Mirrors Free My TV's SERVICES table.
pub struct StreamingService {
    /// Lowercased substring found in the TMDB provider_name.
    pub tmdb: &'static str,
    /// Display name shown in the details line.
    pub name: &'static str,
    /// Lowercased substrings to match an installed app's id or title.
    pub app_match: &'static [&'static str],
}

pub const SERVICES: &[StreamingService] = &[
    StreamingService {
        tmdb: "netflix",
        name: "Netflix",
        app_match: &["netflix"],
    },
    StreamingService {
        tmdb: "disney",
        name: "Disney+",
        app_match: &["disney"],
    },
    StreamingService {
        tmdb: "star+",
        name: "Star+",
        app_match: &["disney", "star"],
    },
    StreamingService {
        tmdb: "amazon prime",
        name: "Prime Video",
        app_match: &["amazon", "prime"],
    },
    StreamingService {
        tmdb: "prime video",
        name: "Prime Video",
        app_match: &["amazon", "prime"],
    },
    StreamingService {
        tmdb: "globoplay",
        name: "Globoplay",
        app_match: &["globoplay", "globo"],
    },
    StreamingService {
        tmdb: "max",
        name: "Max",
        app_match: &["wbd", "hbo", ".max", "max"],
    },
    StreamingService {
        tmdb: "apple tv",
        name: "Apple TV",
        app_match: &["apple", "appletv"],
    },
    StreamingService {
        tmdb: "paramount",
        name: "Paramount+",
        app_match: &["paramount", "cbs"],
    },
    StreamingService {
        tmdb: "crunchyroll",
        name: "Crunchyroll",
        app_match: &["crunchyroll"],
    },
    StreamingService {
        tmdb: "telecine",
        name: "Telecine",
        app_match: &["telecine"],
    },
    StreamingService {
        tmdb: "youtube",
        name: "YouTube",
        app_match: &["youtube"],
    },
];

/// The service whose TMDB name matches `provider_name`, if any.
pub fn service_for(provider_name: &str) -> Option<&'static StreamingService> {
    let n = provider_name.to_lowercase();
    if n.contains(" channel") {
        return None; // "X Amazon Channel" is sold inside another app
    }
    SERVICES.iter().find(|s| {
        if s.tmdb == "max" {
            // avoid matching "cinemax" etc.; word-ish match
            n == "max" || n.contains("hbo max") || n.contains(" max")
        } else {
            n.contains(s.tmdb)
        }
    })
}

/// Installed apps that share a brand substring with a streaming service but are NOT the player
/// (voice assistants, stores, companions). Matching these would launch the wrong app — e.g.
/// "amazon" must resolve to Prime Video (id `amazon`), never `amazon.alexa.view` (Alexa).
const NOT_PLAYERS: &[&str] = &["alexa", "assistant", "store", "companion", ".demo", "setup"];

/// The installed app that provides `service`, matched by id or title (Free My TV's `appFor`).
pub fn app_for<'a>(
    service: &StreamingService,
    installed: &'a [InstalledApp],
) -> Option<&'a InstalledApp> {
    installed.iter().find(|app| {
        let id = app.id.to_lowercase();
        let title = app.title.to_lowercase();
        // Never resolve to a non-player app that merely shares a brand word (e.g. Alexa).
        if NOT_PLAYERS
            .iter()
            .any(|bad| id.contains(bad) || title.contains(bad))
        {
            return false;
        }
        service
            .app_match
            .iter()
            .any(|m| id.contains(m) || title.contains(m))
    })
}

impl Recommendation {
    /// The poster's URL: TVmaze's image CDN only.
    pub fn poster_url(&self) -> Option<&str> {
        self.poster_path
            .starts_with(POSTER_ORIGIN)
            .then_some(self.poster_path.as_str())
    }

    /// The on-disk cache path for this poster under `cache_dir`, named after its path on the CDN.
    pub fn poster_file(&self, cache_dir: &std::path::Path) -> Option<std::path::PathBuf> {
        let name: String = self
            .poster_url()?
            .trim_start_matches(POSTER_ORIGIN)
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        (!name.is_empty()).then(|| cache_dir.join(name))
    }
}

/// TVmaze's image CDN; posters come from nowhere else.
const POSTER_ORIGIN: &str = "https://static.tvmaze.com/uploads/images/";
/// Days of the streaming schedule a refresh reads.
const SCHEDULE_DAYS: i64 = 7;

/// A show from a TVmaze schedule entry, with its popularity (`weight`, 0–100) for sorting.
pub fn parse_show(show: &Value, installed: &[InstalledApp]) -> Option<(i64, Recommendation)> {
    let id = show.get("id")?.as_i64()?;
    let title: String = show
        .get("name")?
        .as_str()?
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(120)
        .collect();
    let poster_path = show
        .pointer("/image/medium")
        .and_then(Value::as_str)
        .filter(|url| url.starts_with(POSTER_ORIGIN))?
        .to_owned();
    if title.is_empty() {
        return None;
    }
    let genres: Vec<&str> = show
        .get("genres")
        .and_then(Value::as_array)
        .map(|g| g.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let channel = show
        .pointer("/webChannel/name")
        .or_else(|| show.pointer("/network/name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut item = Recommendation {
        id,
        title,
        poster_path,
        is_show: true,
        year: show
            .get("premiered")
            .and_then(Value::as_str)
            .map(|d| d.chars().take(4).collect::<String>())
            .filter(|y| y.len() == 4)
            .unwrap_or_default(),
        genre: genres.first().map(|g| g.to_string()).unwrap_or_default(),
        detail: show
            .pointer("/rating/average")
            .and_then(Value::as_f64)
            .map(|r| format!("★ {r:.1}"))
            .unwrap_or_default(),
        is_anime: show.get("type").and_then(Value::as_str) == Some("Animation")
            || genres.contains(&"Anime"),
        detailed: true,
        ..Default::default()
    };
    if let Some(service) = service_for(channel) {
        item.services.push(service.name.to_owned());
        if let Some(app) = app_for(service, installed) {
            item.app_id = app.id.clone();
            item.service_name = service.name.to_owned();
        }
    }
    let weight = show.get("weight").and_then(Value::as_i64).unwrap_or(0);
    Some((weight, item))
}

/// The shows of TVmaze schedule replies (`/schedule/web` embeds them, `/schedule` nests them),
/// most popular first, without repeats.
pub fn parse_schedules(replies: &[Value], installed: &[InstalledApp]) -> Vec<Recommendation> {
    let mut shows: Vec<(i64, Recommendation)> = Vec::new();
    for entry in replies.iter().filter_map(Value::as_array).flatten() {
        let Some(show) = entry
            .pointer("/_embedded/show")
            .or_else(|| entry.get("show"))
        else {
            continue;
        };
        if let Some((weight, item)) = parse_show(show, installed)
            && !shows.iter().any(|(_, seen)| seen.id == item.id)
        {
            shows.push((weight, item));
        }
    }
    // Stable, so equally popular shows keep their schedule order.
    shows.sort_by_key(|(weight, _)| std::cmp::Reverse(*weight));
    shows.into_iter().map(|(_, item)| item).collect()
}

const LIMIT: usize = 2 * 1024 * 1024;

/// TVmaze client. One per worker; cloning shares the underlying ureq agent config cheaply.
#[derive(Clone)]
pub struct Client {
    agent: ureq::Agent,
    base: String,
}

impl Client {
    pub fn new(test_server: Option<&str>) -> Self {
        let base = match test_server {
            Some(server) => server.trim_end_matches('/').to_string(),
            None => "https://api.tvmaze.com".to_string(),
        };
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(8)))
            .max_redirects(0)
            .proxy(None)
            .build()
            .new_agent();
        Self { agent, base }
    }

    fn request(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, String> {
        use std::io::Read;
        let url = format!("{}{path}", self.base);
        let mut request = self
            .agent
            .get(&url)
            .header("Accept", "application/json")
            .header("User-Agent", "LumaNative");
        for (key, value) in query {
            request = request.query(key, value);
        }
        let mut response = request
            .call()
            .map_err(|_| "TVmaze is unavailable. Check the network.".to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "TVmaze returned status {}",
                response.status().as_u16()
            ));
        }
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "The TVmaze response could not be read".to_string())?;
        if bytes.len() > LIMIT {
            return Err("The TVmaze response is too large".into());
        }
        serde_json::from_slice(&bytes).map_err(|_| "TVmaze returned unreadable data".into())
    }

    /// The shows streaming in the week up to today (page 1) or a week earlier per further page,
    /// from TVmaze's streaming schedule, plus today's TV schedule in `region`. Only shows an
    /// installed app plays are kept, most popular first. At most `limit`.
    pub fn trending(
        &self,
        region: &str,
        installed: &[InstalledApp],
        page: u32,
        limit: usize,
    ) -> Result<Vec<Recommendation>, String> {
        if installed.is_empty() {
            return Err("No streaming apps are installed to recommend from.".into());
        }
        let today = time::OffsetDateTime::now_utc().date();
        let last = today - time::Duration::days(SCHEDULE_DAYS * (page.max(1) as i64 - 1));
        let mut replies = Vec::new();
        let mut error = None;
        for day in 0..SCHEDULE_DAYS {
            let date = (last - time::Duration::days(day)).to_string();
            match self.request("/schedule/web", &[("date", &date)]) {
                Ok(value) => replies.push(value),
                Err(message) => error = Some(message),
            }
        }
        if page <= 1
            && let Ok(value) = self.request(
                "/schedule",
                &[("country", region), ("date", &today.to_string())],
            )
        {
            replies.push(value);
        }
        if replies.is_empty() {
            return Err(error.unwrap_or_else(|| "TVmaze is unavailable.".into()));
        }
        let mut shows: Vec<Recommendation> = parse_schedules(&replies, installed)
            .into_iter()
            .filter(|item| !item.app_id.is_empty())
            .collect();
        if shows.is_empty() {
            return Err("No shows from your streaming apps this week.".into());
        }
        shows.truncate(limit);
        Ok(shows)
    }
}

/// The cached list plus when it was fetched, persisted so the widget shows something at once.
/// Mirrors Free My TV: the full list is fetched at most once per interval, and the visible row
/// rotates through that cached list on each interval tick — the API is not hit on every rotation.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Feed {
    pub items: Vec<Recommendation>,
    /// Unix seconds of the last successful fetch.
    pub fetched_at: i64,
    /// The type the cached list was fetched for (a change forces a refetch).
    pub kind: Option<RecType>,
    /// Manual page advance added by the Refresh card (see `visible`).
    #[serde(default)]
    pub extra_shift: usize,
}

impl Feed {
    /// True when the cache should be refetched: empty, older than `interval_seconds`, or a different type.
    pub fn is_stale(&self, now: i64, interval_seconds: i64, kind: RecType) -> bool {
        self.items.is_empty()
            || self.kind != Some(kind)
            || now < self.fetched_at
            || now - self.fetched_at >= interval_seconds
    }

    /// The `count` recommendations to show now, rotating through the cached list by the interval tick,
    /// so the selection changes every `interval_seconds` without any network call (Free My TV's approach).
    pub fn visible(&self, now: i64, interval_seconds: i64, count: usize) -> Vec<Recommendation> {
        let len = self.items.len();
        if len == 0 || count == 0 {
            return Vec::new();
        }
        let interval = interval_seconds.max(1);
        let tick = now.div_euclid(interval).max(0) as usize;
        let take = count.min(len);
        // The temporal rotation advances by whole pages of `take` as time ticks.
        // `extra_shift` (the Refresh button) advances by a fixed odd stride so the shown set
        // actually changes even when the view requests the whole pool (take == len): a stride of
        // len would be a no-op mod len, so use a stride coprime-ish to len.
        let refresh_stride = (len / 3).max(1);
        let start = (tick
            .wrapping_mul(take)
            .wrapping_add(self.extra_shift.wrapping_mul(refresh_stride)))
            % len;
        (0..take)
            .map(|i| self.items[(start + i) % len].clone())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Worker: fetches the list and downloads+decodes posters off the SDL thread.
// Mirrors weather-service's shape (one Service, a receiver the UI drains per frame),
// but a single latest-wins job: a new request supersedes an in-flight one.
// ---------------------------------------------------------------------------
use std::path::Path;
use std::path::PathBuf;
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use std::thread;

const POSTER_LIMIT: usize = 2 * 1024 * 1024;
/// Max posters fetched per refresh (keeps a row reasonable and bounds network/memory).
/// Also the upper bound for the rotating window the UI shows (see `view`/`state`/`accept`).
pub const MAX_ITEMS: usize = 24;

pub struct Request {
    pub id: u64,
    pub region: String,
    /// Directory where downloaded posters are cached on disk.
    pub cache_dir: PathBuf,
    /// The TV's installed apps (id/title), used to filter and resolve launchable titles.
    pub installed: Vec<InstalledApp>,
    /// Leave cartoons/anime (TMDB's Animation genre) out of the feed.
    pub hide_animation: bool,
    /// TMDB page to fetch (1-based), bumped by the Refresh card for a fresh batch of titles.
    pub page: u32,
}

pub enum Response {
    /// The whole refresh finished: the cached list and the posters that decoded.
    Loaded {
        id: u64,
        result: Result<(Vec<Recommendation>, usize), String>,
    },
}

#[derive(Default)]
struct Pending {
    job: Option<Request>,
    closed: bool,
}

pub struct Service {
    pending: Arc<(Mutex<Pending>, Condvar)>,
    generation: Arc<AtomicU64>,
    pub receiver: mpsc::Receiver<Response>,
}

impl Service {
    pub fn new(client: Client) -> Self {
        let (sender, receiver) = mpsc::channel();
        let pending: Arc<(Mutex<Pending>, Condvar)> =
            Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let generation = Arc::new(AtomicU64::new(0));
        let worker_pending = pending.clone();
        let worker_generation = generation.clone();
        thread::spawn(move || {
            loop {
                let job = {
                    let (mutex, ready) = &*worker_pending;
                    let mut guard = mutex.lock().unwrap();
                    while guard.job.is_none() && !guard.closed {
                        guard = ready.wait(guard).unwrap();
                    }
                    if guard.closed {
                        break;
                    }
                    guard.job.take().unwrap()
                };
                // Skip stale jobs (a newer request already arrived).
                if worker_generation.load(Ordering::SeqCst) != job.id {
                    continue;
                }
                let result = load(&client, &job, &worker_generation);
                if worker_generation.load(Ordering::SeqCst) == job.id
                    && sender
                        .send(Response::Loaded { id: job.id, result })
                        .is_err()
                {
                    break;
                }
            }
        });
        Self {
            pending,
            generation,
            receiver,
        }
    }

    /// Request a refresh; the id lets the UI ignore responses older than the latest ask.
    pub fn send(&self, request: Request) {
        self.generation.store(request.id, Ordering::SeqCst);
        let (mutex, ready) = &*self.pending;
        mutex.lock().unwrap().job = Some(request);
        ready.notify_one();
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        let (mutex, ready) = &*self.pending;
        let mut guard = mutex.lock().unwrap();
        guard.closed = true;
        guard.job = None;
        ready.notify_one();
    }
}

/// Fetch the recommendations list, fill any missing details, drop titles with no installed app,
/// then download each poster. Bails early if superseded. Mirrors Free My TV: every kept title is
/// watchable on an app the TV has, so clicking a card always opens something.
fn load(
    client: &Client,
    job: &Request,
    generation: &AtomicU64,
) -> Result<(Vec<Recommendation>, usize), String> {
    let items = client.trending(&job.region, &job.installed, job.page, MAX_ITEMS)?;
    let mut kept = Vec::new();
    let mut saved = 0usize;
    for item in items {
        if generation.load(Ordering::SeqCst) != job.id {
            break; // a newer request arrived; stop wasting bandwidth
        }
        // Only keep titles an installed app can actually play.
        if item.app_id.is_empty() {
            continue;
        }
        // Optionally leave cartoons/anime out.
        if job.hide_animation && item.is_anime {
            continue;
        }
        if download_poster(client, &item, &job.cache_dir).is_some() {
            saved += 1;
        }
        kept.push(item);
    }
    Ok((kept, saved))
}

/// Download one poster from TVmaze's image CDN and save it under `cache_dir`.
/// Returns the saved path (the renderer decodes and caches it as a texture when drawing).
fn download_poster(client: &Client, item: &Recommendation, cache_dir: &Path) -> Option<PathBuf> {
    use std::io::Read;
    let path = item.poster_file(cache_dir)?;
    // Reuse a cached file if it's already here.
    if path.is_file() {
        return Some(path);
    }
    let url = item.poster_url()?;
    let mut response = client.agent.get(url).call().ok()?;
    if !response.status().is_success() {
        return None;
    }
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(POSTER_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.is_empty() || bytes.len() > POSTER_LIMIT {
        return None;
    }
    // Sanity check it's a decodable image before trusting it on disk.
    if image::guess_format(&bytes).is_err() {
        return None;
    }
    std::fs::create_dir_all(cache_dir).ok()?;
    std::fs::write(&path, &bytes).ok()?;
    Some(path)
}

// ---------------------------------------------------------------------------
// UI-side state (lives in State; testable, holds no GL textures). Tracks the cached
// feed and decides when to ask the worker for a refresh, mirroring Free My TV's timing.
// ---------------------------------------------------------------------------

/// The widget's logical state on the UI thread.
#[derive(Default)]
pub struct Recommendations {
    /// Cached list + when it was fetched (persisted via storage, like the weather cache).
    pub feed: Feed,
    /// Monotonic request id so the worker's responses can be matched to the latest ask.
    pub request_id: u64,
    /// True while a refresh is in flight (avoids piling up requests).
    pub loading: bool,
    /// Last error text from the worker, shown in place of the row when there are no items.
    pub error: Option<String>,
    /// Focused card index within the visible row.
    pub focus: usize,
    /// Index of the first card currently shown (left edge of the visible window). The row only
    /// slides when focus would leave this window, so going back moves the cursor with a still row
    /// until it reaches the first visible card (Netflix-style).
    pub window_start: usize,
    /// Where posters are cached on disk (set by the tick; used by the view to find files).
    pub cache_dir: PathBuf,
    /// Number of items in the feed (= index of the refresh card), for carousel navigation bounds.
    pub visible_len: usize,
    /// TMDB page to fetch (1-based). The Refresh card bumps this and forces a refetch, so each
    /// refresh pulls a different batch of titles from the API instead of reshuffling the same list.
    pub page: u32,
}

impl Recommendations {
    /// Decide whether to ask the worker to refresh. Returns a `Request` to send, or None.
    /// `now` is Unix seconds.
    pub fn sync(
        &mut self,
        prefs: &crate::model::Preferences,
        region: &str,
        cache_dir: &Path,
        installed: &[InstalledApp],
        now: i64,
    ) -> Option<Request> {
        if !prefs.show_recommendations || self.loading {
            return None;
        }
        if installed.is_empty() {
            return None; // nothing to recommend from yet (apps not loaded)
        }
        let interval = prefs.rec_interval.seconds();
        if !self.feed.is_stale(now, interval, prefs.rec_type) {
            return None;
        }
        self.request_id += 1;
        self.loading = true;
        Some(Request {
            id: self.request_id,
            region: region.to_string(),
            cache_dir: cache_dir.to_path_buf(),
            installed: installed.to_vec(),
            hide_animation: prefs.rec_hide_animation,
            page: self.page.max(1),
        })
    }

    /// Apply a finished refresh (newest request only). Returns the posters to upload as textures.
    pub fn accept(
        &mut self,
        id: u64,
        result: Result<(Vec<Recommendation>, usize), String>,
        now: i64,
        kind: crate::model::RecType,
    ) -> bool {
        if id != self.request_id {
            return false; // a newer request superseded this one
        }
        self.loading = false;
        match result {
            Ok((items, _saved)) => {
                self.visible_len = items.len().min(MAX_ITEMS);
                self.feed = Feed {
                    items,
                    fetched_at: now,
                    kind: Some(kind),
                    extra_shift: 0,
                };
                self.error = None;
                self.focus = 0;
                self.window_start = 0;
                true
            }
            Err(message) => {
                self.error = Some(message);
                false
            }
        }
    }

    /// The recommendations to show right now (rotated by the interval tick).
    pub fn visible(&self, now: i64, interval_seconds: i64, count: usize) -> Vec<Recommendation> {
        self.feed.visible(now, interval_seconds, count)
    }

    /// Load a previously saved feed from disk so the widget shows titles immediately at startup,
    /// without a network call, until the interval expires or the user hits Refresh.
    pub fn load_cache(&mut self, path: &Path) {
        if let Some(cache) = load_feed_cache(path) {
            self.visible_len = cache.feed.items.len().min(MAX_ITEMS);
            self.feed = cache.feed;
            self.page = cache.page.max(1);
        }
    }

    /// Persist the current feed (and page) to disk after a successful refresh.
    pub fn save_cache(&self, path: &Path) {
        let cache = FeedCache {
            feed: self.feed.clone(),
            page: self.page.max(1),
        };
        let _ = save_feed_cache(path, &cache);
    }
}

/// The feed plus the TMDB page it was fetched for, persisted together so a restart restores both.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct FeedCache {
    pub feed: Feed,
    #[serde(default)]
    pub page: u32,
}

/// Where the feed cache lives, next to the preferences file (mirrors the weather cache).
pub fn cache_path(preferences: &Path) -> PathBuf {
    preferences.with_extension("recommendations.json")
}

/// Read the saved feed cache, bounded like the weather cache. None on any problem.
pub fn load_feed_cache(path: &Path) -> Option<FeedCache> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(512 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 512 * 1024 {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

/// Atomically write the feed cache to disk.
pub fn save_feed_cache(path: &Path, cache: &FeedCache) -> Result<(), String> {
    crate::storage::write_atomic(path, &serde_json::to_vec(cache).map_err(|e| e.to_string())?)
}

#[cfg(test)]
mod feed_tests {
    use super::*;
    use crate::model::RecType;

    fn rec(n: usize) -> Recommendation {
        Recommendation {
            title: format!("t{n}"),
            poster_path: format!("/p{n}.jpg"),
            ..Default::default()
        }
    }

    #[test]
    fn tvmaze_schedules_become_shows_from_installed_apps_most_popular_first() {
        use serde_json::json;
        let show = |id: i64, name: &str, weight: i64, channel: &str| {
            json!({
                "id": id, "name": name, "weight": weight, "type": "Scripted",
                "genres": ["Drama", "Thriller"], "premiered": "2024-03-01",
                "rating": {"average": 8.14},
                "image": {"medium": format!("https://static.tvmaze.com/uploads/images/medium_portrait/1/{id}.jpg")},
                "webChannel": {"name": channel}
            })
        };
        let web = json!([
            {"_embedded": {"show": show(1, "Quiet", 40, "Netflix")}},
            {"_embedded": {"show": show(2, "Loud", 90, "Netflix")}},
            {"_embedded": {"show": show(2, "Loud", 90, "Netflix")}},
            {"_embedded": {"show": show(3, "Elsewhere", 99, "Some Channel")}},
            {"_embedded": {"show": {"id": 4, "name": "No poster", "image": null}}}
        ]);
        let tv = json!([{"show": show(5, "Broadcast", 60, "Disney+")}]);
        let installed = [InstalledApp {
            id: "netflix".into(),
            title: "Netflix".into(),
        }];
        let shows = parse_schedules(&[web, tv], &installed);
        let titles: Vec<&str> = shows.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Elsewhere", "Loud", "Broadcast", "Quiet"]);
        let loud = &shows[1];
        assert_eq!(
            (loud.app_id.as_str(), loud.service_name.as_str()),
            ("netflix", "Netflix")
        );
        assert_eq!((loud.year.as_str(), loud.genre.as_str()), ("2024", "Drama"));
        assert_eq!(loud.detail, "★ 8.1");
        assert!(shows[0].app_id.is_empty() && shows[2].app_id.is_empty());
        assert!(
            loud.poster_file(std::path::Path::new("/cache"))
                .is_some_and(|p| p.ends_with("medium_portrait_1_2.jpg"))
        );
        let elsewhere = Recommendation {
            poster_path: "https://example.com/a.jpg".into(),
            ..Default::default()
        };
        assert!(elsewhere.poster_url().is_none());
    }

    #[test]
    fn stale_when_empty_old_or_type_changed() {
        let feed = Feed {
            items: vec![rec(1)],
            fetched_at: 1000,
            kind: Some(RecType::Both),
            extra_shift: 0,
        };
        assert!(
            !feed.is_stale(1000 + 3599, 3600, RecType::Both),
            "fresh dentro do intervalo"
        );
        assert!(
            feed.is_stale(1000 + 3600, 3600, RecType::Both),
            "stale no intervalo"
        );
        assert!(
            feed.is_stale(1500, 3600, RecType::Movies),
            "stale se mudou o tipo"
        );
        let empty = Feed::default();
        assert!(empty.is_stale(0, 3600, RecType::Both), "vazio sempre stale");
    }

    #[test]
    fn visible_rotates_by_tick_without_repeating_within_a_window() {
        let feed = Feed {
            items: (0..10).map(rec).collect(),
            fetched_at: 0,
            kind: Some(RecType::Both),
            extra_shift: 0,
        };
        let a = feed.visible(0, 3600, 3);
        let b = feed.visible(3600, 3600, 3); // proximo tick
        assert_eq!(a.len(), 3);
        assert_ne!(a, b, "a selecao muda a cada intervalo");
        // dentro do mesmo tick, estavel
        assert_eq!(feed.visible(100, 3600, 3), feed.visible(3599, 3600, 3));
    }

    #[test]
    fn refresh_extra_shift_changes_the_set_even_for_the_whole_pool() {
        // Regressão: antes, pedir o pool inteiro (take == len) zerava o deslocamento do refresh
        // (x * len % len == 0), então o botão Atualizar não trocava nada.
        let base = Feed {
            items: (0..24).map(rec).collect(),
            fetched_at: 0,
            kind: Some(RecType::Both),
            extra_shift: 0,
        };
        let shifted = Feed {
            extra_shift: 1,
            ..Feed {
                items: (0..24).map(rec).collect(),
                fetched_at: 0,
                kind: Some(RecType::Both),
                extra_shift: 0,
            }
        };
        // Mesmo pedindo todos os 24, a ordem (primeiros itens) muda com o refresh.
        let a = base.visible(0, 3600, 24);
        let b = shifted.visible(0, 3600, 24);
        assert_eq!(a.len(), 24);
        assert_eq!(b.len(), 24);
        assert_ne!(a[0], b[0], "o refresh precisa trocar o conjunto visível");
    }
}
