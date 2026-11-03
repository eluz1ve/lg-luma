//! What Home's Today card, beside the greeting, lists: live CS2 matches, then matches about to
//! start by time, then series of TV Shows with a new episode today. Signed in to Lampa's CUB
//! account, the user's own shows replace those: today's releases, the week's unwatched ones and
//! what to continue, each kind given a line before any gets a second. Crypto prices take the last
//! line when they sit beside the greeting.
use crate::{cs2, cub::Cub, state::State};
use time::Date;

#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Match {
        id: String,
    },
    /// A series of TV Shows with a new episode out today.
    Episode {
        show: i64,
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
pub const LINES: usize = 5;
/// Upcoming matches count as today's this long before they start.
const MATCH_AHEAD: i64 = 12 * 3600;
/// A series watched within this long is offered to continue.
const CONTINUE_WITHIN: i64 = 21 * 86_400;

/// The opened card's sections, in the order its lines come.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Section {
    /// Matches being played.
    Live,
    /// Matches about to start.
    Matches,
    /// Today's new episodes, or Lampa's releases.
    Episodes,
    /// Lampa's series to pick up.
    Continue,
    Prices,
}

impl Section {
    /// Its heading's message key.
    pub fn key(self) -> &'static str {
        match self {
            Section::Live => "todayLive",
            Section::Matches => "todayMatches",
            Section::Episodes => "todayEpisodes",
            Section::Continue => "todayContinue",
            Section::Prices => "todayPrices",
        }
    }
}

/// The day's lines by kind, before the card picks from them.
struct Day {
    live: Vec<Line>,
    /// Matches still to come, by time.
    coming: Vec<Line>,
    /// Series with a new episode today, the most watched first.
    aired: Vec<Line>,
    /// Lampa's releases today, the week's earlier ones and series to continue.
    lampa: Option<[Vec<Line>; 3]>,
    crypto: bool,
}

/// The card's lines at `now`; `date` is the TV's local date.
pub fn today(state: &State, now: i64, date: Date) -> Today {
    let Day {
        live,
        coming,
        aired,
        lampa,
        crypto,
    } = gather(state, now, date);
    let events: Vec<Line> = coming.into_iter().chain(aired).collect();
    let groups = match lampa {
        Some([fresh, week, resume]) => vec![fresh, events, week, resume],
        None => vec![events],
    };
    let mut lines = live;
    lines.extend(interleave(groups));
    let room = LINES - usize::from(crypto);
    let more = lines.len().saturating_sub(room);
    lines.truncate(room);
    if crypto {
        lines.push(Line::Crypto);
    }
    Today { lines, more }
}

/// The whole day for the opened card, by section, nothing left out; empty sections are dropped.
pub fn sections(state: &State, now: i64, date: Date) -> Vec<(Section, Vec<Line>)> {
    let Day {
        live,
        coming,
        aired,
        lampa,
        crypto,
    } = gather(state, now, date);
    let (matches, mut episodes): (Vec<_>, Vec<_>) = coming
        .into_iter()
        .partition(|line| matches!(line, Line::Match { .. }));
    episodes.extend(aired);
    let mut resume = Vec::new();
    if let Some([fresh, week, continued]) = lampa {
        episodes.extend(fresh);
        episodes.extend(week);
        resume = continued;
    }
    [
        (Section::Live, live),
        (Section::Matches, matches),
        (Section::Episodes, episodes),
        (Section::Continue, resume),
        (
            Section::Prices,
            if crypto { vec![Line::Crypto] } else { vec![] },
        ),
    ]
    .into_iter()
    .filter(|(_, lines)| !lines.is_empty())
    .collect()
}

fn gather(state: &State, now: i64, date: Date) -> Day {
    let preferences = &state.preferences;
    let lampa = preferences.lampa_today && state.cub.signed_in();
    let mut live = Vec::new();
    let mut coming = Vec::new();
    // An old feed's "live" may long be over.
    if preferences.in_today(crate::model::Center::Cs2)
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
    let aired = if preferences.in_today(crate::model::Center::Shows) && !lampa {
        state
            .shows
            .list
            .iter()
            .filter(|show| show.new_on(date))
            .map(|show| Line::Episode { show: show.id })
            .collect()
    } else {
        Vec::new()
    };
    coming.sort_by_key(|(time, _)| *time);
    Day {
        live,
        coming: coming.into_iter().map(|(_, line)| line).collect(),
        aired,
        lampa: lampa.then(|| lampa_lines(&state.cub, now, date)),
        crypto: preferences.in_today(crate::model::Center::Crypto),
    }
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
