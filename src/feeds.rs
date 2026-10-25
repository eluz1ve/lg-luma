//! Data for Home widgets, fetched on a worker thread: Binance 24 h mini tickers for crypto prices,
//! TVmaze season premieres and their posters, and Counter-Strike matches from PandaScore with the
//! user's free key (lists of running and upcoming matches, and team rosters). As for weather: fixed HTTPS origins, no proxy or
//! redirects, bounded replies and timeouts; nothing runs on the SDL thread.
use crate::app_artwork::Artwork;
use serde_json::Value;
use std::{
    io::Read,
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

/// Coins shown left to right; a wider widget shows more of them.
pub const COINS: [&str; 6] = ["BTC", "ETH", "SOL", "BNB", "XRP", "DOGE"];
const PRICE_HOSTS: [&str; 2] = ["https://api.binance.com", "https://api.binance.us"];
const SCHEDULES: [&str; 2] = [
    "https://api.tvmaze.com/schedule/web",
    "https://api.tvmaze.com/schedule",
];
const POSTER_ORIGIN: &str = "https://static.tvmaze.com/";
const PANDASCORE: &str = "https://api.pandascore.co";
/// The shared feed for widgets without a personal key, rebuilt every 15 minutes.
const FEED: &str = "https://eluz1ve.github.io/luma-feeds/esports";

/// The shared feed's file for a discipline.
pub fn feed_url(game: &str) -> String {
    format!("{FEED}/{}.json", game_path(game))
}

/// Seconds between refreshes with a personal key while a match is live: two lists per
/// discipline, kept under the free plan's 1000 requests an hour, and never under 30 s.
pub fn live_interval(games: usize) -> u64 {
    (games as u64 * 8).max(30)
}
/// PandaScore's path for each discipline and its name; the free key covers all of them.
pub const GAMES: [(&str, &str); 8] = [
    ("csgo", "Counter-Strike 2"),
    ("dota2", "Dota 2"),
    ("lol", "League of Legends"),
    ("valorant", "Valorant"),
    ("r6siege", "Rainbow Six Siege"),
    ("ow", "Overwatch 2"),
    ("rl", "Rocket League"),
    ("codmw", "Call of Duty"),
];
/// The discipline's path, Counter-Strike for anything unknown.
pub fn game_path(game: &str) -> &'static str {
    GAMES
        .iter()
        .find(|(path, _)| *path == game)
        .map_or("csgo", |(path, _)| path)
}
/// Kept per discipline at most.
const MATCHES: usize = 20;
const JSON_LIMIT: usize = 1024 * 1024;
const IMAGE_LIMIT: usize = 2 * 1024 * 1024;
/// Popular shows only: TVmaze's weight is its 0–100 popularity.
const MIN_WEIGHT: u64 = 95;
const PREMIERES: usize = 12;

#[derive(Clone, Debug, PartialEq)]
pub struct Quote {
    pub coin: &'static str,
    /// In US dollars (USDT pairs).
    pub last: f64,
    /// Percent over 24 hours.
    pub change: f64,
    pub low: f64,
    pub high: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Premiere {
    pub title: String,
    pub season: u64,
    /// `YYYY-MM-DD`.
    pub date: String,
    pub poster: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Team {
    pub id: u64,
    pub name: String,
    /// The acronym when PandaScore has one, else the name.
    pub short: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameState {
    Waiting,
    Running,
    Finished,
}

/// One map of a series; PandaScore's free lists carry its state and winner, not the map's name.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchGame {
    pub position: u64,
    pub state: GameState,
    pub winner: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CsMatch {
    pub id: u64,
    /// PandaScore's path of the discipline, e.g. `csgo`.
    pub game: String,
    pub running: bool,
    /// RFC 3339, UTC.
    pub begin_at: String,
    /// A team still to be decided is `None`.
    pub teams: [Option<Team>; 2],
    /// Maps won, in the teams' order.
    pub score: [u64; 2],
    pub best_of: u64,
    pub games: Vec<MatchGame>,
    /// League and series, e.g. "BLAST Premier Fall 2026".
    pub event: String,
    pub tier: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchesError {
    /// PandaScore refused the key.
    Key,
    Unavailable,
}

/// What Home's widgets show; filled from `FeedResponse`s on the SDL thread.
#[derive(Default)]
pub struct FeedData {
    pub quotes: Vec<Quote>,
    pub premieres: Vec<Premiere>,
    pub posters: std::collections::HashMap<String, Arc<Artwork>>,
    pub crypto_failed: bool,
    pub premieres_failed: bool,
    /// Running and upcoming matches by discipline; a discipline is missing until it arrives.
    pub matches: std::collections::HashMap<String, Vec<CsMatch>>,
    pub matches_error: Option<MatchesError>,
    /// Player nicknames by team id.
    pub rosters: std::collections::HashMap<u64, Vec<String>>,
    /// Lineups a match dialog asked for, sent to the worker by the main loop.
    pub pending_rosters: Vec<(String, Vec<u64>)>,
}

pub enum FeedRequest {
    Crypto,
    Premieres,
    Poster(String),
    Matches {
        key: String,
        game: String,
    },
    Rosters {
        key: String,
        game: String,
        teams: Vec<u64>,
    },
    /// A discipline from the shared feed, lineups included.
    Shared {
        game: String,
    },
}

pub enum FeedResponse {
    Crypto(Result<Vec<Quote>, String>),
    Premieres(Result<Vec<Premiere>, String>),
    Poster(String, Option<Arc<Artwork>>),
    Matches {
        game: String,
        result: Result<Vec<CsMatch>, MatchesError>,
    },
    Rosters(Vec<(u64, Vec<String>)>),
    Shared {
        game: String,
        result: Result<(Vec<CsMatch>, Vec<(u64, Vec<String>)>), MatchesError>,
    },
}

pub struct Feeds {
    sender: mpsc::Sender<FeedRequest>,
    pub receiver: mpsc::Receiver<FeedResponse>,
}

impl Feeds {
    pub fn new() -> Self {
        let (sender, requests) = mpsc::channel();
        let (responses, receiver) = mpsc::channel();
        thread::spawn(move || {
            let agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(10)))
                .max_redirects(0)
                .proxy(None)
                .build()
                .new_agent();
            while let Ok(request) = requests.recv() {
                let response = match request {
                    FeedRequest::Crypto => FeedResponse::Crypto(quotes(&agent)),
                    FeedRequest::Premieres => FeedResponse::Premieres(premieres(&agent)),
                    FeedRequest::Poster(url) => {
                        let image = poster(&agent, &url);
                        FeedResponse::Poster(url, image)
                    }
                    FeedRequest::Matches { key, game } => FeedResponse::Matches {
                        result: matches(&agent, &key, &game),
                        game,
                    },
                    FeedRequest::Rosters { key, game, teams } => {
                        FeedResponse::Rosters(rosters(&agent, &key, &game, &teams))
                    }
                    FeedRequest::Shared { game } => FeedResponse::Shared {
                        result: get_json(&agent, &feed_url(&game))
                            .map(|body| parse_feed(&body, &game))
                            .map_err(|error| {
                                eprintln!("Luma esports feed: {error}");
                                MatchesError::Unavailable
                            }),
                        game,
                    },
                };
                if responses.send(response).is_err() {
                    break;
                }
            }
        });
        Self { sender, receiver }
    }

    pub fn send(&self, request: FeedRequest) {
        let _ = self.sender.send(request);
    }
}

fn get(agent: &ureq::Agent, url: &str, limit: usize) -> Result<Vec<u8>, String> {
    let mut response = agent
        .get(url)
        .header("User-Agent", "LumaNative")
        .call()
        .map_err(|e| format!("{url}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("{url}: HTTP {}", response.status()));
    }
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{url}: {e}"))?;
    if bytes.len() > limit {
        return Err(format!("{url}: reply too large"));
    }
    Ok(bytes)
}

fn get_json(agent: &ureq::Agent, url: &str) -> Result<Value, String> {
    serde_json::from_slice(&get(agent, url, JSON_LIMIT)?).map_err(|e| format!("{url}: {e}"))
}

fn quotes(agent: &ureq::Agent) -> Result<Vec<Quote>, String> {
    let symbols = COINS
        .iter()
        .map(|coin| format!("%22{coin}USDT%22"))
        .collect::<Vec<_>>()
        .join(",");
    let mut error = String::new();
    // Binance.com refuses some regions; Binance.US answers the same call.
    for host in PRICE_HOSTS {
        let url = format!("{host}/api/v3/ticker/24hr?type=MINI&symbols=%5B{symbols}%5D");
        match get_json(agent, &url).and_then(|body| parse_quotes(&body)) {
            Ok(quotes) if !quotes.is_empty() => return Ok(quotes),
            Ok(_) => error = format!("{host}: no prices"),
            Err(e) => error = e,
        }
    }
    Err(error)
}

/// Mini tickers in `COINS` order; pairs Binance does not list are left out.
pub fn parse_quotes(body: &Value) -> Result<Vec<Quote>, String> {
    let items = body
        .as_array()
        .ok_or("Binance did not return a ticker list")?;
    let number = |item: &Value, key: &str| {
        item.get(key)
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<f64>().ok())
    };
    Ok(COINS
        .iter()
        .filter_map(|coin| {
            let symbol = format!("{coin}USDT");
            let item = items
                .iter()
                .find(|item| item.get("symbol").and_then(Value::as_str) == Some(&symbol))?;
            let (open, last) = (number(item, "openPrice")?, number(item, "lastPrice")?);
            Some(Quote {
                coin,
                last,
                change: if open > 0. {
                    (last - open) / open * 100.
                } else {
                    0.
                },
                low: number(item, "lowPrice")?,
                high: number(item, "highPrice")?,
            })
        })
        .collect())
}

/// Dates from three days ago to ten days ahead, `YYYY-MM-DD`.
pub fn schedule_days(today: time::Date) -> Vec<String> {
    (-3..=10)
        .filter_map(|offset| today.checked_add(time::Duration::days(offset)))
        .map(|day| {
            format!(
                "{:04}-{:02}-{:02}",
                day.year(),
                day.month() as u8,
                day.day()
            )
        })
        .collect()
}

fn premieres(agent: &ureq::Agent) -> Result<Vec<Premiere>, String> {
    let today = time::OffsetDateTime::now_utc().date();
    let mut days = Vec::new();
    let mut failures = 0;
    for day in schedule_days(today) {
        for schedule in SCHEDULES {
            let url = if schedule.ends_with("/web") {
                format!("{schedule}?date={day}")
            } else {
                format!("{schedule}?country=US&date={day}")
            };
            match get_json(agent, &url) {
                Ok(body) => days.push(body),
                Err(error) => {
                    failures += 1;
                    eprintln!("Luma premieres: {error}");
                }
            }
        }
    }
    if days.is_empty() {
        return Err(format!("TVmaze unavailable ({failures} requests failed)"));
    }
    Ok(parse_premieres(&days))
}

/// First episodes of popular scripted or animated shows, one per show, by air date.
pub fn parse_premieres(days: &[Value]) -> Vec<Premiere> {
    let mut found: Vec<(u64, u64, Premiere)> = Vec::new();
    for episode in days.iter().filter_map(Value::as_array).flatten() {
        let show = episode
            .pointer("/_embedded/show")
            .or_else(|| episode.get("show"));
        let Some(show) = show else { continue };
        let kind = show.get("type").and_then(Value::as_str).unwrap_or("");
        let weight = show.get("weight").and_then(Value::as_u64).unwrap_or(0);
        if episode.get("number").and_then(Value::as_u64) != Some(1)
            || weight < MIN_WEIGHT
            || !matches!(kind, "Scripted" | "Animation")
        {
            continue;
        }
        let id = show.get("id").and_then(Value::as_u64).unwrap_or(0);
        if found.iter().any(|(other, ..)| *other == id) {
            continue;
        }
        let (Some(title), Some(date)) = (
            show.get("name").and_then(Value::as_str),
            episode.get("airdate").and_then(Value::as_str),
        ) else {
            continue;
        };
        let poster = show
            .pointer("/image/medium")
            .and_then(Value::as_str)
            .filter(|url| poster_allowed(url))
            .map(str::to_owned);
        found.push((
            id,
            weight,
            Premiere {
                title: title.chars().take(80).collect(),
                season: episode.get("season").and_then(Value::as_u64).unwrap_or(1),
                date: date.chars().take(10).collect(),
                poster,
            },
        ));
    }
    found.sort_by(|a, b| a.2.date.cmp(&b.2.date).then(b.1.cmp(&a.1)));
    found
        .into_iter()
        .take(PREMIERES)
        .map(|(.., premiere)| premiere)
        .collect()
}

pub fn poster_allowed(url: &str) -> bool {
    url.starts_with(POSTER_ORIGIN) && !url.contains("..") && url.len() < 512
}

/// A PandaScore list with the user's key; a refused key is told apart from other failures.
fn pandascore(
    agent: &ureq::Agent,
    key: &str,
    game: &str,
    path: &str,
) -> Result<Value, MatchesError> {
    let url = format!("{PANDASCORE}/{}{path}", game_path(game));
    let mut response = agent
        .get(&url)
        .header("Authorization", format!("Bearer {}", key.trim()))
        .header("Accept", "application/json")
        .header("User-Agent", "LumaNative")
        .config()
        .http_status_as_error(false)
        .build()
        .call()
        .map_err(|error| {
            eprintln!("Luma matches: {error}");
            MatchesError::Unavailable
        })?;
    let status = response.status().as_u16();
    if status == 401 || status == 403 {
        return Err(MatchesError::Key);
    }
    if !(200..300).contains(&status) {
        eprintln!("Luma matches: {url}: HTTP {status}");
        return Err(MatchesError::Unavailable);
    }
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(JSON_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| MatchesError::Unavailable)?;
    if bytes.len() > JSON_LIMIT {
        return Err(MatchesError::Unavailable);
    }
    serde_json::from_slice(&bytes).map_err(|_| MatchesError::Unavailable)
}

/// Every running and upcoming match of a discipline; widgets apply their own event filter.
fn matches(agent: &ureq::Agent, key: &str, game: &str) -> Result<Vec<CsMatch>, MatchesError> {
    let running = pandascore(
        agent,
        key,
        game,
        "/matches/running?per_page=25&sort=begin_at",
    )?;
    let upcoming = pandascore(
        agent,
        key,
        game,
        "/matches/upcoming?per_page=50&sort=begin_at",
    )?;
    let game = game_path(game);
    let mut matches = select_matches(parse_matches(&running), parse_matches(&upcoming), false);
    for found in &mut matches {
        found.game = game.into();
    }
    Ok(matches)
}

fn rosters(agent: &ureq::Agent, key: &str, game: &str, teams: &[u64]) -> Vec<(u64, Vec<String>)> {
    if teams.is_empty() {
        return vec![];
    }
    let ids = teams
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    pandascore(
        agent,
        key,
        game,
        // Brackets encoded: they are not allowed unencoded in a URI's query.
        &format!("/teams?filter%5Bid%5D={ids}&per_page=10"),
    )
    .map(|body| parse_rosters(&body))
    .unwrap_or_default()
}

/// PandaScore match objects; malformed entries are skipped.
pub fn parse_matches(body: &Value) -> Vec<CsMatch> {
    let text = |value: Option<&Value>| value.and_then(Value::as_str).unwrap_or("").to_owned();
    body.as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let id = item.get("id")?.as_u64()?;
            let teams: Vec<Option<Team>> = item
                .get("opponents")
                .and_then(Value::as_array)
                .map(|opponents| {
                    opponents
                        .iter()
                        .take(2)
                        .map(|entry| {
                            let team = entry.get("opponent")?;
                            let name: String = text(team.get("name")).chars().take(40).collect();
                            let acronym = text(team.get("acronym"));
                            Some(Team {
                                id: team.get("id")?.as_u64()?,
                                short: if acronym.is_empty() {
                                    name.clone()
                                } else {
                                    acronym.chars().take(12).collect()
                                },
                                name,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let teams = [
                teams.first().cloned().flatten(),
                teams.get(1).cloned().flatten(),
            ];
            let results = item.get("results").and_then(Value::as_array);
            let score = teams.clone().map(|team| {
                let id = team.map(|team| team.id);
                results
                    .into_iter()
                    .flatten()
                    .find(|result| result.get("team_id").and_then(Value::as_u64) == id)
                    .and_then(|result| result.get("score")?.as_u64())
                    .unwrap_or(0)
            });
            let mut games: Vec<MatchGame> = item
                .get("games")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|game| {
                    Some(MatchGame {
                        position: game.get("position")?.as_u64()?,
                        state: match game.get("status").and_then(Value::as_str) {
                            Some("running") => GameState::Running,
                            Some("finished") => GameState::Finished,
                            _ => GameState::Waiting,
                        },
                        winner: game.pointer("/winner/id").and_then(Value::as_u64),
                    })
                })
                .collect();
            games.sort_by_key(|game| game.position);
            let league = text(item.pointer("/league/name"));
            let serie = text(item.pointer("/serie/full_name"));
            Some(CsMatch {
                id,
                game: String::new(),
                running: item.get("status").and_then(Value::as_str) == Some("running"),
                begin_at: text(item.get("begin_at").or_else(|| item.get("scheduled_at"))),
                teams,
                score,
                best_of: item
                    .get("number_of_games")
                    .and_then(Value::as_u64)
                    .unwrap_or(1),
                games,
                event: format!("{league} {serie}")
                    .trim()
                    .chars()
                    .take(60)
                    .collect(),
                tier: text(item.pointer("/tournament/tier")).to_lowercase(),
            })
        })
        .collect()
}

/// Running matches first, then upcoming ones by start; with `major_only`, S- and A-tier events.
pub fn select_matches(
    running: Vec<CsMatch>,
    mut upcoming: Vec<CsMatch>,
    major_only: bool,
) -> Vec<CsMatch> {
    upcoming.sort_by(|a, b| a.begin_at.cmp(&b.begin_at));
    running
        .into_iter()
        .chain(upcoming)
        .filter(|m| !major_only || matches!(m.tier.as_str(), "s" | "a"))
        .take(MATCHES)
        .collect()
}

/// The shared feed's matches (already filtered to running and upcoming) and their lineups.
pub fn parse_feed(body: &Value, game: &str) -> (Vec<CsMatch>, Vec<(u64, Vec<String>)>) {
    let text = |value: Option<&Value>, limit: usize| -> String {
        value
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .chars()
            .take(limit)
            .collect()
    };
    let mut rosters = Vec::new();
    let matches = body
        .get("matches")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(MATCHES)
        .filter_map(|item| {
            let teams: Vec<Option<Team>> = item
                .get("teams")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .take(2)
                .map(|team| {
                    Some(Team {
                        id: team.get("id")?.as_u64()?,
                        name: text(team.get("name"), 40),
                        short: text(team.get("short"), 12),
                    })
                })
                .collect();
            let teams = [
                teams.first().cloned().flatten(),
                teams.get(1).cloned().flatten(),
            ];
            let lineups = item.get("lineups").and_then(Value::as_array);
            for (side, team) in teams.iter().enumerate() {
                if let (Some(team), Some(players)) = (
                    team,
                    lineups.and_then(|l| l.get(side)).and_then(Value::as_array),
                ) {
                    let names: Vec<String> = players
                        .iter()
                        .map(|p| text(Some(p), 24))
                        .filter(|n| !n.is_empty())
                        .take(7)
                        .collect();
                    if !names.is_empty() {
                        rosters.push((team.id, names));
                    }
                }
            }
            let score = item.get("score").and_then(Value::as_array);
            let side = |index: usize| {
                score
                    .and_then(|s| s.get(index))
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
            };
            Some(CsMatch {
                id: item.get("id")?.as_u64()?,
                game: game_path(game).into(),
                running: item
                    .get("running")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                begin_at: text(item.get("begin_at"), 32),
                teams,
                score: [side(0), side(1)],
                best_of: item.get("best_of").and_then(Value::as_u64).unwrap_or(1),
                games: item
                    .get("games")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|game| {
                        Some(MatchGame {
                            position: game.get("position")?.as_u64()?,
                            state: match game.get("state").and_then(Value::as_str) {
                                Some("running") => GameState::Running,
                                Some("finished") => GameState::Finished,
                                _ => GameState::Waiting,
                            },
                            winner: game.get("winner").and_then(Value::as_u64),
                        })
                    })
                    .collect(),
                event: text(item.get("event"), 60),
                tier: text(item.get("tier"), 12).to_lowercase(),
            })
        })
        .collect();
    (matches, rosters)
}

/// Player nicknames by team, from PandaScore team objects.
pub fn parse_rosters(body: &Value) -> Vec<(u64, Vec<String>)> {
    body.as_array()
        .into_iter()
        .flatten()
        .filter_map(|team| {
            let players = team
                .get("players")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|player| player.get("name")?.as_str())
                .map(|name| name.trim().chars().take(24).collect())
                .take(7)
                .collect();
            Some((team.get("id")?.as_u64()?, players))
        })
        .collect()
}

fn poster(agent: &ureq::Agent, url: &str) -> Option<Arc<Artwork>> {
    if !poster_allowed(url) {
        return None;
    }
    let bytes = get(agent, url, IMAGE_LIMIT)
        .map_err(|error| eprintln!("Luma poster: {error}"))
        .ok()?;
    let image = image::load_from_memory(&bytes).ok()?;
    let pixels = image.thumbnail(300, 450).into_rgba8();
    Some(Arc::new(Artwork {
        key: format!("poster:{url}"),
        pixels,
        backdrop: [[16, 16, 20]; 2],
    }))
}

#[cfg(test)]
#[path = "feeds-tests.rs"]
mod tests;
