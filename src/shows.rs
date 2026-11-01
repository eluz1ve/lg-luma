//! TV Shows on Home: series streaming this week, from TVmaze's public schedule (no key). Shows an
//! installed app plays come first, the most popular first; OK opens that app. Requests and
//! scheduling mirror CS2 Score: one job in flight, generation IDs, posters on their own worker.
use crate::app_artwork::Artwork;
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};

/// Where posters may come from.
pub const POSTER_ORIGIN: &str = "https://static.tvmaze.com/uploads/images/";
/// The row lists; six cards show at a time, the favorites' width and pitch.
pub const SHOWN: usize = 18;
pub const VISIBLE: usize = 6;
/// The schedule changes daily; a failed fetch is retried sooner.
pub const REFRESH_SECONDS: i64 = 6 * 3600;
const RETRY_SECONDS: i64 = 5 * 60;
/// Posters kept in memory; more are forgotten and fetched again if needed.
const POSTER_CACHE: usize = 48;

/// A streaming service: its name on TVmaze and the words its TV app's id or title contains.
struct Service {
    name: &'static str,
    channel: &'static [&'static str],
    app: &'static [&'static str],
}

const SERVICES: &[Service] = &[
    Service {
        name: "Netflix",
        channel: &["netflix"],
        app: &["netflix"],
    },
    Service {
        name: "Disney+",
        channel: &["disney+", "hulu"],
        app: &["disney"],
    },
    Service {
        name: "Prime Video",
        channel: &["prime video", "amazon"],
        app: &["amazon", "primevideo"],
    },
    Service {
        name: "HBO Max",
        channel: &["hbo max", "max"],
        app: &["hbo", "wbd", ".max"],
    },
    Service {
        name: "Apple TV",
        channel: &["apple tv"],
        app: &["appletv", "apple tv"],
    },
    Service {
        name: "Paramount+",
        channel: &["paramount+"],
        app: &["paramount", "cbs"],
    },
    Service {
        name: "Crunchyroll",
        channel: &["crunchyroll"],
        app: &["crunchyroll"],
    },
    Service {
        name: "YouTube",
        channel: &["youtube"],
        app: &["youtube"],
    },
];

/// Apps sharing a service's word that do not play it: Alexa, stores, companions.
const NOT_PLAYERS: &[&str] = &["alexa", "assistant", "store", "companion", "demo", "setup"];

fn service(channel: &str) -> Option<&'static Service> {
    let channel = channel.to_lowercase();
    SERVICES.iter().find(|service| {
        service.channel.iter().any(|name| {
            // "Max" alone is a word; "Cinemax" is another channel.
            if *name == "max" {
                channel == "max"
            } else {
                channel.contains(name)
            }
        })
    })
}

/// The installed app that plays `service`, by its id or title.
fn app_for<'a>(service: &Service, installed: &'a [(String, String)]) -> Option<&'a str> {
    installed.iter().find_map(|(id, title)| {
        let (id_lower, title_lower) = (id.to_lowercase(), title.to_lowercase());
        let player = !NOT_PLAYERS
            .iter()
            .any(|bad| id_lower.contains(bad) || title_lower.contains(bad));
        (player
            && service
                .app
                .iter()
                .any(|word| id_lower.contains(word) || title_lower.contains(word)))
        .then_some(id.as_str())
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Show {
    pub id: i64,
    pub title: String,
    pub poster: Option<String>,
    /// "Drama · 2024 · ★ 8.1", whichever parts TVmaze has.
    pub meta: String,
    /// The streaming service, when Luma knows it.
    pub service: Option<&'static str>,
    /// The installed app that plays it.
    pub app: Option<String>,
    /// Its airings on the schedules read, earliest first.
    pub episodes: Vec<Episode>,
}

/// One airing of a show on a schedule.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Episode {
    /// Unix seconds.
    pub airs: i64,
    pub season: Option<u32>,
    pub number: Option<u32>,
}

/// Airings kept per show: a week of a daily show and the day after.
const EPISODES: usize = 8;

impl Show {
    /// Its first airing from `from` until before `to`.
    pub fn airing(&self, from: i64, to: i64) -> Option<&Episode> {
        self.episodes
            .iter()
            .find(|episode| (from..to).contains(&episode.airs))
    }

    fn add(&mut self, episode: Episode) {
        if self.episodes.contains(&episode) {
            return;
        }
        self.episodes.push(episode);
        self.episodes.sort_by_key(|episode| episode.airs);
        self.episodes.truncate(EPISODES);
    }
}

/// A schedule entry's airing; entries without an air time have none.
fn episode(entry: &Value) -> Option<Episode> {
    let airs = time::OffsetDateTime::parse(
        entry.get("airstamp")?.as_str()?,
        &time::format_description::well_known::Rfc3339,
    )
    .ok()?
    .unix_timestamp();
    let count = |key| {
        entry
            .get(key)
            .and_then(Value::as_u64)
            .and_then(|count| u32::try_from(count).ok())
    };
    Some(Episode {
        airs,
        season: count("season"),
        number: count("number"),
    })
}

fn text(value: &Value, limit: usize) -> String {
    value
        .as_str()
        .unwrap_or("")
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(limit)
        .collect()
}

/// One show from a schedule entry, with TVmaze's popularity (`weight`, 0–100).
fn show(value: &Value, installed: &[(String, String)]) -> Option<(i64, Show)> {
    let id = value.get("id")?.as_i64()?;
    let title = text(value.get("name")?, 120);
    if title.is_empty() {
        return None;
    }
    let poster = value
        .pointer("/image/medium")
        .and_then(Value::as_str)
        .filter(|url| url.starts_with(POSTER_ORIGIN) && url.len() <= 300)
        .map(String::from);
    let genre = value
        .pointer("/genres/0")
        .map(|genre| text(genre, 30))
        .unwrap_or_default();
    let year = value
        .get("premiered")
        .and_then(Value::as_str)
        .and_then(|date| date.get(..4))
        .filter(|year| year.bytes().all(|b| b.is_ascii_digit()))
        .unwrap_or("");
    let rating = value
        .pointer("/rating/average")
        .and_then(Value::as_f64)
        .filter(|rating| (0. ..=10.).contains(rating))
        .map(|rating| format!("★ {rating:.1}"))
        .unwrap_or_default();
    let meta = [genre.as_str(), year, rating.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    let channel = value
        .pointer("/webChannel/name")
        .or_else(|| value.pointer("/network/name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let service = service(channel);
    let app = service
        .and_then(|service| app_for(service, installed))
        .map(String::from);
    let weight = value.get("weight").and_then(Value::as_i64).unwrap_or(0);
    Some((
        weight,
        Show {
            id,
            title,
            poster,
            meta,
            service: service.map(|service| service.name),
            app,
            episodes: Vec::new(),
        },
    ))
}

/// The shows of TVmaze schedule replies (`/schedule/web` embeds them, `/schedule` nests them),
/// without repeats or posterless ones: those an installed app plays first, each group the most
/// popular first, each with its airings. At most `SHOWN`.
pub fn parse_schedules(replies: &[Value], installed: &[(String, String)]) -> Vec<Show> {
    let mut shows: Vec<(i64, Show)> = Vec::new();
    for entry in replies.iter().filter_map(Value::as_array).flatten() {
        let Some(value) = entry
            .pointer("/_embedded/show")
            .or_else(|| entry.get("show"))
        else {
            continue;
        };
        let Some((weight, show)) = show(value, installed).filter(|(_, show)| show.poster.is_some())
        else {
            continue;
        };
        let index = match shows.iter().position(|(_, seen)| seen.id == show.id) {
            Some(index) => index,
            None => {
                shows.push((weight, show));
                shows.len() - 1
            }
        };
        if let Some(episode) = episode(entry) {
            shows[index].1.add(episode);
        }
    }
    // Stable, so equally popular shows keep their schedule order.
    shows.sort_by_key(|(weight, show)| (show.app.is_none(), std::cmp::Reverse(*weight)));
    shows.truncate(SHOWN);
    shows.into_iter().map(|(_, show)| show).collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub id: u64,
    /// The country whose TV schedule joins the streaming one.
    pub country: String,
    /// The TV's apps as (id, title), to find the app that plays each show.
    pub installed: Vec<(String, String)>,
}

pub enum Poster {
    Loading,
    Ready(Arc<Artwork>),
    Missing,
}

#[derive(Default)]
pub struct Shows {
    pub list: Vec<Show>,
    pub loading: bool,
    pub failed: bool,
    pub posters: HashMap<String, Poster>,
    id: u64,
    pending: Option<u64>,
    next_fetch: i64,
}
impl Shows {
    pub fn generation(&self) -> u64 {
        self.id
    }
    /// Starts a fetch when Home shows the widget, the apps are known and the list is due.
    /// Returns the request to send and whether anything changed.
    pub fn sync(
        &mut self,
        enabled: bool,
        home_visible: bool,
        country: &str,
        installed: &[(String, String)],
        now: i64,
    ) -> (Option<Command>, bool) {
        let before = (self.loading, self.failed);
        let mut command = None;
        if !enabled || !home_visible || installed.is_empty() {
            if self.pending.take().is_some() {
                self.id += 1;
            }
            self.loading = false;
        } else if self.pending.is_none() && now >= self.next_fetch {
            self.id += 1;
            self.pending = Some(self.id);
            self.loading = self.list.is_empty();
            command = Some(Command {
                id: self.id,
                country: country.into(),
                installed: installed.to_vec(),
            });
        }
        (command, before != (self.loading, self.failed))
    }
    /// Takes the reply to the request in flight; replies to older requests are ignored.
    pub fn accept(&mut self, id: u64, result: Result<Vec<Show>, String>, now: i64) -> bool {
        if self.pending != Some(id) {
            return false;
        }
        self.pending = None;
        self.loading = false;
        match result {
            Ok(list) => {
                self.list = list;
                self.failed = false;
                self.next_fetch = now + REFRESH_SECONDS;
            }
            Err(_) => {
                self.failed = true;
                self.next_fetch = now + RETRY_SECONDS;
            }
        }
        true
    }
    /// Posters for the cards around `first` not fetched yet, marked as loading.
    pub fn wanted_posters(&mut self, first: usize) -> Vec<String> {
        let wanted: Vec<String> = self
            .list
            .iter()
            .skip(first.saturating_sub(VISIBLE))
            .take(VISIBLE * 3)
            .filter_map(|show| show.poster.clone())
            .filter(|url| !self.posters.contains_key(url))
            .collect();
        if self.posters.len() + wanted.len() > POSTER_CACHE {
            let keep: Vec<&String> = self.list.iter().filter_map(|s| s.poster.as_ref()).collect();
            self.posters.retain(|url, _| keep.contains(&url));
        }
        for url in &wanted {
            self.posters.insert(url.clone(), Poster::Loading);
        }
        wanted
    }
    pub fn accept_poster(&mut self, url: String, image: Option<Arc<Artwork>>) -> bool {
        if !matches!(self.posters.get(&url), Some(Poster::Loading)) {
            return false;
        }
        self.posters
            .insert(url, image.map_or(Poster::Missing, Poster::Ready));
        true
    }
    pub fn poster(&self, show: &Show) -> Option<Arc<Artwork>> {
        match self.posters.get(show.poster.as_ref()?) {
            Some(Poster::Ready(image)) => Some(image.clone()),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "shows-tests.rs"]
mod tests;
