//! Lampa's account on CUB (cub.red): what the user watches, for Home's Today card. The TV signs
//! in by device code (OAuth 2.0 Device Flow): Luma shows a code, the user confirms it on their
//! phone at cub.red/activate, and Luma reads the account with a token, never a password.
use crate::{app_artwork::Artwork, i18n::Language};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, path::Path, sync::Arc};
use time::{Date, Month};

/// Luma's CUB app, of the TV type: an id, no secret.
pub const CLIENT_ID: &str = "ddcd0b0e58629088c5445df31fe1b9d5";
/// Read access only: the profile's name, bookmarks (and so its release calendar) and history.
pub const SCOPE: &str = "profile bookmarks:read history:read";
/// cub.red is blocked in some countries; its mirrors serve the same API.
pub const HOSTS: [&str; 3] = ["cub.red", "cub.best", "cub.black"];

/// The account's tokens: access lasts a day, refresh 90 days and works once.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
    /// When `access` expires, Unix seconds.
    pub expires: i64,
}

/// A sign-in waiting for the user to confirm its code.
#[derive(Clone, Debug, PartialEq)]
pub struct Login {
    pub device_code: String,
    pub user_code: String,
    /// The page to open, without its scheme: "cub.red/activate".
    pub uri: String,
    /// The same page with the code filled in, for the QR code.
    pub link: String,
    /// Seconds between polls.
    pub interval: u64,
    /// Unix seconds.
    pub expires: i64,
}

/// The token endpoint's answer to a poll or a refresh.
#[derive(Clone, Debug, PartialEq)]
pub enum Grant {
    Tokens(Tokens),
    /// Not confirmed yet; ask again after the interval.
    Pending,
    /// Asked too often; wait five seconds longer.
    SlowDown,
    Denied,
    Expired,
    /// The refresh token is gone (used, expired, or the app was disconnected): sign in again.
    Revoked,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Badge {
    None,
    /// The show's first episode.
    Premiere,
    SeasonPremiere,
    Finale,
    /// Already out and not in the history.
    New,
    Movie,
}

/// A day's release of a bookmarked card: a series' episodes, or a film's premiere.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    /// "tv_2190", as CUB names cards.
    pub card: String,
    pub title: String,
    /// TMDB's poster path, e.g. "/1CGw.jpg".
    pub poster: Option<String>,
    /// IMDb's id, for its English poster.
    pub imdb: Option<String>,
    pub date: Date,
    pub season: u32,
    pub episodes: Vec<u32>,
    pub badge: Badge,
    /// Every episode of it (or the film) is in the history.
    pub watched: bool,
}

/// A card from the history with what was last watched of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Watched {
    pub card: String,
    pub title: String,
    pub poster: Option<String>,
    pub imdb: Option<String>,
    /// (season, episode) for a series.
    pub episode: Option<(u32, u32)>,
    /// Unix seconds.
    pub at: i64,
}

impl Release {
    pub fn poster_key(&self, language: Language) -> Option<String> {
        crate::shows::poster_key(self.poster.as_deref(), self.imdb.as_deref(), language)
    }
}

impl Watched {
    pub fn poster_key(&self, language: Language) -> Option<String> {
        crate::shows::poster_key(self.poster.as_deref(), self.imdb.as_deref(), language)
    }
}

/// The account is read this often, and sooner after a failed read.
pub const REFRESH: i64 = 30 * 60;
pub const RETRY: i64 = 5 * 60;
/// Back on screen (from watching in Lampa, say), the account is read again unless it was read
/// this recently.
pub const FRESH: i64 = 2 * 60;

/// Why an account read failed.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    /// The tokens no longer work: the user disconnected Luma or the refresh token lapsed.
    SignedOut,
    Unavailable(String),
}

/// What an account read brings.
#[derive(Clone, Debug, PartialEq)]
pub struct Account {
    pub profile: Profile,
    pub releases: Vec<Release>,
    pub watched: Vec<Watched>,
}

/// Access tokens last a day; they are renewed in the hour before.
pub fn needs_refresh(tokens: &Tokens, now: i64) -> bool {
    tokens.expires - now < 3600
}

/// The calendar's days read: the week before today, for new episodes not yet watched, through
/// tomorrow, which west of UTC holds tonight's.
pub fn range(today: Date) -> (Date, Date) {
    (
        today - time::Duration::days(7),
        today + time::Duration::days(1),
    )
}

/// What the worker should be asked next.
#[derive(Clone, Debug, PartialEq)]
pub enum Due {
    /// A device code to show.
    SignIn,
    Poll(String),
    Read {
        id: u64,
        tokens: Tokens,
        profile: Option<Profile>,
    },
}

/// What a reply changed on disk.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Saved {
    Nothing,
    /// The tokens or profile changed: write them.
    Tokens,
    /// Signed out: remove them.
    Forget,
}

/// The account as the TV knows it, and the sign-in under way.
#[derive(Default)]
pub struct Cub {
    pub tokens: Option<Tokens>,
    pub profile: Option<Profile>,
    /// Waiting for a device code.
    pub signing_in: bool,
    /// The device code was asked for.
    asked: bool,
    /// The code shown, while the user confirms it.
    pub login: Option<Login>,
    next_poll: i64,
    polling: bool,
    pub releases: Vec<Release>,
    pub watched: Vec<Watched>,
    /// When the account was last read, Unix seconds.
    pub fetched: Option<i64>,
    next_read: i64,
    reading: Option<u64>,
    request: u64,
    /// The last sign-in or read failed.
    pub failed: bool,
    /// Posters by key (see `shows::poster_key`); None while loading or when there is none.
    posters: HashMap<String, Option<Arc<Artwork>>>,
    /// Luma is on screen.
    visible: bool,
}

impl Cub {
    pub fn signed_in(&self) -> bool {
        self.tokens.is_some()
    }

    /// Signs in with tokens kept from an earlier run.
    pub fn sign_in_with(&mut self, tokens: Tokens, profile: Option<Profile>) {
        self.tokens = Some(tokens);
        self.profile = profile;
        self.next_read = 0;
    }

    /// A sign-in starts: the worker is asked for a device code.
    pub fn begin(&mut self) {
        self.signing_in = true;
        self.asked = false;
        self.login = None;
        self.failed = false;
    }

    pub fn cancel(&mut self) {
        self.signing_in = false;
        self.login = None;
        self.polling = false;
    }

    pub fn login(&mut self, result: Result<Login, String>, now: i64) {
        if !self.signing_in {
            return;
        }
        match result {
            Ok(login) => {
                self.next_poll = now + login.interval as i64;
                self.login = Some(login);
            }
            Err(_) => {
                self.signing_in = false;
                self.failed = true;
            }
        }
    }

    pub fn due(&mut self, now: i64) -> Option<Due> {
        if self.signing_in && self.login.is_none() {
            return (!std::mem::replace(&mut self.asked, true)).then_some(Due::SignIn);
        }
        if let Some(login) = &self.login {
            if now >= login.expires {
                self.cancel();
                self.failed = true;
                return None;
            }
            if !self.polling && now >= self.next_poll {
                self.polling = true;
                return Some(Due::Poll(login.device_code.clone()));
            }
            return None;
        }
        let tokens = self.tokens.as_ref()?;
        if self.reading.is_some() || now < self.next_read {
            return None;
        }
        self.request += 1;
        self.reading = Some(self.request);
        Some(Due::Read {
            id: self.request,
            tokens: tokens.clone(),
            profile: self.profile.clone(),
        })
    }

    /// A poll's answer; the tokens to keep when the user confirmed.
    pub fn grant(&mut self, device_code: &str, grant: Grant, now: i64) -> Option<Tokens> {
        let login = self.login.as_mut()?;
        if login.device_code != device_code {
            return None;
        }
        self.polling = false;
        match grant {
            Grant::Tokens(tokens) => {
                self.cancel();
                self.failed = false;
                self.profile = None;
                self.tokens = Some(tokens.clone());
                self.reading = None;
                self.next_read = now;
                return Some(tokens);
            }
            Grant::Pending | Grant::Failed(_) => self.next_poll = now + login.interval as i64,
            Grant::SlowDown => {
                login.interval += 5;
                self.next_poll = now + login.interval as i64;
            }
            Grant::Denied | Grant::Expired | Grant::Revoked => {
                self.cancel();
                self.failed = true;
            }
        }
        None
    }

    pub fn account(
        &mut self,
        id: u64,
        refreshed: Option<Tokens>,
        result: Result<Account, Failure>,
        now: i64,
    ) -> Saved {
        if self.reading != Some(id) {
            return Saved::Nothing;
        }
        self.reading = None;
        let mut saved = Saved::Nothing;
        if let Some(tokens) = refreshed {
            self.tokens = Some(tokens);
            saved = Saved::Tokens;
        }
        match result {
            Ok(account) => {
                if self.profile.as_ref() != Some(&account.profile) {
                    saved = Saved::Tokens;
                }
                self.profile = Some(account.profile);
                self.releases = account.releases;
                self.watched = account.watched;
                // Keep only the posters of what the account still holds, in either language.
                let (releases, watched) = (&self.releases, &self.watched);
                self.posters.retain(|key, _| {
                    let source = crate::shows::poster_source(key);
                    let holds = |poster: &Option<String>, imdb: &Option<String>| {
                        source == (imdb.as_deref(), poster.as_deref())
                            || source == (None, poster.as_deref())
                    };
                    releases.iter().any(|r| holds(&r.poster, &r.imdb))
                        || watched.iter().any(|w| holds(&w.poster, &w.imdb))
                });
                self.fetched = Some(now);
                self.failed = false;
                self.next_read = now + REFRESH;
            }
            Err(Failure::SignedOut) => {
                self.sign_out();
                return Saved::Forget;
            }
            Err(Failure::Unavailable(_)) => {
                self.failed = true;
                self.next_read = now + RETRY;
            }
        }
        saved
    }

    /// Luma's visibility, each frame: coming back on screen, a read older than `FRESH` is
    /// renewed at once, so what was just watched in Lampa shows.
    pub fn shown(&mut self, visible: bool, now: i64) {
        let back = visible && !self.visible;
        self.visible = visible;
        if back && self.fetched.is_some_and(|at| now - at >= FRESH) {
            self.next_read = self.next_read.min(now);
        }
    }

    /// Posters in `language` Today and the Lampa row may show and not asked for yet: unwatched
    /// releases and recent history.
    pub fn wanted_posters(&mut self, language: Language) -> Vec<String> {
        let paths: Vec<String> = self
            .releases
            .iter()
            .filter(|release| !release.watched)
            .filter_map(|release| release.poster_key(language))
            // The history fills Home's Lampa row as well as Today.
            .chain(
                self.watched
                    .iter()
                    .take(crate::shows::SHOWN)
                    .filter_map(|w| w.poster_key(language)),
            )
            .collect();
        let mut wanted = Vec::new();
        for path in paths {
            if !self.posters.contains_key(&path) {
                self.posters.insert(path.clone(), None);
                wanted.push(path);
            }
        }
        wanted
    }

    /// A poster's answer; whether it was asked for.
    pub fn accept_poster(&mut self, path: String, image: Option<Arc<Artwork>>) -> bool {
        match self.posters.get_mut(&path) {
            Some(slot) => {
                *slot = image;
                true
            }
            None => false,
        }
    }

    pub fn poster(&self, path: &str) -> Option<Arc<Artwork>> {
        self.posters.get(path)?.clone()
    }

    /// Forgets the account; a read still in flight is then ignored.
    pub fn sign_out(&mut self) {
        *self = Self {
            request: self.request,
            ..Self::default()
        };
    }
}

/// The sign-in as kept on the TV, beside the preferences, readable by Luma alone.
#[derive(Serialize, Deserialize)]
struct Kept {
    tokens: Tokens,
    profile: Option<Profile>,
}

pub fn load(path: &Path) -> Option<(Tokens, Option<Profile>)> {
    let kept: Kept = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    Some((kept.tokens, kept.profile))
}

pub fn save(path: &Path, tokens: &Tokens, profile: Option<&Profile>) -> Result<(), String> {
    let kept = Kept {
        tokens: tokens.clone(),
        profile: profile.cloned(),
    };
    let bytes = serde_json::to_vec(&kept).map_err(|e| e.to_string())?;
    crate::storage::write_atomic(path, &bytes)
}

pub fn forget(path: &Path) {
    let _ = std::fs::remove_file(path);
}

pub(crate) fn text(value: Option<&Value>, limit: usize) -> Option<String> {
    let text: String = value?
        .as_str()?
        .chars()
        .filter(|c| !c.is_control())
        .take(limit)
        .collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn number(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
}

fn small(value: Option<&Value>) -> u32 {
    number(value)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0)
}

/// A TMDB poster path: one file name under the root.
pub(crate) fn poster_path(value: Option<&Value>) -> Option<String> {
    let path = value?.as_str()?;
    let name = path.strip_prefix('/')?;
    (!name.is_empty()
        && name.len() <= 100
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')))
    .then(|| path.to_owned())
}

pub fn parse_login(reply: &Value, now: i64) -> Option<Login> {
    let link = text(reply.get("verification_uri_complete"), 300);
    let uri = text(reply.get("verification_uri"), 200)?;
    Some(Login {
        device_code: text(reply.get("device_code"), 200)?,
        user_code: text(reply.get("user_code"), 20)?,
        link: link.unwrap_or_else(|| uri.clone()),
        uri: uri
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .to_owned(),
        interval: number(reply.get("interval")).unwrap_or(5).clamp(1, 60) as u64,
        expires: now
            + number(reply.get("expires_in"))
                .unwrap_or(600)
                .clamp(60, 3600),
    })
}

pub fn parse_grant(reply: &Value, now: i64) -> Grant {
    if let (Some(access), Some(refresh)) = (
        text(reply.get("access_token"), 400),
        text(reply.get("refresh_token"), 400),
    ) {
        let lasts = number(reply.get("expires_in")).unwrap_or(86_400).max(60);
        return Grant::Tokens(Tokens {
            access,
            refresh,
            expires: now + lasts,
        });
    }
    match reply.get("error").and_then(Value::as_str) {
        Some("authorization_pending") => Grant::Pending,
        Some("slow_down") => Grant::SlowDown,
        Some("access_denied") => Grant::Denied,
        Some("expired_token") => Grant::Expired,
        Some("invalid_grant") => Grant::Revoked,
        error => Grant::Failed(
            text(reply.get("error_description"), 200)
                .or_else(|| error.map(String::from))
                .unwrap_or_else(|| "CUB did not answer".into()),
        ),
    }
}

/// The main profile, else the first.
pub fn parse_profile(reply: &Value) -> Option<Profile> {
    let profiles = reply
        .get("profiles")
        .and_then(Value::as_array)
        .or_else(|| reply.as_array())?;
    let main = |profile: &&Value| {
        profile
            .get("main")
            .is_some_and(|main| main.as_bool() == Some(true) || main.as_i64() == Some(1))
    };
    let profile = profiles.iter().find(main).or_else(|| profiles.first())?;
    let id = profile.get("id")?;
    Some(Profile {
        id: id
            .as_str()
            .map(String::from)
            .or_else(|| id.as_i64().map(|id| id.to_string()))?,
        name: text(profile.get("name"), 60).unwrap_or_default(),
    })
}

fn card_id(card: &Value, kind: Option<&Value>) -> Option<String> {
    let kind = kind
        .or_else(|| card.get("type"))
        .and_then(Value::as_str)
        .filter(|kind| matches!(*kind, "tv" | "movie"))?;
    Some(format!("{kind}_{}", number(card.get("id"))?))
}

/// "2026-11-03".
pub(crate) fn day(value: Option<&Value>) -> Option<Date> {
    let mut parts = value?.as_str()?.splitn(3, '-').map(str::parse::<i32>);
    let (year, month, day) = (
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    );
    let month = Month::try_from(u8::try_from(month).ok()?).ok()?;
    Date::from_calendar_date(year, month, u8::try_from(day).ok()?).ok()
}

pub fn parse_calendar(reply: &Value) -> Vec<Release> {
    let Some(items) = reply.get("items").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let card = item.get("card")?;
            let date = day(item.get("date"))?;
            Some(Release {
                card: card_id(card, None)?,
                title: text(card.get("name"), 120)
                    .or_else(|| text(card.get("original_name"), 120))?,
                poster: poster_path(card.get("poster_path")),
                imdb: crate::shows::imdb_id(card.get("imdb_id")),
                date,
                season: small(item.get("season")),
                episodes: item
                    .get("episodes")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|episode| small(episode.get("episode")))
                    .filter(|episode| *episode > 0)
                    .collect(),
                badge: match item.get("badge").and_then(Value::as_str) {
                    Some("premiere") => Badge::Premiere,
                    Some("season_premiere") => Badge::SeasonPremiere,
                    Some("finale") => Badge::Finale,
                    Some("new") => Badge::New,
                    Some("movie") => Badge::Movie,
                    _ => Badge::None,
                },
                watched: item.get("watched").and_then(Value::as_bool) == Some(true),
            })
        })
        .collect()
}

/// One entry per card (`group=card`), newest first.
pub fn parse_history(reply: &Value) -> Vec<Watched> {
    let Some(history) = reply.get("history").and_then(Value::as_array) else {
        return Vec::new();
    };
    history
        .iter()
        .filter_map(|entry| {
            let card = entry.get("card")?;
            let last = entry
                .get("last")
                .filter(|last| last.is_object())
                .unwrap_or(entry);
            let millis =
                number(last.get("watched_at")).or_else(|| number(entry.get("watched_at")))?;
            let known = |season: u32, episode: u32| season > 0 && episode > 0;
            // Without its last episode, the latest of the episodes watched.
            let episode = Some((small(last.get("season")), small(last.get("episode"))))
                .filter(|&(season, episode)| known(season, episode))
                .or_else(|| {
                    entry
                        .get("episodes")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .map(|e| (small(e.get("season")), small(e.get("episode"))))
                        .filter(|&(season, episode)| known(season, episode))
                        .max()
                });
            Some(Watched {
                card: text(entry.get("card_id"), 40)
                    .or_else(|| card_id(card, entry.get("card_type")))?,
                title: text(card.get("name"), 120)
                    .or_else(|| text(card.get("original_name"), 120))?,
                poster: poster_path(card.get("poster_path")),
                imdb: crate::shows::imdb_id(card.get("imdb_id")),
                episode,
                at: millis / 1000,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "cub-tests.rs"]
mod tests;
