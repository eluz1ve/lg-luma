use super::*;
use crate::{
    model::{Preferences, demo_inventory},
    shows::{Episode, Show},
};

/// 1 November 2026, 20:00 UTC; the TV's day is that UTC day.
const NOW: i64 = 1_793_563_200;
const DAY: (i64, i64) = (1_793_491_200, 1_793_491_200 + 86_400);

fn state() -> State {
    let mut state = State::new(Preferences {
        show_cs2: true,
        show_shows: true,
        ..Preferences::default()
    });
    state.set_inventory(demo_inventory(), None);
    state
}

/// A feed of `(id, state, begins)` matches, received just now.
fn feed(state: &mut State, matches: &[(&str, &str, i64)]) {
    let iso = |unix: i64| {
        time::OffsetDateTime::from_unix_timestamp(unix)
            .unwrap()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap()
    };
    let matches: Vec<_> = matches
        .iter()
        .map(|(id, kind, begins)| {
            serde_json::json!({
                "id": id, "begin_at": iso(*begins), "state": kind, "best_of": 3,
                "tournament": "IEM", "tier": 1,
                "teams": [{"name": "Vitality"}, {"name": "FaZe"}], "maps": []
            })
        })
        .collect();
    let feed =
        crate::cs2::parse_feed(&serde_json::json!({"updated": NOW, "matches": matches})).unwrap();
    let (Some(command), _) = state.cs2.sync(true, true, NOW) else {
        panic!("a fetch is due");
    };
    state.cs2.accept(command.id, Ok(feed), NOW);
}

fn show(id: i64, app: bool, airs: &[i64]) -> Show {
    Show {
        id,
        title: format!("Show {id}"),
        poster: None,
        meta: String::new(),
        service: Some("Netflix"),
        app: app.then(|| "netflix".to_owned()),
        episodes: airs
            .iter()
            .map(|&airs| Episode {
                airs,
                season: Some(1),
                number: Some(2),
            })
            .collect(),
    }
}

fn matched(id: &str) -> Line {
    Line::Match { id: id.into() }
}

#[test]
fn live_matches_come_first_then_events_by_time_then_aired_episodes() {
    let mut state = state();
    feed(
        &mut state,
        &[
            ("soon", "upcoming", NOW + 2 * 3600),
            ("live", "live", NOW - 1800),
            ("done", "finished", NOW - 7200),
            ("later", "upcoming", NOW + 13 * 3600),
        ],
    );
    state.shows.list = vec![
        show(1, true, &[NOW - 5 * 3600]),
        show(2, true, &[NOW + 3600]),
    ];
    let today = today(&state, NOW, DAY);
    assert_eq!(
        today.lines,
        [
            matched("live"),
            Line::Episode {
                show: 2,
                airs: NOW + 3600
            },
            matched("soon"),
        ]
    );
    assert_eq!(today.more, 1, "the episode that aired this afternoon");
}

#[test]
fn crypto_beside_is_always_the_last_line_and_the_rest_counts_as_more() {
    let mut state = state();
    state.preferences.show_crypto = true;
    state.preferences.crypto_beside = true;
    feed(
        &mut state,
        &[("a", "live", NOW), ("b", "live", NOW), ("c", "live", NOW)],
    );
    let today = today(&state, NOW, DAY);
    assert_eq!(today.lines, [matched("a"), matched("b"), Line::Crypto]);
    assert_eq!(today.more, 1);
}

#[test]
fn widgets_kept_as_rows_or_switched_off_add_nothing() {
    let mut state = state();
    feed(&mut state, &[("a", "live", NOW)]);
    state.shows.list = vec![show(1, true, &[NOW + 60])];
    state.preferences.show_crypto = true;
    state.preferences.crypto_beside = false;
    state.preferences.cs2_today = false;
    state.preferences.shows_today = false;
    assert_eq!(today(&state, NOW, DAY), Today::default());
    state.preferences.cs2_today = true;
    state.preferences.shows_today = true;
    state.preferences.show_cs2 = false;
    state.preferences.show_shows = false;
    assert_eq!(today(&state, NOW, DAY), Today::default());
}

#[test]
fn episodes_need_an_installed_app_and_todays_air_time() {
    let mut state = state();
    state.shows.list = vec![
        show(1, false, &[NOW + 60]),
        show(2, true, &[DAY.0 - 60, DAY.1]),
        show(3, true, &[DAY.0 - 86_400, DAY.0 + 60]),
    ];
    assert_eq!(
        today(&state, NOW, DAY).lines,
        [Line::Episode {
            show: 3,
            airs: DAY.0 + 60
        }]
    );
}

#[test]
fn a_stale_feed_adds_no_matches() {
    let mut state = state();
    feed(&mut state, &[("a", "live", NOW)]);
    let later = NOW + crate::cs2::MAX_AGE + 1;
    assert!(today(&state, later, DAY).lines.is_empty());
}

#[test]
fn old_preferences_without_today_flags_put_cs2_and_shows_in_today() {
    let preferences: Preferences = serde_json::from_str("{}").unwrap();
    assert!(preferences.cs2_today);
    assert!(preferences.shows_today);
}
