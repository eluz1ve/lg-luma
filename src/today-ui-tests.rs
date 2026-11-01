use super::*;
use crate::shows::{Episode, Show};

/// 1 November 2026, 20:00 UTC; the TV is on UTC.
const NOW: i64 = 1_793_563_200;

fn at(state: &State) -> view::Scene {
    let assets = view::Assets {
        root: PathBuf::new(),
        version: "test".into(),
        metrics: BTreeMap::new(),
        artwork: crate::app_artwork::Service::new(false),
    };
    let navigation = view::navigation(&assets, state.preferences.language);
    view::scene_with_pill(
        state,
        &assets,
        navigation.items[state.page_index()].x,
        navigation.items[state.page_index()].w,
        time::OffsetDateTime::from_unix_timestamp(NOW).unwrap(),
    )
}

/// Home with CS2 Score and TV Shows in Today: a live match, and an episode at 21:00 on Netflix.
fn today_state() -> State {
    let mut state = ready();
    state.preferences.show_cs2 = true;
    state.preferences.show_shows = true;
    let feed = crate::cs2::parse_feed(&serde_json::json!({
        "updated": NOW,
        "matches": [{
            "id": "m1", "begin_at": "2026-11-01T19:00:00Z", "state": "live", "best_of": 3,
            "tournament": "IEM", "tier": 1, "score": [1, 0],
            "teams": [{"name": "Natus Vincere", "short": "NAVI"}, {"name": "FaZe Clan", "short": "FaZe"}],
            "maps": [{"map": "Mirage", "winner": null}]
        }]
    }))
    .unwrap();
    let (Some(command), _) = state.cs2.sync(true, true, NOW) else {
        panic!("a fetch is due");
    };
    state.cs2.accept(command.id, Ok(feed), NOW);
    state.shows.list = vec![Show {
        id: 7,
        title: "Severance".into(),
        poster: None,
        meta: String::new(),
        service: Some("Netflix"),
        app: Some("netflix".into()),
        episodes: vec![Episode {
            airs: NOW + 3600,
            season: Some(2),
            number: Some(5),
        }],
    }];
    state
}

fn texts(scene: &view::Scene) -> Vec<String> {
    scene
        .draw
        .iter()
        .filter_map(|draw| match draw {
            crate::renderer::Draw::Text(label, ..) => Some(label.clone()),
            _ => None,
        })
        .collect()
}

fn first_tile_y(scene: &view::Scene) -> Option<f32> {
    scene
        .controls
        .iter()
        .find(|c| matches!(c.target, Target::App(_)))
        .map(|c| c.rect.y)
}

#[test]
fn today_card_lists_its_lines_and_opens_the_match_or_the_app() {
    let mut state = today_state();
    let scene = at(&state);
    let texts = texts(&scene);
    assert!(texts.iter().any(|t| t == "TODAY"));
    assert!(texts.iter().any(|t| t == "NAVI 1–0 FaZe"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "LIVE · Mirage"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "Severance — S2E5"));
    assert!(texts.iter().any(|t| t == "Netflix · 9:00 PM"), "{texts:?}");
    let line = |target: &Target| {
        scene
            .controls
            .iter()
            .find(|c| c.target == *target)
            .map(|c| c.rect)
            .unwrap()
    };
    let game = line(&Target::TodayMatch("m1".into()));
    let episode = line(&Target::TodayShow(7));
    assert!(game.x >= 1160. && game.y + game.h <= 400.);
    assert!(episode.y > game.y, "the live match comes first");

    state.focus = Target::TodayMatch("m1".into());
    state.activate();
    assert!(matches!(
        &state.overlay,
        Some(crate::state::Overlay::Cs2Match { id, restore })
            if id == "m1" && *restore == Target::TodayMatch("m1".into())
    ));
    state.overlay = None;
    state.focus = Target::TodayShow(7);
    assert_eq!(state.activate(), vec![Effect::Launch("netflix".into())]);
}

#[test]
fn no_today_lines_hide_the_card_without_moving_favorites() {
    let with = today_state();
    let without = ready();
    let (with, without) = (at(&with), at(&without));
    assert!(texts(&with).iter().any(|t| t == "TODAY"));
    assert!(!texts(&without).iter().any(|t| t == "TODAY"));
    assert_eq!(first_tile_y(&with), first_tile_y(&without));
}

#[test]
fn up_from_the_first_favorite_reaches_today_then_the_tabs() {
    let mut state = today_state();
    let first = state.apps()[0].id.clone();
    state.focus = Target::App(first.clone());
    state.direction(Direction::Up, &at(&state).controls);
    assert_eq!(state.focus, Target::TodayShow(7), "the card's last line");
    state.direction(Direction::Up, &at(&state).controls);
    assert_eq!(state.focus, Target::TodayMatch("m1".into()));
    state.direction(Direction::Up, &at(&state).controls);
    assert_eq!(state.focus, Target::Nav(0));
    // Without lines, Up from a favorite reaches the tabs.
    let mut state = ready();
    state.focus = Target::App(first);
    state.direction(Direction::Up, &at(&state).controls);
    assert_eq!(state.focus, Target::Nav(0));
}

#[test]
fn a_focused_match_that_leaves_the_feed_returns_focus_to_favorites() {
    let mut state = today_state();
    state.focus = Target::TodayMatch("m1".into());
    // The match is over and the episode has aired yesterday: the card has nothing left.
    state.preferences.show_cs2 = false;
    state.shows.list.clear();
    let controls = at(&state).controls;
    state.reveal_focus(&controls);
    assert!(matches!(state.focus, Target::App(_)), "{:?}", state.focus);
}

#[test]
fn format_compact_keeps_four_coins_on_one_line() {
    use crate::crypto::format_compact;
    assert_eq!(format_compact(77_881.97), "77.9K");
    assert_eq!(format_compact(2_262.98), "2,263");
    assert_eq!(format_compact(94.3), "94.30");
    assert_eq!(format_compact(0.00001042), "0.00001042");
    let mut state = ready();
    state.preferences.show_crypto = true;
    state.preferences.crypto_beside = true;
    state.preferences.crypto_coins = ["BTC", "ETH", "SOL", "DOGE"].map(String::from).to_vec();
    let scene = at(&state);
    let coins: Vec<_> = scene
        .draw
        .iter()
        .filter_map(|draw| match draw {
            crate::renderer::Draw::Text(label, rect, ..)
                if ["BTC", "ETH", "SOL", "DOGE"]
                    .iter()
                    .any(|c| label.starts_with(c)) =>
            {
                Some(*rect)
            }
            _ => None,
        })
        .collect();
    assert_eq!(coins.len(), 4);
    let y = coins[0].y;
    assert!(coins.iter().all(|r| r.y == y), "one line");
    assert!(
        coins
            .iter()
            .all(|r| r.x >= 1160. && r.x + r.w <= 1840. - 24.)
    );
}
