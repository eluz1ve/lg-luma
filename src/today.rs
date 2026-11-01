//! What Home's Today card, beside the greeting, lists: live CS2 matches, then today's events by
//! time (matches about to start, new episodes of shows an installed app plays), then the episodes
//! that already aired today. Crypto prices take the last line when they sit beside the greeting.
use crate::{cs2, state::State};

#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Match { id: String },
    Episode { show: i64, airs: i64 },
    Crypto,
}

#[derive(Debug, Default, PartialEq)]
pub struct Today {
    pub lines: Vec<Line>,
    /// Events left out for want of room.
    pub more: usize,
}

/// The card's lines at most.
pub const LINES: usize = 3;
/// Upcoming matches count as today's this long before they start.
const MATCH_AHEAD: i64 = 12 * 3600;

/// The card's lines at `now`; `day` is the TV's local day, from midnight to midnight.
pub fn today(state: &State, now: i64, day: (i64, i64)) -> Today {
    let preferences = &state.preferences;
    let mut live = Vec::new();
    let mut coming = Vec::new();
    let mut aired = Vec::new();
    // An old feed's "live" may long be over.
    if preferences.show_cs2
        && preferences.cs2_today
        && !state.cs2.stale(now)
        && let Some(feed) = state.cs2.current(now)
    {
        for item in cs2::shown(feed, preferences.cs2_major_only, false) {
            let line = Line::Match {
                id: item.id.clone(),
            };
            match item.state {
                cs2::State::Live => live.push(line),
                cs2::State::Upcoming if item.begins < now + MATCH_AHEAD => {
                    coming.push((item.begins, line));
                }
                _ => {}
            }
        }
    }
    if preferences.show_shows && preferences.shows_today {
        for show in state.shows.list.iter().filter(|show| show.app.is_some()) {
            if let Some(episode) = show.today(day.0, day.1, now) {
                let line = Line::Episode {
                    show: show.id,
                    airs: episode.airs,
                };
                if episode.airs >= now {
                    coming.push((episode.airs, line));
                } else {
                    aired.push((episode.airs, line));
                }
            }
        }
    }
    coming.sort_by_key(|(time, _)| *time);
    aired.sort_by_key(|(time, _)| std::cmp::Reverse(*time));
    let mut lines: Vec<Line> = live
        .into_iter()
        .chain(coming.into_iter().map(|(_, line)| line))
        .chain(aired.into_iter().map(|(_, line)| line))
        .collect();
    let crypto = preferences.show_crypto
        && preferences.crypto_beside
        && !preferences.crypto_coins.is_empty();
    let room = LINES - usize::from(crypto);
    let more = lines.len().saturating_sub(room);
    lines.truncate(room);
    if crypto {
        lines.push(Line::Crypto);
    }
    Today { lines, more }
}

#[cfg(test)]
#[path = "today-tests.rs"]
mod tests;
