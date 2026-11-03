//! What Home's Today card, beside the greeting, lists: live CS2 matches, then today's events by
//! time (matches about to start, new episodes of shows an installed app plays), then the episodes
//! that already aired today. Signed in to Lampa's CUB account, the user's own shows replace
//! TVmaze's: today's releases, the week's unwatched ones and what to continue, each kind given a
//! line before any gets a second. Crypto prices take the last line when they sit beside the
//! greeting.
use crate::{cs2, cub::Cub, state::State};
use time::Date;

#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Match {
        id: String,
    },
    Episode {
        show: i64,
        airs: i64,
    },
    /// A bookmarked card's release on `date`, not watched yet.
    Release {
        card: String,
        date: Date,
    },
    /// A series watched lately, to pick up where it was left.
    Continue {
        card: String,
    },
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
/// A series watched within this long is offered to continue.
const CONTINUE_WITHIN: i64 = 21 * 86_400;

/// The card's lines at `now`; `day` is the TV's local day, from midnight to midnight, and `date`
/// its date.
pub fn today(state: &State, now: i64, day: (i64, i64), date: Date) -> Today {
    let preferences = &state.preferences;
    let lampa = preferences.lampa_today && state.cub.signed_in();
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
    if preferences.show_shows && preferences.shows_today && !lampa {
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
    let events: Vec<Line> = coming
        .into_iter()
        .map(|(_, line)| line)
        .chain(aired.into_iter().map(|(_, line)| line))
        .collect();
    let groups = if lampa {
        let [fresh, week, resume] = lampa_lines(&state.cub, now, date);
        vec![fresh, events, week, resume]
    } else {
        vec![events]
    };
    let mut lines = live;
    lines.extend(interleave(groups));
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

/// Lampa's lines: today's releases, the week's earlier ones, and series to continue; one line
/// per card, its newest release.
fn lampa_lines(cub: &Cub, now: i64, date: Date) -> [Vec<Line>; 3] {
    let mut releases: Vec<_> = cub
        .releases
        .iter()
        .filter(|release| !release.watched && release.date <= date)
        .collect();
    releases.sort_by_key(|release| std::cmp::Reverse(release.date));
    let mut cards: Vec<&str> = Vec::new();
    let (mut fresh, mut week) = (Vec::new(), Vec::new());
    for release in releases {
        if cards.contains(&release.card.as_str()) {
            continue;
        }
        cards.push(&release.card);
        let line = Line::Release {
            card: release.card.clone(),
            date: release.date,
        };
        if release.date == date {
            fresh.push(line);
        } else {
            week.push(line);
        }
    }
    let mut watched: Vec<_> = cub
        .watched
        .iter()
        // Series only: whether a film was watched to the end is not known.
        .filter(|watched| watched.card.starts_with("tv_") && watched.at >= now - CONTINUE_WITHIN)
        .filter(|watched| !cards.contains(&watched.card.as_str()))
        .collect();
    watched.sort_by_key(|watched| std::cmp::Reverse(watched.at));
    let resume = watched
        .into_iter()
        .map(|watched| Line::Continue {
            card: watched.card.clone(),
        })
        .collect();
    [fresh, week, resume]
}

/// The groups' lines taken in turns: each group's first, then each one's second, and so on.
fn interleave(groups: Vec<Vec<Line>>) -> Vec<Line> {
    let mut groups: Vec<_> = groups.into_iter().map(Vec::into_iter).collect();
    let mut lines = Vec::new();
    loop {
        let before = lines.len();
        for group in &mut groups {
            lines.extend(group.next());
        }
        if lines.len() == before {
            return lines;
        }
    }
}

#[cfg(test)]
#[path = "today-tests.rs"]
mod tests;
