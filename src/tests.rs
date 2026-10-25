use crate::{
    model::{App, Input, InputMode, Preferences, demo_inventory},
    platform::{normalize_apps, normalize_inputs},
    state::{Direction, Effect, Overlay, Page, State, Target},
    storage, view,
};
use serde_json::json;
use std::{collections::BTreeMap, path::PathBuf};

#[path = "recents-tests.rs"]
mod recent_tests;

fn ready() -> State {
    let mut state = State::new(Preferences::default());
    state.set_inventory(demo_inventory(), None);
    state
}

fn scene(state: &State) -> view::Scene {
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
        time::OffsetDateTime::UNIX_EPOCH,
    )
}

fn controls(state: &State) -> Vec<crate::state::Control> {
    scene(state).controls
}

#[test]
fn favorites_edge_fragments_share_draw_hit_and_wheel_viewports() {
    let mut state = ready();
    let favorites: Vec<_> = state
        .inventory
        .apps
        .iter()
        .take(9)
        .map(|app| app.id.clone())
        .collect();
    state.preferences.favorites = Some(favorites.clone());
    assert_eq!(
        state.hit_test(&controls(&state), 1860., 600.),
        Some(Target::App(favorites[6].clone()))
    );
    assert!(state.wheel_scroll(&controls(&state), Some((1860., 600.)), 0, -1));
    assert!((state.home_scroll - crate::state::HOME_TILE_PITCH).abs() < 0.01);
    assert_eq!(state.scroll, 0.);
    assert_eq!(
        state.hit_test(&controls(&state), 40., 600.),
        Some(Target::App(favorites[0].clone()))
    );
    let scroll = state.home_scroll;
    state.pointer_focus(Target::App(favorites[0].clone()));
    assert_eq!(state.home_scroll, scroll);
    assert!(state.hit_test(&controls(&state), 4., 600.).is_none());
    assert!(state.hit_test(&controls(&state), 1915., 600.).is_none());
    assert!(state.hit_test(&controls(&state), 40., 1060.).is_none());
    let scene = scene(&state);
    let viewport = state.favorites_viewport();
    assert!(scene.draw.iter().any(|draw| matches!(draw, crate::renderer::Draw::Clip(Some(rect)) if rect.x == viewport.x && rect.w == viewport.w)));
    assert!(state.wheel_scroll(&scene.controls, Some((40., 600.)), 0, 1));
    assert_eq!(state.home_scroll, 0.);
    assert_eq!(state.scroll, 0.);
}

#[test]
fn widening_favorites_does_not_widen_apps_or_unclip_offscreen_rows() {
    let mut state = ready();
    state.open(1);
    let control = crate::state::Control {
        target: Target::App("sample".into()),
        rect: crate::state::Rect::new(0., 500., 1920., 600.),
        scrollable: true,
    };
    let controls = [control];
    assert!(state.hit_test(&controls, 40., 600.).is_none());
    assert!(state.hit_test(&controls, 1860., 600.).is_none());
    assert!(state.hit_test(&controls, 100., 1000.).is_none());
    assert_eq!(
        state.hit_test(&controls, 100., 600.),
        Some(Target::App("sample".into()))
    );
    assert!(!state.wheel_scroll(&controls, Some((1860., 600.)), 0, -1));
    assert_eq!(state.home_scroll, 0.);
    assert_eq!(state.scroll, 90.);
}

#[test]
fn remote_routes_from_header_and_back_through_settings() {
    let mut state = ready();
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::App("netflix".into()));
    // Up climbs through the grid's blocks above the favourites, then reaches the header.
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::WeatherConfigure);
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(
        state.focus,
        Target::Block(crate::home_grid::BlockKind::Greeting)
    );
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::Nav(0));
    state.open(2);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Sidebar(0));
    state.activate();
    assert_eq!(state.focus, Target::Setting(0));
    assert!(state.back().is_empty());
    assert_eq!(state.focus, Target::Sidebar(0));
    state.back();
    assert_eq!(state.focus, Target::Nav(2));
    state.back();
    assert_eq!(state.page, Page::Home);
    assert_eq!(
        state.back(),
        vec![Effect::Recent {
            generation: 1,
            command: crate::recents::Command::List
        }]
    );
    assert!(state.recents_open());
}

#[test]
fn hiding_restores_nearest_app_and_hidden_filter_can_restore_it() {
    let mut state = ready();
    state.open(1);
    state.focus = Target::App("netflix".into());
    state.options();
    state.focus = Target::Option(1);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.hidden.contains(&"netflix".into()));
    assert_eq!(state.focus, Target::App("youtube.leanback.v4".into()));
    state.filter = 5;
    assert_eq!(state.apps().len(), 1);
    state.focus = Target::App("netflix".into());
    state.options();
    state.focus = Target::Option(1);
    state.activate();
    assert!(state.preferences.hidden.is_empty());
    assert_eq!(state.focus, Target::App("netflix".into()));
    assert_eq!(state.filter, 0);
}

#[test]
fn reorder_persists_order_and_keeps_selected_identity() {
    let mut state = ready();
    state.focus = Target::App("netflix".into());
    state.hold();
    // The favourites' block menu offers reordering first.
    assert_eq!(state.activate(), vec![]);
    assert_eq!(
        state.direction(Direction::Right, &controls(&state)),
        vec![Effect::Save]
    );
    assert_eq!(
        &state.preferences.favorites(&state.inventory.apps)[..2],
        &["youtube.leanback.v4", "netflix"]
    );
    assert_eq!(state.focus, Target::App("netflix".into()));
    assert!(state.back().is_empty());
    assert!(state.moving.is_none());
}

#[test]
fn keyboard_is_modal_and_back_preserves_then_clears_search() {
    let mut state = ready();
    state.open(1);
    state.focus = Target::Search;
    state.activate();
    assert!(
        controls(&state)
            .iter()
            .all(|c| matches!(c.target, Target::Key(_) | Target::CloseDialog))
    );
    state.text("netflix");
    state.delete();
    state.text("x");
    state.back();
    assert_eq!(state.query, "netflix");
    assert_eq!(state.focus, Target::Search);
    assert_eq!(state.apps().len(), 1);
    state.back();
    assert!(state.query.is_empty());
    assert_eq!(state.focus, Target::Nav(1));
    assert_eq!(state.page, Page::Apps);
}

#[test]
fn partial_refresh_preserves_valid_inputs_and_never_invents_tv_apps() {
    let mut state = ready();
    state.set_inventory(
        crate::model::Inventory::default(),
        Some("Inputs unavailable".into()),
    );
    assert!(state.inventory.apps.is_empty());
    assert_eq!(state.inventory.inputs.len(), 3);
    assert_eq!(state.first_content(), Target::Browse);
}

#[test]
fn inventory_filters_system_apps_duplicates_and_unsafe_identifiers() {
    let apps = normalize_apps(&json!([
        {"id":"netflix","title":" Netflix\n"}, {"id":"netflix","title":"Duplicate"},
        {"id":"bad;command","title":"Bad"}, {"id":"com.webos.app.hidden","title":"Internal"},
        {"id":"com.webos.app.livetv","visible":false,"title":"Live TV"},
        {"id":"com.luma.launcher.native","visible":true},
        {"id":"com.webos.app.hdmi1","visible":true},
        {"id":"hidden","visible":"false"}
    ]))
    .unwrap();
    assert_eq!(apps.len(), 2);
    assert_eq!(
        apps.iter().find(|a| a.id == "netflix").unwrap().title,
        "Netflix"
    );
    assert!(normalize_apps(&json!({"apps":[]})).is_err());
    let inputs = normalize_inputs(&json!([
        {"appId":"com.webos.app.hdmi1","id":"HDMI_1","port":1,"connected":true},
        {"appId":"com.webos.app.hdmi2","id":"HDMI_4"},
        {"appId":"com.webos.app.hdmi3"}, {"appId":"com.webos.app.hdmi3"},
        {"appId":"com.webos.app.hdmi4","label":"Console"}
    ]))
    .unwrap();
    assert_eq!(inputs.len(), 2);
    assert_eq!(inputs[1].connected, None);
}

#[test]
fn inventory_visibility_cases_match_the_launcher_filters() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/fixtures/app-visibility.json")).unwrap();
    let inventory: Vec<_> = cases.iter().map(|case| case["app"].clone()).collect();
    let actual: std::collections::BTreeSet<_> = normalize_apps(&json!(inventory))
        .unwrap()
        .into_iter()
        .map(|app| app.id)
        .collect();
    let expected: std::collections::BTreeSet<_> = cases
        .iter()
        .filter(|case| case["include"] == true)
        .map(|case| case["app"]["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn asset_loading_and_unknown_app_cards_do_not_require_collected_artwork() {
    let root = std::env::temp_dir().join(format!("luma-assets-no-dumps-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("version.txt"), "test").unwrap();
    std::fs::write(root.join("font-metrics.json"), "{}").unwrap();
    for webos in [false, true] {
        let assets = view::Assets::load(root.clone(), webos).unwrap();
        let mut state = State::new(Preferences::default());
        state.set_inventory(
            crate::model::Inventory {
                apps: vec![App::new("com.example.regional", "Regional App")],
                inputs: vec![],
            },
            None,
        );
        state.open(1);
        let scene = view::scene_with_pill(
            &state,
            &assets,
            800.,
            100.,
            time::OffsetDateTime::UNIX_EPOCH,
        );
        assert!(scene.draw.iter().any(|draw| matches!(draw,
            crate::renderer::Draw::TintedImage(path, ..) if path.ends_with("ui-icons/generic.lvec"))));
        assert!(scene.draw.iter().any(|draw| matches!(draw,
            crate::renderer::Draw::Text(value, ..) if value == "Regional App")));
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn auto_input_visibility_requires_a_positive_connection_report() {
    let mut preferences = Preferences::default();
    let mut input = Input {
        id: "com.webos.app.hdmi1".into(),
        title: "Console".into(),
        port: "HDMI 1".into(),
        connected: None,
    };
    assert!(!preferences.input_visible(&input));
    input.connected = Some(true);
    assert!(preferences.input_visible(&input));
    preferences
        .input_modes
        .insert(input.id.clone(), InputMode::Off);
    assert!(!preferences.input_visible(&input));
    preferences
        .input_modes
        .insert(input.id.clone(), InputMode::On);
    input.connected = Some(false);
    assert!(preferences.input_visible(&input));
}

#[test]
fn preferences_round_trip_and_corrupt_data_is_not_silently_overwritten() {
    let directory = std::env::temp_dir().join(format!("luma-native-test-{}", std::process::id()));
    let path = directory.join("preferences.json");
    let mut preferences = Preferences {
        favorites: Some(vec![]),
        background: "forest".into(),
        ..Preferences::default()
    };
    preferences
        .input_modes
        .insert("com.webos.app.hdmi1".into(), InputMode::Off);
    storage::save(&path, &preferences).unwrap();
    let loaded = storage::load(&path).unwrap();
    assert_eq!(loaded.favorites, Some(vec![]));
    assert_eq!(loaded.background, "forest");
    assert_eq!(loaded.input_modes["com.webos.app.hdmi1"], InputMode::Off);
    std::fs::write(&path, "broken").unwrap();
    assert!(storage::load(&path).is_err());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn gallery_and_grid_navigation_scroll_to_keep_focus_visible() {
    let mut state = ready();
    state.open(2);
    state.section = crate::state::BACKGROUND_SECTION;
    state.focus = Target::Background(0);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Background(3));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Background(6));
    assert_eq!(
        state.scroll, 0.,
        "the web-sized gallery fits all three rows"
    );
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(
        state.focus,
        Target::Sidebar(crate::state::BACKGROUND_SECTION)
    );
    state.open(1);
    let last = state.apps().last().unwrap().id.clone();
    state.focus(Target::App(last), &controls(&state));
    assert!(
        state.scroll > 0.,
        "library focus includes its wrapped caption"
    );
}

#[test]
fn cancelling_options_preserves_identity_and_does_not_save() {
    let mut state = ready();
    state.focus = Target::App("netflix".into());
    state.options();
    assert!(matches!(state.overlay, Some(Overlay::Options { .. })));
    state.focus = Target::CloseDialog;
    assert!(state.activate().is_empty());
    assert_eq!(state.focus, Target::App("netflix".into()));
    assert!(state.overlay.is_none());
}

#[test]
fn pointer_focus_does_not_scroll_the_clicked_control() {
    let mut state = ready();
    state.open(1);
    state.scroll = 150.;
    state.pointer_focus(Target::App("netflix".into()));
    assert_eq!(state.scroll, 150.);
    state.focus(Target::Nav(1), &controls(&state));
    assert_eq!(state.scroll, 0.);
}

#[test]
fn gallery_right_crosses_rows_in_gallery_order() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    state.focus = Target::Background(2);
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::Background(3));
}

#[test]
fn reordering_skips_hidden_and_unavailable_favorite_slots() {
    let mut state = ready();
    state.preferences.favorites = Some(vec![
        "netflix".into(),
        "missing".into(),
        "youtube.leanback.v4".into(),
        "amazon".into(),
    ]);
    state.preferences.hidden.push("youtube.leanback.v4".into());
    state.focus = Target::App("netflix".into());
    state.hold();
    state.activate();
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(
        state.preferences.favorites.unwrap(),
        vec!["amazon", "missing", "youtube.leanback.v4", "netflix"]
    );
    assert_eq!(state.focus, Target::App("netflix".into()));
}

#[test]
fn focused_navigation_tabs_switch_pages_immediately() {
    let mut state = ready();
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.page, Page::Apps);
    assert_eq!(state.focus, Target::Nav(1));
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.page, Page::Settings);
    assert_eq!(state.focus, Target::Nav(2));
}

#[test]
fn greeting_subpage_opens_the_keyboard_and_back_restores_its_parent() {
    let mut state = ready();
    state.open(2);
    state.focus = Target::Setting(6);
    assert!(
        state
            .direction(Direction::Right, &controls(&state))
            .is_empty()
    );
    assert_eq!(state.subpage, Some(crate::state::Subpage::Greeting));
    assert_eq!(state.focus, Target::GreetingName);
    assert!(!state.text_input_active());
    assert!(state.activate().is_empty());
    assert!(state.text_input_active());
    state.text("Ana");
    assert_eq!(state.editing_text(), "Ana");
    assert!(state.preferences.greeting_name.is_empty());
    assert_eq!(state.finish_text_input(), vec![Effect::Save]);
    assert_eq!(state.focus, Target::GreetingName);
    assert!(state.finish_text_input().is_empty());
    assert_eq!(
        crate::i18n::Language::En.greeting(9, &state.preferences.greeting_name),
        "Good morning, Ana."
    );
    assert_eq!(state.subpage, Some(crate::state::Subpage::Greeting));
    assert!(state.back().is_empty());
    assert!(state.subpage.is_none());
    assert_eq!(state.focus, Target::Setting(6));
}

#[test]
fn greeting_name_replacement_commits_on_every_keyboard_completion_route() {
    for route in 0..3 {
        let mut state = ready();
        state.preferences.greeting_name = "Ana".into();
        state.open(2);
        state.focus = Target::Setting(6);
        state.activate();
        state.activate();
        assert!(state.editing_text().is_empty());
        state.text("  BobX ");
        state.delete_surrounding_text(-3, 3);
        assert_eq!(state.editing_text(), "  Bo");
        state.text("b  ");
        assert_eq!(state.preferences.greeting_name, "Ana");
        let effects = match route {
            0 => {
                state.overlay = Some(Overlay::SystemKeyboard);
                state.finish_text_input()
            }
            1 => state.back(),
            _ => {
                state.focus = Target::Key(crate::state::KEYS.len() - 1);
                state.activate()
            }
        };
        assert_eq!(effects, vec![Effect::Save]);
        assert_eq!(state.preferences.greeting_name, "Bob");
        assert_eq!(state.focus, Target::GreetingName);
        assert!(state.finish_text_input().is_empty());
        state.activate();
        state.text("   ");
        assert!(state.back().is_empty());
        assert_eq!(state.preferences.greeting_name, "Bob");
    }
}

#[test]
fn remote_can_clear_a_saved_greeting_name_without_moving_the_controls() {
    let mut state = ready();
    state.preferences.greeting_name = "Ana".into();
    state.open(2);
    state.focus = Target::Setting(6);
    state.activate();
    let before: Vec<_> = controls(&state)
        .iter()
        .map(|c| (c.target.clone(), c.rect.y))
        .collect();
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::GreetingNameClear);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.greeting_name.is_empty());
    assert_eq!(state.focus, Target::GreetingName);
    let after: Vec<_> = controls(&state)
        .iter()
        .map(|c| (c.target.clone(), c.rect.y))
        .collect();
    assert_eq!(before, after);
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::Setting(6));
    assert!(state.subpage.is_none());
}

#[test]
fn app_name_settings_use_independent_defaults_including_with_legacy_preferences() {
    let old: Preferences = serde_json::from_value(json!({"show_names": true})).unwrap();
    assert!(!old.show_home_names);
    assert!(old.show_app_names);
    let mut state = ready();
    assert!(!state.preferences.show_home_names);
    assert!(state.preferences.show_app_names);
    state.open(2);
    state.focus = Target::Setting(0);
    assert!(
        state
            .direction(Direction::Right, &controls(&state))
            .is_empty()
    );
    assert_eq!(state.subpage, Some(crate::state::Subpage::AppNames));
    assert_eq!(state.focus, Target::AppNamesHome);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.show_home_names);
    assert!(state.preferences.show_app_names);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::AppNamesApps);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.show_home_names);
    assert!(!state.preferences.show_app_names);
    assert_eq!(state.subpage, Some(crate::state::Subpage::AppNames));
    let restored: Preferences =
        serde_json::from_value(serde_json::to_value(&state.preferences).unwrap()).unwrap();
    assert!(restored.show_home_names);
    assert!(!restored.show_app_names);
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::Setting(0));
    assert!(state.subpage.is_none());
}

#[test]
fn home_and_apps_render_only_their_own_caption_preference() {
    let mut state = ready();
    for (home, apps) in [(false, true), (true, false), (true, true), (false, false)] {
        state.preferences.show_home_names = home;
        state.preferences.show_app_names = apps;
        for (page, expected) in [(0, home), (1, apps)] {
            state.open(page);
            let scene = scene(&state);
            let caption = scene.draw.iter().any(
                |draw| matches!(draw, crate::renderer::Draw::Text(value, ..) if value == "Netflix"),
            );
            assert_eq!(caption, expected, "Home={home}, Apps={apps}, page={page}");
            let card = scene
                .controls
                .iter()
                .find(|c| c.target == Target::App("netflix".into()))
                .unwrap();
            assert_eq!(
                card.rect.h > 154.4,
                expected,
                "Hidden captions must not reserve space"
            );
        }
    }
}

#[test]
fn input_name_actions_only_highlight_the_focused_row_and_keep_the_saved_mode() {
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state
        .preferences
        .set_input_name("com.webos.app.hdmi1", "Console");
    state.open(2);
    state.section = crate::state::INPUTS_SECTION;
    state.focus = Target::InputMode("com.webos.app.hdmi1".into());
    state.activate();
    for target in [Target::InputRename, Target::InputNameReset] {
        state.pointer_focus(target.clone());
        let scene = scene(&state);
        let highlighted: Vec<_> = scene
            .draw
            .iter()
            .filter_map(|draw| match draw {
                crate::renderer::Draw::RoundedGradient(rect, ..)
                    if rect.x == 640. && rect.w == 1120. =>
                {
                    Some(rect.y)
                }
                _ => None,
            })
            .collect();
        let focused = scene.controls.iter().find(|c| c.target == target).unwrap();
        assert_eq!(highlighted, vec![focused.rect.y]);
        assert!(scene.draw.iter().any(|draw| matches!(draw, crate::renderer::Draw::TintedImage(path, ..) if path.ends_with("check.lvec"))));
        assert_eq!(
            state
                .preferences
                .input_modes
                .get("com.webos.app.hdmi1")
                .copied()
                .unwrap_or_default(),
            InputMode::Auto
        );
        assert!(scene.draw.iter().any(
            |draw| matches!(draw, crate::renderer::Draw::Text(value, ..) if value == "Clear name")
        ));
    }
}

#[test]
fn clock_choices_only_highlight_focus_while_preserving_saved_checkmarks() {
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.preferences.show_seconds = true;
    state.open(2);
    state.focus = Target::Setting(1);
    state.activate();
    state.activate();
    for index in 0..2 {
        state.pointer_focus(Target::Choice(index));
        let scene = scene(&state);
        let highlighted: Vec<_> = scene
            .draw
            .iter()
            .filter_map(|draw| match draw {
                crate::renderer::Draw::RoundedGradient(rect, ..)
                    if rect.x == 640. && rect.w == 1120. =>
                {
                    Some(rect.y)
                }
                _ => None,
            })
            .collect();
        let focused = scene
            .controls
            .iter()
            .find(|c| c.target == Target::Choice(index))
            .unwrap();
        assert_eq!(highlighted, vec![focused.rect.y]);
        let checks = scene.draw.iter().filter(|draw| matches!(draw, crate::renderer::Draw::TintedImage(path, ..) if path.ends_with("check.lvec"))).count();
        assert_eq!(checks, 1);
        assert!(!state.preferences.hour_24);
        assert!(state.preferences.show_seconds);
    }
}

#[test]
fn clock_format_subpage_saves_and_restores_its_parent_row() {
    let mut state = ready();
    state.open(2);
    state.focus = Target::Setting(1);
    assert!(
        state
            .direction(Direction::Right, &controls(&state))
            .is_empty()
    );
    assert_eq!(state.subpage, Some(crate::state::Subpage::Clock));
    assert_eq!(state.focus, Target::ClockFormat);
    assert!(
        state
            .direction(Direction::Right, &controls(&state))
            .is_empty()
    );
    assert_eq!(state.subpage, Some(crate::state::Subpage::ClockFormat));
    assert_eq!(state.focus, Target::Choice(0));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.hour_24);
    assert!(!state.preferences.show_seconds);
    assert_eq!(state.subpage, Some(crate::state::Subpage::Clock));
    assert_eq!(state.focus, Target::ClockFormat);
    state.direction(Direction::Left, &controls(&state));
    assert!(state.subpage.is_none());
    assert_eq!(state.focus, Target::Setting(1));
    state.activate();
    state.activate();
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.subpage, Some(crate::state::Subpage::Clock));
    assert_eq!(state.focus, Target::ClockFormat);
    state.back();
    assert_eq!(state.focus, Target::Setting(1));
    assert!(state.subpage.is_none());
}

#[test]
fn time_format_selects_seconds_without_changing_the_hour_format() {
    let mut state = ready();
    state.preferences.hour_24 = true;
    state.open(2);
    state.focus = Target::Setting(1);
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.subpage, Some(crate::state::Subpage::Clock));
    assert!(!state.preferences.show_seconds);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::ClockTimeFormat);
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.subpage, Some(crate::state::Subpage::TimeFormat));
    assert_eq!(state.focus, Target::Choice(0));
    assert!(
        !controls(&state)
            .iter()
            .any(|c| c.target == Target::Choice(2))
    );
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Choice(1));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.show_seconds);
    assert!(state.preferences.hour_24);
    assert_eq!(state.subpage, Some(crate::state::Subpage::Clock));
    assert_eq!(state.focus, Target::ClockTimeFormat);
    state.activate();
    assert_eq!(state.focus, Target::Choice(1));
    state.back();
    assert_eq!(state.subpage, Some(crate::state::Subpage::Clock));
    assert_eq!(state.focus, Target::ClockTimeFormat);
    state.activate();
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(!state.preferences.show_seconds);
    assert!(state.preferences.hour_24);
    state.back();
    assert!(state.subpage.is_none());
    assert_eq!(state.focus, Target::Setting(1));
}

#[test]
fn system_category_owns_refresh_and_quit_and_restores_sidebar_focus() {
    let mut state = ready();
    state.open(2);
    assert!(
        !controls(&state)
            .iter()
            .any(|c| matches!(c.target, Target::Setting(3 | 5)))
    );
    assert!(
        controls(&state)
            .iter()
            .any(|c| c.target == Target::Sidebar(crate::state::SYSTEM_SECTION))
    );
    state.focus = Target::Sidebar(crate::state::SYSTEM_SECTION - 1);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Sidebar(crate::state::SYSTEM_SECTION));
    assert_eq!(state.section, crate::state::SYSTEM_SECTION);
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::Setting(3));
    assert_eq!(state.activate(), vec![Effect::Reload]);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Setting(5));
    assert_eq!(state.activate(), vec![Effect::Quit]);
    state.back();
    assert_eq!(state.focus, Target::Sidebar(crate::state::SYSTEM_SECTION));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Sidebar(0));
    assert_eq!(state.section, 0);
}

#[test]
fn input_visibility_has_explicit_choices_and_returns_to_the_same_input() {
    let mut state = ready();
    state.open(2);
    state.section = crate::state::INPUTS_SECTION;
    let id = "com.webos.app.hdmi1".to_string();
    state.focus = Target::InputMode(id.clone());
    state.activate();
    assert_eq!(state.focus, Target::Choice(2));
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(state.preferences.input_modes[&id], InputMode::Off);
    assert_eq!(state.focus, Target::InputMode(id.clone()));
    assert!(!state.preferences.input_visible(&state.inventory.inputs[0]));
    state.activate();
    state.back();
    assert_eq!(state.focus, Target::InputMode(id));
    assert!(state.subpage.is_none());
}

#[test]
fn background_entry_restores_the_applied_background() {
    let mut state = ready();
    state.open(2);
    state.preferences.background = "forest".into();
    state.focus = Target::Sidebar(crate::state::BACKGROUND_SECTION);
    state.section = crate::state::BACKGROUND_SECTION;
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::Background(6));
    state.back();
    state.activate();
    assert_eq!(state.focus, Target::Background(6));
}

#[test]
fn categories_and_search_have_clear_actions_and_conditional_hidden_control() {
    let mut state = ready();
    state.open(1);
    assert!(
        !controls(&state)
            .iter()
            .any(|c| c.target == Target::Filter(5))
    );
    state.focus = Target::Search;
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::Filter(4));
    state.preferences.hidden.push("netflix".into());
    state.focus = Target::Search;
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::Filter(5));
    state.activate();
    assert_eq!(state.apps().len(), 1);
    state.focus = Target::ClearFilter;
    state.activate();
    assert_eq!(state.filter, 0);
    assert_eq!(state.focus, Target::Filter(1));
    state.query = "YouTube".into();
    state.focus = Target::ClearSearch;
    state.activate();
    assert!(state.query.is_empty());
    assert_eq!(state.focus, Target::Search);
}

#[test]
fn a_notice_does_not_escape_modal_focus_and_closing_restores_the_app() {
    let mut state = ready();
    state.open(1);
    state.notify("Saved");
    state.focus = Target::App("netflix".into());
    state.options();
    assert!(
        controls(&state)
            .iter()
            .all(|c| matches!(c.target, Target::Option(_) | Target::CloseDialog))
    );
    state.focus = Target::Option(1);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Option(1));
    state.back();
    assert_eq!(state.focus, Target::App("netflix".into()));
}

#[test]
fn remote_toolbar_focus_normalizes_scroll_but_hover_does_not() {
    let mut state = ready();
    state.open(1);
    state.scroll = 100.;
    state.pointer_focus(Target::Filter(1));
    assert_eq!(state.scroll, 100.);
    state.focus(Target::Filter(2), &controls(&state));
    assert_eq!(state.scroll, 0.);
}

#[test]
fn long_captions_expand_rows_without_overlapping_the_next_row() {
    let mut state = ready();
    state.open(1);
    state.inventory.apps.iter_mut().find(|app| app.id == "netflix").unwrap().title = "A very long installed application title that needs more than two lines to remain fully readable".into();
    let targets = controls(&state);
    let title = targets
        .iter()
        .find(|c| c.target == Target::App("netflix".into()))
        .unwrap();
    let next_id = state.apps()[6].id.clone();
    let next = targets
        .iter()
        .find(|c| c.target == Target::App(next_id.clone()))
        .unwrap();
    assert!(title.rect.h > 240.);
    assert!(next.rect.y > title.rect.y + title.rect.h);
}

#[test]
fn reduced_motion_disables_panel_wallpaper_and_notification_animation() {
    let mut state = ready();
    state.preferences.reduced_motion = true;
    state.open(2);
    state.focus = Target::Setting(1);
    state.activate();
    assert!(!state.animating());
    state.back();
    state.focus = Target::Background(6);
    state.activate();
    state.notify("Saved");
    assert!(!state.animating());
}

#[test]
fn pinning_a_hidden_app_restores_it_and_leaves_hidden_when_empty() {
    let mut state = ready();
    state.open(1);
    state.filter = 5;
    state.preferences.hidden.push("org.jellyfin.webos".into());
    state.focus = Target::App("org.jellyfin.webos".into());
    state.options();
    state.activate();
    assert!(state.preferences.hidden.is_empty());
    assert_eq!(state.filter, 0);
    assert!(
        state
            .preferences
            .favorites(&state.inventory.apps)
            .contains(&"org.jellyfin.webos".into())
    );
    assert_eq!(state.focus, Target::App("org.jellyfin.webos".into()));
}

#[test]
fn weather_location_keyboard_back_and_sidebar_routes_restore_focus() {
    let mut state = ready();
    state.open(2);
    state.focus = Target::Sidebar(crate::state::WEATHER_SECTION);
    state.section = crate::state::WEATHER_SECTION;
    state.activate();
    assert_eq!(state.focus, Target::WeatherToggle);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::WeatherLocation);
    state.activate();
    assert_eq!(state.focus, Target::WeatherQuery);
    state.activate();
    state.text("Vienna");
    state.back();
    assert_eq!(state.focus, Target::WeatherSearch);
    assert_eq!(state.weather.query, "Vienna");
    assert_eq!(state.subpage, Some(crate::state::Subpage::WeatherLocation));
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::WeatherLocation);
    assert!(state.subpage.is_none());
}

#[test]
fn weather_units_save_without_replacing_celsius_cache_and_hide_keeps_city() {
    let mut state = ready();
    state.open(2);
    state.section = crate::state::WEATHER_SECTION;
    let place = crate::weather::Location {
        name: "Vienna".into(),
        region: "Vienna".into(),
        country: "Austria".into(),
        latitude: 48.2085,
        longitude: 16.3721,
    };
    state.preferences.weather_location = Some(place.clone());
    state.weather.cache = Some(crate::weather::Cache {
        key: place.key(),
        updated_at: 1000,
        reading: crate::weather::Reading {
            temperature: 14.2,
            code: 2,
            is_day: true,
        },
    });
    state.focus = Target::WeatherTemperature;
    state.activate();
    assert_eq!(state.focus, Target::Choice(0));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(
        state.preferences.temperature_unit,
        crate::weather::Unit::Fahrenheit
    );
    assert_eq!(
        state.weather.cache.as_ref().unwrap().reading.temperature,
        14.2
    );
    assert_eq!(state.focus, Target::WeatherTemperature);
    state.focus = Target::WeatherToggle;
    state.activate();
    assert!(!state.preferences.show_weather);
    assert_eq!(state.preferences.weather_location, Some(place));
}

#[test]
fn removing_weather_location_clears_cache_and_cancels_pending_results() {
    let mut state = ready();
    state.open(2);
    state.section = crate::state::WEATHER_SECTION;
    state.focus = Target::WeatherLocation;
    state.activate();
    state.focus = Target::WeatherClear;
    let effects = state.activate();
    assert_eq!(
        effects,
        vec![
            Effect::Save,
            Effect::Weather(crate::weather::Command::ClearCache)
        ]
    );
    assert!(state.weather.cache.is_none());
    assert!(state.preferences.weather_location.is_none());
    assert_eq!(state.focus, Target::WeatherSearch);
}

#[test]
fn language_selection_restores_its_parent_and_is_saved_with_preferences() {
    let mut state = ready();
    state.open(2);
    state.focus = Target::Setting(4);
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.subpage, Some(crate::state::Subpage::Language));
    assert_eq!(state.focus, Target::Choice(0));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(state.preferences.language, crate::i18n::Language::De);
    assert_eq!(state.focus, Target::Setting(4));
    state.activate();
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::Setting(4));
}

#[test]
fn remote_can_select_and_restore_every_language() {
    for (index, language) in crate::i18n::Language::ALL.into_iter().enumerate() {
        let mut state = ready();
        state.open(2);
        state.focus = Target::Setting(4);
        state.activate();
        for _ in 0..index {
            state.direction(Direction::Down, &controls(&state));
        }
        assert_eq!(state.focus, Target::Choice(index));
        assert_eq!(state.activate(), vec![Effect::Save]);
        assert_eq!(state.preferences.language, language);
        assert_eq!(state.focus, Target::Setting(4));

        let saved = serde_json::to_value(&state.preferences).unwrap();
        let mut restored = State::new(serde_json::from_value(saved).unwrap());
        restored.open(2);
        restored.focus = Target::Setting(4);
        restored.activate();
        assert_eq!(restored.preferences.language, language);
        assert_eq!(restored.focus, Target::Choice(index));
        restored.direction(Direction::Down, &controls(&restored));
        assert_eq!(
            restored.focus,
            Target::Choice((index + 1) % crate::i18n::Language::ALL.len())
        );
    }
}

#[test]
fn unsupported_language_preferences_recover_without_losing_other_choices() {
    let preferences: Preferences = serde_json::from_value(
        json!({"language":"unknown","favorites":["netflix"],"show_home_names":true}),
    )
    .unwrap();
    assert_eq!(preferences.language, crate::i18n::Language::En);
    assert!(preferences.show_home_names);
    assert_eq!(preferences.favorites, Some(vec!["netflix".into()]));
}

#[test]
fn unrelated_app_ids_and_titles_keep_their_installed_artwork() {
    for (id, title) in [
        ("com.tvweb.dashboard", "Glasshouse"),
        ("com.example.ardent", "Notes"),
        ("com.example.3ard", "Notes"),
        ("com.example.ard3", "Notes"),
        ("com.example.plexus", "Notes"),
        ("com.example.media", "Standard dashboard"),
        ("com.example.media", "netflixer"),
        ("com.example.media", "preNetflix"),
        ("amazon.alexa.view", "Amazon Alexa"),
        ("com.example.spotify", "Spotify tools"),
        ("com.example.media", "Plex"),
        ("com.example.browser", "Web Browser"),
        ("com.example.livetv", "Live TV"),
    ] {
        let app = App::new(id, title);
        assert_eq!(
            (app.brand.as_str(), app.category.as_str()),
            ("generic", "Utilities"),
            "An unrelated application must not be rebranded: {id} / {title}"
        );
    }
}

#[test]
fn curated_branding_does_not_use_title_words_or_partial_id_matches() {
    for (id, title) in [
        ("com.example.ard_more", "Notes"),
        ("com.example.more-ard", "Notes"),
        ("com.example.ARD", "Notes"),
        ("com.example.media", "ARD"),
        ("com.example.media", "(ARD)"),
        ("com.example.media", "ARD미디어테크"),
        ("com.example.media", "아에르데ARD"),
        ("com.example.media", "아에르데ARD미디어테크"),
        ("com.example.dashboard", "ARD"),
        ("com.example.media", "standard ardent ARD"),
    ] {
        let app = App::new(id, title);
        assert_eq!(
            (app.brand.as_str(), app.category.as_str()),
            ("generic", "Utilities"),
            "Only an exact app ID may select curated artwork: {id} / {title}"
        );
    }
}

#[test]
fn known_app_ids_preserve_the_curated_brands_and_categories() {
    let apps = demo_inventory().apps;
    let expected = [
        ("com.webos.app.livetv", "live-tv", "Streaming"),
        ("netflix", "netflix", "Streaming"),
        ("youtube.leanback.v4", "youtube", "Streaming"),
        ("com.apple.appletv", "apple-tv", "Streaming"),
        ("com.disney.disneyplus-prod", "disney-plus", "Streaming"),
        ("amazon", "prime-video", "Streaming"),
        ("plex", "plex", "Streaming"),
        ("spotify-beehive", "spotify", "Music"),
        ("org.jellyfin.webos", "jellyfin", "Streaming"),
        ("org.xbmc.kodi", "kodi", "Streaming"),
        ("com.limelight.webos", "moonlight", "Gaming"),
        ("org.webosbrew.hbchannel", "homebrew", "Utilities"),
        ("com.webos.app.browser", "browser", "Utilities"),
        ("com.arte.tv", "arte", "Streaming"),
        ("de.zdf.app.zdfm3", "zdf", "Streaming"),
        ("ard.mediathek", "das-erste", "Streaming"),
    ];
    for (id, brand, category) in expected {
        let app = apps.iter().find(|app| app.id == id).unwrap();
        assert_eq!(
            (app.brand.as_str(), app.category.as_str()),
            (brand, category),
            "Known app {id} must retain its curated brand and category"
        );
    }
}

#[test]
fn hdmi_plug_in_reports_the_cable_state_when_connected_only_means_present() {
    let inputs = normalize_inputs(&json!([
        {"id":"HDMI_1","label":"HDMI 1","port":1,"connected":true,"appId":"com.webos.app.hdmi1","hdmiPlugIn":false},
        {"id":"HDMI_2","label":"AVR-X1800H","port":2,"connected":true,"appId":"com.webos.app.hdmi2","hdmiPlugIn":true},
        {"id":"HDMI_3","label":"HDMI 3","port":3,"connected":true,"appId":"com.webos.app.hdmi3","hdmiPlugIn":false},
        {"id":"HDMI_4","label":"HDMI 4","port":4,"connected":true,"appId":"com.webos.app.hdmi4","hdmiPlugIn":false}
    ]))
    .unwrap();
    let connected: Vec<_> = inputs
        .iter()
        .map(|input| (input.id.as_str(), input.connected))
        .collect();
    assert_eq!(
        connected,
        [
            ("com.webos.app.hdmi1", Some(false)),
            ("com.webos.app.hdmi2", Some(true)),
            ("com.webos.app.hdmi3", Some(false)),
            ("com.webos.app.hdmi4", Some(false)),
        ]
    );
    assert_eq!(inputs[1].title, "AVR-X1800H");
    let preferences = Preferences::default();
    let visible: Vec<_> = inputs
        .iter()
        .filter(|input| preferences.input_visible(input))
        .map(|input| input.id.as_str())
        .collect();
    assert_eq!(visible, ["com.webos.app.hdmi2"]);
}

#[test]
fn inputs_without_hdmi_plug_in_keep_the_connected_report() {
    let inputs = normalize_inputs(&json!([
        {"appId":"com.webos.app.hdmi1","id":"HDMI_1","port":1,"connected":true},
        {"appId":"com.webos.app.hdmi2","id":"HDMI_2","port":2,"connected":false},
        {"appId":"com.webos.app.hdmi3","connected":true,"hdmiPlugIn":"yes"},
        {"appId":"com.webos.app.hdmi4"}
    ]))
    .unwrap();
    let connected: Vec<_> = inputs.iter().map(|input| input.connected).collect();
    assert_eq!(connected, [Some(true), Some(false), Some(true), None]);
}

#[test]
fn input_names_are_sanitized_and_old_preferences_load_without_them() {
    let old: Preferences = serde_json::from_value(json!({"background": "forest"})).unwrap();
    assert!(old.input_names.is_empty());
    let mut preferences: Preferences = serde_json::from_value(json!({
        "input_names": {
            "com.webos.app.hdmi1": "  Play\nStation\t5  ",
            "com.webos.app.hdmi2": "   ",
            "bad;id": "Ignored",
            "com.webos.app.hdmi3": "A".repeat(60),
        }
    }))
    .unwrap();
    preferences.sanitize();
    assert_eq!(
        preferences.input_names,
        BTreeMap::from([
            (
                "com.webos.app.hdmi1".to_string(),
                "PlayStation5".to_string()
            ),
            (
                "com.webos.app.hdmi3".to_string(),
                "A".repeat(crate::model::INPUT_NAME_LIMIT)
            ),
        ])
    );
    assert!(preferences.set_input_name("com.webos.app.hdmi2", " DENON "));
    assert!(!preferences.set_input_name("com.webos.app.hdmi2", "DENON"));
    assert_eq!(preferences.input_names["com.webos.app.hdmi2"], "DENON");
    assert!(preferences.set_input_name("com.webos.app.hdmi2", ""));
    assert!(!preferences.input_names.contains_key("com.webos.app.hdmi2"));
    assert!(!preferences.set_input_name("bad;id", "Name"));
}

#[test]
fn custom_input_names_keep_the_label_port_and_status_as_detail() {
    let mut preferences = Preferences::default();
    let input = Input {
        id: "com.webos.app.hdmi2".into(),
        title: "AVR-X1800H".into(),
        port: "HDMI 2".into(),
        connected: Some(true),
    };
    let language = preferences.language;
    let status = |preferences: &Preferences| {
        view::input_status(&input, preferences.input_name(&input).is_some(), language)
    };
    assert_eq!(preferences.input_title(&input), "HDMI 2");
    assert_eq!(status(&preferences), "AVR-X1800H · Connected");
    preferences.set_input_name(&input.id, "DENON");
    assert_eq!(preferences.input_title(&input), "DENON");
    assert_eq!(status(&preferences), "AVR-X1800H · HDMI 2 · Connected");
    let plain = Input {
        title: "HDMI 2".into(),
        ..input.clone()
    };
    assert_eq!(
        view::input_status(&plain, true, language),
        "HDMI 2 · Connected"
    );
    assert_eq!(view::input_status(&plain, false, language), "Connected");
}

#[test]
fn remote_renames_and_resets_an_input_from_its_settings_page() {
    let mut state = ready();
    state.open(2);
    state.section = crate::state::INPUTS_SECTION;
    let id = "com.webos.app.hdmi2".to_string();
    state.focus = Target::InputMode(id.clone());
    state.activate();
    assert_eq!(state.focus, Target::Choice(2));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::InputRename);
    assert!(
        !controls(&state)
            .iter()
            .any(|c| c.target == Target::InputNameReset)
    );
    state.activate();
    assert!(matches!(state.overlay, Some(Overlay::Keyboard)));
    state.text("DENONX");
    state.delete();
    assert_eq!(state.back(), vec![Effect::Save]);
    assert_eq!(state.focus, Target::InputRename);
    assert_eq!(state.preferences.input_names[&id], "DENON");
    assert_eq!(
        state.preferences.input_title(&state.inventory.inputs[1]),
        "DENON"
    );
    // Dismissing an empty keyboard keeps the saved name.
    state.activate();
    assert!(state.finish_text_input().is_empty());
    assert_eq!(state.preferences.input_names[&id], "DENON");
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::InputNameReset);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.input_names.is_empty());
    assert_eq!(state.focus, Target::InputRename);
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::Choice(2));
    state.focus = Target::InputRename;
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::InputMode(id));
    assert!(state.subpage.is_none());
}

#[test]
fn keyboard_done_saves_a_typed_input_name() {
    let mut state = ready();
    state.open(2);
    state.section = crate::state::INPUTS_SECTION;
    state.focus = Target::InputMode("com.webos.app.hdmi1".into());
    state.activate();
    state.focus = Target::InputRename;
    state.activate();
    state.text("PS5");
    state.focus = Target::Key(
        crate::state::KEYS
            .iter()
            .position(|k| *k == "Done")
            .unwrap(),
    );
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(state.preferences.input_names["com.webos.app.hdmi1"], "PS5");
    assert!(state.overlay.is_none());
}

#[test]
fn home_lifecycle_replies_map_for_both_registration_versions() {
    use crate::platform::Lifecycle;
    // Lifecycle v2 (registerApp, in place of LG's Home executable) names it in "event".
    assert_eq!(
        Lifecycle::from_reply(&json!({"event": "relaunch", "reason": "hotKey"})),
        Some(Lifecycle::Relaunch)
    );
    assert_eq!(
        Lifecycle::from_reply(&json!({"event": "close"})),
        Some(Lifecycle::Close)
    );
    // Lifecycle v1 (registerNativeApp for a staged Home manifest on webOS 10/11) in "message".
    assert_eq!(
        Lifecycle::from_reply(&json!({
            "parameters": {"launchType": "homeKey", "activateType": "home"},
            "returnValue": true,
            "message": "relaunch"
        })),
        Some(Lifecycle::Relaunch)
    );
    assert_eq!(
        Lifecycle::from_reply(&json!({"returnValue": true, "message": "close"})),
        Some(Lifecycle::Close)
    );
    // Acknowledgements and keep-alive need no action.
    for reply in [
        json!({"returnValue": true, "message": "registered"}),
        json!({"event": "registered"}),
        json!({"event": "pause", "reason": "keepAlive"}),
        json!({"returnValue": false, "errorText": "refused"}),
    ] {
        assert_eq!(Lifecycle::from_reply(&reply), None, "{reply}");
    }
}

#[test]
fn foreground_check_needs_focus_after_a_relaunch_from_the_background() {
    use crate::platform::ForegroundCheck;
    use std::time::{Duration, Instant};
    let start = Instant::now();
    let late = start + ForegroundCheck::TIMEOUT;

    // Relaunch while hidden, then only Shown (SDL emits it for SDL_ShowWindow itself): fails.
    let mut check = ForegroundCheck::default();
    check.relaunched(start, false);
    assert!(!check.failed(start + Duration::from_secs(1)));
    assert!(check.failed(late));
    // Reported once.
    assert!(!check.failed(late + Duration::from_secs(1)));

    // FocusGained or a remote key in time: no failure.
    let mut check = ForegroundCheck::default();
    check.relaunched(start, false);
    check.confirmed();
    assert!(!check.failed(late));

    // Already focused and visible (Home pressed on Home): nothing to check.
    let mut check = ForegroundCheck::default();
    check.relaunched(start, true);
    assert!(!check.failed(late));

    // A later relaunch from the background is checked again.
    check.relaunched(late, false);
    assert!(check.failed(late + ForegroundCheck::TIMEOUT));
}

#[test]
fn prewarm_waits_while_recents_replaces_the_page() {
    let mut state = ready();
    let mut jobs = vec![Page::Settings, Page::Apps];
    state.open_recents();
    state.recents.visibility = 1.;
    // As after the frame that showed it.
    state.dirty = false;
    // The Apps scene would be Recents only, so preparing it now would be wasted.
    let apps_controls = |state: &mut State| {
        let current = std::mem::replace(&mut state.page, Page::Apps);
        let found = controls(state)
            .iter()
            .any(|control| matches!(control.target, Target::App(_)));
        state.page = current;
        found
    };
    assert!(!apps_controls(&mut state));
    assert_eq!(crate::next_prewarm(&mut jobs, &state, true), None);
    assert_eq!(jobs, [Page::Settings, Page::Apps]);
    // Once Recents is gone the retained jobs run, in order.
    state.overlay = None;
    state.recents.visibility = 0.;
    state.dirty = false;
    assert!(apps_controls(&mut state));
    assert_eq!(crate::next_prewarm(&mut jobs, &state, false), None);
    assert_eq!(
        crate::next_prewarm(&mut jobs, &state, true),
        Some(Page::Apps)
    );
    assert_eq!(
        crate::next_prewarm(&mut jobs, &state, true),
        Some(Page::Settings)
    );
    assert_eq!(crate::next_prewarm(&mut jobs, &state, true), None);
}

#[test]
fn interface_scale_defaults_to_normal_and_recovers_from_unknown_values() {
    use crate::model::UiSize;
    assert_eq!(Preferences::default().ui_scale, UiSize::Normal);
    let preferences: Preferences =
        serde_json::from_value(json!({"ui_scale": "larger", "show_home_names": true})).unwrap();
    assert_eq!(preferences.ui_scale, UiSize::Larger);
    for broken in [json!("huge"), json!(3), json!({"clock": "large"})] {
        let preferences: Preferences =
            serde_json::from_value(json!({"ui_scale": broken, "show_home_names": true})).unwrap();
        assert_eq!(preferences.ui_scale, UiSize::Normal);
        assert!(preferences.show_home_names);
    }
    let restored: Preferences =
        serde_json::from_value(serde_json::to_value(&preferences).unwrap()).unwrap();
    assert_eq!(restored.ui_scale, UiSize::Larger);
    assert_eq!(UiSize::Normal.scale(), 1.);
    assert_eq!(UiSize::Large.scale(), 1.25);
    assert_eq!(UiSize::Larger.scale(), 1.5);
}

#[test]
fn interface_scale_is_chosen_like_input_visibility_and_stays_open() {
    use crate::model::UiSize;
    use crate::state::Subpage;
    let mut state = ready();
    state.open(2);
    state.direction(Direction::Down, &controls(&state));
    state.direction(Direction::Right, &controls(&state));
    for _ in 0..5 {
        state.direction(Direction::Down, &controls(&state));
    }
    assert_eq!(state.focus, Target::Setting(7));
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.subpage, Some(Subpage::Appearance));
    assert_eq!(state.focus, Target::Choice(0));
    state.direction(Direction::Down, &controls(&state));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(state.preferences.ui_scale, UiSize::Larger);
    assert_eq!(state.subpage, Some(Subpage::Appearance));
    assert_eq!(state.focus, Target::Choice(2));
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.subpage, None);
    assert_eq!(state.focus, Target::Setting(7));
    // Reopening starts on the chosen scale; Back returns to the row.
    state.activate();
    assert_eq!(state.focus, Target::Choice(2));
    state.back();
    assert_eq!(state.focus, Target::Setting(7));
}

#[test]
fn interface_scale_enlarges_the_clock_and_hints_and_the_preview_is_not_focusable() {
    use crate::model::UiSize;
    let text_size = |scene: &view::Scene, needle: &str| {
        scene.draw.iter().find_map(|draw| match draw {
            crate::renderer::Draw::Text(text, _, style, ..) if text.contains(needle) => {
                Some(style.size)
            }
            _ => None,
        })
    };
    let mut state = ready();
    state.open(2);
    state.subpage = Some(crate::state::Subpage::Appearance);
    state.focus = Target::Choice(0);
    let normal = scene(&state);
    let clock = text_size(&normal, "AM").expect("clock");
    let hint = text_size(&normal, "Select").expect("hint");
    state.preferences.ui_scale = UiSize::Larger;
    let larger = scene(&state);
    assert!((text_size(&larger, "AM").unwrap() - clock * 1.5).abs() < 0.01);
    assert!((text_size(&larger, "Select").unwrap() - hint * 1.5).abs() < 0.01);
    // The preview draws categories and search, but only the options take focus.
    assert!(text_size(&larger, "Streaming").is_some());
    assert!(!larger.controls.iter().any(|c| matches!(
        c.target,
        Target::Filter(_) | Target::Search | Target::App(_)
    )));
}

#[test]
fn home_takeover_is_chosen_in_system_and_only_changes_on_a_new_choice() {
    use crate::state::Subpage;
    let mut state = ready();
    state.open(2);
    state.section = crate::state::SYSTEM_SECTION;
    state.focus = Target::Setting(8);
    assert!(
        controls(&state)
            .iter()
            .any(|c| c.target == Target::Setting(8))
    );
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.subpage, Some(Subpage::HomeTakeover));
    assert_eq!(
        state.focus,
        Target::Choice(1),
        "starts on the current state: off"
    );
    // Off again changes nothing.
    assert_eq!(state.activate(), vec![]);
    assert_eq!(state.focus, Target::Setting(8));
    state.activate();
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::Choice(0));
    assert_eq!(state.activate(), vec![Effect::HomeTakeover(true)]);
    assert_eq!(state.subpage, None);
    assert_eq!(state.focus, Target::Setting(8));
    // Running as Home, the page offers to turn it off.
    state.home_takeover = true;
    state.activate();
    assert_eq!(state.focus, Target::Choice(0));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.activate(), vec![Effect::HomeTakeover(false)]);
}

#[test]
fn failed_first_inventory_keeps_loading_until_retries_are_used_up() {
    let mut state = State::new(Preferences::default());
    state.loading = true;
    for attempt in 1..=crate::state::INVENTORY_RETRIES {
        assert_eq!(
            state.inventory_failed("The TV service timed out".into()),
            Some(crate::state::INVENTORY_RETRY_DELAY)
        );
        assert_eq!(state.inventory_failures, attempt);
        assert!(state.loading && state.error.is_none());
    }
    assert_eq!(
        state.inventory_failed("The TV service timed out".into()),
        None
    );
    assert!(!state.loading);
    assert_eq!(state.error.as_deref(), Some("The TV service timed out"));
    assert_eq!(state.first_content(), Target::Retry);
    // Retry by hand starts a fresh round of automatic retries.
    assert_eq!(state.inventory_failures, 0);
}

#[test]
fn inventory_success_resets_failures_and_later_failures_show_at_once() {
    let mut state = State::new(Preferences::default());
    state.loading = true;
    assert!(state.inventory_failed("timeout".into()).is_some());
    state.set_inventory(demo_inventory(), None);
    assert_eq!(state.inventory_failures, 0);
    state.loading = true;
    // Apps are on screen already: a failed refresh is reported, not retried behind them.
    assert_eq!(state.inventory_failed("timeout".into()), None);
    assert!(!state.loading && state.error.is_some());
    assert!(!state.inventory.apps.is_empty());
}

fn control_rect(state: &State, target: &Target) -> crate::state::Rect {
    controls(state)
        .into_iter()
        .find(|c| &c.target == target)
        .unwrap_or_else(|| panic!("no control {target:?}"))
        .rect
}

#[test]
fn home_draws_each_block_in_its_grid_cells() {
    use crate::home_grid::{BlockKind, rect};
    let state = ready();
    let blocks = state.home_blocks();
    let cell = |kind| rect(blocks.iter().find(|b| b.kind == kind).unwrap());
    let greeting = control_rect(&state, &Target::Block(BlockKind::Greeting));
    assert_eq!(
        (greeting.x, greeting.y),
        (cell(BlockKind::Greeting).x, cell(BlockKind::Greeting).y)
    );
    let first = state.apps()[0].id.clone();
    let tile = control_rect(&state, &Target::App(first));
    assert_eq!(tile.y, cell(BlockKind::Favorites).y + 66.);
    let input = state.inventory.inputs[0].id.clone();
    let card = control_rect(&state, &Target::Input(input));
    assert_eq!(
        (card.x, card.y),
        (cell(BlockKind::Inputs).x, cell(BlockKind::Inputs).y)
    );
    assert!(
        (card.w - 569.1).abs() < 0.01,
        "three cards across twelve columns"
    );
}

#[test]
fn a_narrow_favorites_block_keeps_its_strip_inside_the_block() {
    use crate::home_grid::{Block, BlockKind};
    let mut state = ready();
    state.preferences.home_layout = vec![
        Block {
            kind: BlockKind::Greeting,
            x: 0,
            y: 0,
            w: 8,
            h: 2,
        },
        Block {
            kind: BlockKind::Favorites,
            x: 0,
            y: 2,
            w: 8,
            h: 2,
        },
        Block {
            kind: BlockKind::Weather,
            x: 8,
            y: 2,
            w: 4,
            h: 1,
        },
    ];
    let strip = state.favorites_strip();
    assert!(
        (strip.right - (84. + 3. * 281.6 + 256.)).abs() < 0.01,
        "four whole tiles"
    );
    assert!(
        (strip.clip_right - (84. + 1160.)).abs() < 0.01,
        "clipped at the block's edge"
    );
    let viewport = state.favorites_viewport();
    assert!((viewport.x + viewport.w - strip.clip_right).abs() < 0.01);
    // A tile past the block's edge is not a remote target from outside the strip.
    let full = state.preferences.home_layout.clone();
    state.preferences.home_layout = full;
    assert_eq!(crate::home_grid::bottom(&state.home_blocks()), 4);
}

#[test]
fn add_widget_sits_below_the_last_block_while_kinds_are_missing() {
    use crate::home_grid::{TOP, bottom};
    let mut state = ready();
    let add = control_rect(&state, &Target::AddBlock);
    assert_eq!(add.y, TOP + f32::from(bottom(&state.home_blocks())) * 124.);
    state.preferences.home_layout = crate::home_grid::BlockKind::ALL
        .iter()
        .map(|kind| {
            let (x, y) = crate::home_grid::first_free(&state.home_blocks(), 12, 3);
            crate::home_grid::Block {
                kind: *kind,
                x,
                y,
                w: 12,
                h: 3,
            }
        })
        .collect();
    assert!(
        !controls(&state)
            .iter()
            .any(|c| c.target == Target::AddBlock)
    );
}

fn kind_rect(state: &State, kind: crate::home_grid::BlockKind) -> crate::home_grid::Block {
    *state
        .preferences
        .home_layout
        .iter()
        .find(|b| b.kind == kind)
        .unwrap_or_else(|| panic!("no {kind:?}"))
}

#[test]
fn moving_a_block_pushes_others_and_back_restores_the_layout() {
    use crate::home_grid::BlockKind;
    let mut state = ready();
    let start = state.preferences.home_layout.clone();
    state.focus = Target::Block(BlockKind::Greeting);
    state.hold();
    assert!(matches!(
        state.overlay,
        Some(Overlay::Block {
            kind: BlockKind::Greeting,
            ..
        })
    ));
    assert_eq!(state.focus, Target::BlockAction(0), "Move comes first");
    assert_eq!(state.activate(), vec![]);
    assert!(state.home_edit.as_ref().is_some_and(|edit| !edit.resize));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(kind_rect(&state, BlockKind::Greeting).y, 1);
    assert!(kind_rect(&state, BlockKind::Weather).y >= 3, "pushed below");
    assert_eq!(state.back(), vec![]);
    assert!(state.home_edit.is_none());
    assert_eq!(state.preferences.home_layout, start);
}

#[test]
fn resizing_stays_within_limits_and_ok_saves() {
    use crate::home_grid::BlockKind;
    let mut state = ready();
    state.focus = Target::Block(BlockKind::Greeting);
    state.hold();
    state.focus = Target::BlockAction(1);
    state.activate();
    assert!(state.home_edit.as_ref().is_some_and(|edit| edit.resize));
    for _ in 0..10 {
        state.direction(Direction::Right, &controls(&state));
        state.direction(Direction::Up, &controls(&state));
    }
    let greeting = kind_rect(&state, BlockKind::Greeting);
    assert_eq!((greeting.w, greeting.h), (12, 1));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.home_edit.is_none());
    assert_eq!(kind_rect(&state, BlockKind::Greeting).w, 12);
}

#[test]
fn widgets_can_be_removed_and_added_back() {
    use crate::home_grid::BlockKind;
    let mut state = ready();
    state.focus = Target::WeatherConfigure;
    state.hold();
    assert!(matches!(
        state.overlay,
        Some(Overlay::Block {
            kind: BlockKind::Weather,
            ..
        })
    ));
    state.focus = Target::BlockAction(2);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(
        !state
            .preferences
            .home_layout
            .iter()
            .any(|b| b.kind == BlockKind::Weather)
    );
    state.focus = Target::AddBlock;
    state.activate();
    assert!(matches!(state.overlay, Some(Overlay::AddBlock { .. })));
    let missing = state.missing_blocks();
    assert_eq!(
        missing,
        vec![
            BlockKind::Weather,
            BlockKind::Crypto,
            BlockKind::Premieres,
            BlockKind::Esports(0)
        ]
    );
    state.focus = Target::AddKind(1);
    assert_eq!(state.activate(), vec![Effect::Save]);
    let crypto = kind_rect(&state, BlockKind::Crypto);
    assert_eq!(
        (crypto.x, crypto.y, crypto.w, crypto.h),
        (8, 0, 4, 1),
        "first free place"
    );
    assert_eq!(state.focus, Target::Block(BlockKind::Crypto));
    assert!(state.overlay.is_none());
}

#[test]
fn holding_a_favorite_offers_reordering_its_apps_first() {
    use crate::home_grid::BlockKind;
    let mut state = ready();
    let first = state.apps()[0].id.clone();
    state.focus = Target::App(first.clone());
    state.hold();
    assert!(matches!(
        state.overlay,
        Some(Overlay::Block {
            kind: BlockKind::Favorites,
            ..
        })
    ));
    state.activate();
    assert_eq!(state.moving, Some(first));
    // Favourites and the greeting cannot be removed.
    assert!(
        !state
            .block_actions(BlockKind::Favorites, true)
            .contains(&crate::state::BlockAction::Remove)
    );
    assert!(
        !state
            .block_actions(BlockKind::Greeting, false)
            .contains(&crate::state::BlockAction::Remove)
    );
}

fn quote(coin: &'static str) -> crate::feeds::Quote {
    crate::feeds::Quote {
        coin,
        last: 100.,
        change: 1.5,
        low: 90.,
        high: 110.,
    }
}

fn scene_text(state: &State) -> Vec<String> {
    scene(state)
        .draw
        .into_iter()
        .filter_map(|draw| match draw {
            crate::renderer::Draw::Text(text, ..) => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn wider_crypto_blocks_show_more_coins() {
    use crate::home_grid::{Block, BlockKind};
    let mut state = ready();
    state.feeds.quotes = crate::feeds::COINS.map(quote).to_vec();
    for (w, shown, hidden) in [(4, "ETH", "SOL"), (6, "SOL", "BNB")] {
        state.preferences.home_layout = crate::home_grid::classic_layout();
        state.preferences.home_layout.push(Block {
            kind: BlockKind::Crypto,
            x: 0,
            y: 6,
            w,
            h: 1,
        });
        let texts = scene_text(&state);
        assert!(texts.iter().any(|t| t == shown), "{w} columns show {shown}");
        assert!(
            !texts.iter().any(|t| t == hidden),
            "{w} columns hide {hidden}"
        );
    }
}

#[test]
fn a_premiere_poster_opens_apps_to_watch_it_in() {
    use crate::home_grid::{Block, BlockKind};
    let mut state = ready();
    state.preferences.home_layout.push(Block {
        kind: BlockKind::Premieres,
        x: 0,
        y: 6,
        w: 12,
        h: 2,
    });
    state.feeds.premieres = vec![crate::feeds::Premiere {
        title: "Lupin".into(),
        season: 4,
        date: "2026-10-23".into(),
        poster: None,
    }];
    assert!(
        controls(&state)
            .iter()
            .any(|c| c.target == Target::Poster(0))
    );
    state.focus = Target::Poster(0);
    assert_eq!(state.activate(), vec![]);
    assert!(matches!(
        state.overlay,
        Some(Overlay::Poster { index: 0, .. })
    ));
    assert_eq!(state.focus, Target::PosterApp(0));
    let apps = state.poster_apps();
    assert!(!apps.is_empty());
    let first = apps[0].clone();
    let effects = state.activate();
    assert_eq!(effects[..2], [Effect::Save, Effect::Launch(first.clone())]);
    // The title follows the launch, so it replaces the generic "Opening…" notice.
    assert!(matches!(&effects[2], Effect::Notify(text) if text.contains("Lupin")));
    assert_eq!(state.preferences.poster_app, first);
    assert!(state.overlay.is_none());
    // The app chosen last comes first next time; holding a poster edits its block.
    assert_eq!(state.poster_apps()[0], first);
    state.focus = Target::Poster(0);
    state.hold();
    assert!(matches!(
        state.overlay,
        Some(Overlay::Block {
            kind: BlockKind::Premieres,
            ..
        })
    ));
}

#[test]
fn a_home_theme_is_chosen_in_general_and_replaces_the_layout() {
    use crate::state::Subpage;
    let mut state = ready();
    state.open(2);
    state.focus = Target::Setting(9);
    assert!(
        controls(&state)
            .iter()
            .any(|c| c.target == Target::Setting(9))
    );
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.subpage, Some(Subpage::Theme));
    assert_eq!(state.focus, Target::Choice(0));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(state.preferences.home_theme, "cinema");
    assert_eq!(state.preferences.background, "midnight");
    assert_eq!(
        state.preferences.home_layout,
        (crate::themes::theme("cinema").layout)()
    );
    assert_eq!(state.subpage, Some(Subpage::Theme), "stays open");
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::Setting(9));
}

fn esports_match(game: &str, id: u64, tier: &str) -> crate::feeds::CsMatch {
    let team = |id, short: &str| {
        Some(crate::feeds::Team {
            id,
            name: short.into(),
            short: short.into(),
            image: None,
            location: "EU".into(),
        })
    };
    crate::feeds::CsMatch {
        id,
        game: game.into(),
        running: id == 7,
        begin_at: format!("2026-10-25T18:{id:02}:00Z"),
        teams: [team(1, "NAVI"), team(2, "FaZe")],
        score: [1, 0],
        best_of: 3,
        games: vec![],
        event: "BLAST Premier".into(),
        tier: tier.into(),
        stage: "Playoffs".into(),
        prizepool: "1000000 United States Dollar".into(),
        online: Some(false),
        league_image: None,
        streams: vec![crate::feeds::Stream {
            language: "ru".into(),
            platform: "Twitch".into(),
            official: true,
            main: true,
        }],
        maps_source: String::new(),
    }
}

#[test]
fn a_match_names_its_maps_with_scores_picks_and_their_source() {
    use crate::feeds::{GameState, MatchGame};
    use crate::home_grid::{Block, BlockKind};
    let mut state = ready();
    state.preferences.home_layout.push(Block {
        kind: BlockKind::Esports(0),
        x: 0,
        y: 6,
        w: 12,
        h: 2,
    });
    let map = |position, state, name: &str, score, picked_by, decider| MatchGame {
        position,
        state,
        winner: (state == GameState::Finished).then_some(1),
        begin_at: String::new(),
        length: None,
        map: name.into(),
        score,
        picked_by,
        decider,
    };
    let mut found = esports_match("csgo", 7, "s");
    found.games = vec![
        map(
            1,
            GameState::Finished,
            "Dust II",
            Some([13., 11.]),
            Some(2),
            false,
        ),
        map(
            2,
            GameState::Running,
            "Nuke",
            Some([7., 5.]),
            Some(1),
            false,
        ),
        map(3, GameState::Waiting, "Ancient", None, None, true),
    ];
    found.maps_source = "Liquipedia".into();
    state.feeds.matches.insert("csgo".into(), vec![found]);
    assert!(
        scene_text(&state).iter().any(|t| t == "Map 2 · Nuke"),
        "Home tells which map is live"
    );
    state.focus = Target::Match(0, 0);
    state.activate();
    let texts = scene_text(&state);
    for name in ["Dust II", "Nuke", "Ancient"] {
        assert!(texts.iter().any(|t| t == name), "{name}");
    }
    assert!(!texts.iter().any(|t| t == "Map 1"), "a known map is named");
    assert!(
        texts.iter().any(|t| t == "13:11"),
        "rounds of a finished map"
    );
    assert!(texts.iter().any(|t| t == "7:5"), "rounds of the live map");
    assert!(texts.iter().any(|t| t == "FaZe pick"));
    assert!(texts.iter().any(|t| t == "Decider"));
    assert!(texts.iter().any(|t| t == "Maps: Liquipedia (CC-BY-SA)"));
    // PandaScore's own data: numbered maps and no credit.
    let found = &mut state.feeds.matches.get_mut("csgo").unwrap()[0];
    found.maps_source.clear();
    for game in &mut found.games {
        game.map.clear();
        game.score = None;
    }
    let texts = scene_text(&state);
    assert!(texts.iter().any(|t| t == "Map 1"));
    assert!(!texts.iter().any(|t| t.contains("Liquipedia")));
}

#[test]
fn a_personal_key_names_live_maps_from_the_shared_feed_and_credits_it() {
    use crate::feeds::{GameState, MatchGame, SharedFeed};
    use crate::home_grid::{Block, BlockKind};
    let mut state = ready();
    state.preferences.home_layout.push(Block {
        kind: BlockKind::Esports(0),
        x: 0,
        y: 6,
        w: 12,
        h: 2,
    });
    state.preferences.esports_key = "key".into();
    let map = |position, state, name: &str| MatchGame {
        position,
        state,
        winner: (state == GameState::Finished).then_some(1),
        begin_at: String::new(),
        length: None,
        map: name.into(),
        score: None,
        picked_by: None,
        decider: false,
    };
    // PandaScore's lists: live, but the maps are only numbered.
    let mut live = esports_match("csgo", 7, "s");
    live.games = vec![
        map(1, GameState::Finished, ""),
        map(2, GameState::Running, ""),
        map(3, GameState::Waiting, ""),
    ];
    // The shared feed names them, though its own state is minutes behind.
    let mut shared = live.clone();
    shared.score = [0, 0];
    shared.games = vec![
        map(1, GameState::Running, "Dust II"),
        map(2, GameState::Waiting, "Nuke"),
        map(3, GameState::Waiting, "Ancient"),
    ];
    shared.games[0].score = Some([9., 4.]);
    shared.maps_source = "Liquipedia".into();
    state.esports_received("csgo".into(), vec![live.clone()]);
    state.shared_received(
        "csgo".into(),
        SharedFeed {
            matches: vec![shared],
            rosters: vec![],
            updated: Some(0),
        },
    );
    let dialog = |state: &mut State| {
        state.focus = Target::Match(0, 0);
        state.activate();
        let texts = scene_text(state);
        state.back();
        texts
    };
    assert!(
        scene_text(&state).iter().any(|t| t == "Map 2 · Nuke"),
        "PandaScore tells which map is live, the feed its name"
    );
    assert!(
        !scene_text(&state)
            .iter()
            .any(|t| t.starts_with("Data as of")),
        "the matches stay live"
    );
    let texts = dialog(&mut state);
    for name in ["Dust II", "Nuke", "Ancient", "9:4"] {
        assert!(texts.iter().any(|t| t == name), "{name}");
    }
    assert!(texts.iter().any(|t| t == "Maps: Liquipedia (CC-BY-SA)"));
    // The next PandaScore poll keeps the names until the feed is fetched again.
    state.esports_received("csgo".into(), vec![live]);
    let texts = dialog(&mut state);
    assert!(texts.iter().any(|t| t == "Nuke"));
    assert!(texts.iter().any(|t| t == "Maps: Liquipedia (CC-BY-SA)"));
    assert_eq!(
        state.feeds.matches["csgo"][0].score,
        [1, 0],
        "the series score stays PandaScore's"
    );
}

#[test]
fn a_match_opens_its_details_and_asks_for_missing_lineups() {
    use crate::home_grid::{Block, BlockKind};
    let mut state = ready();
    state.preferences.home_layout.push(Block {
        kind: BlockKind::Esports(0),
        x: 0,
        y: 6,
        w: 6,
        h: 2,
    });
    state.preferences.esports_key = "key".into();
    state
        .feeds
        .matches
        .insert("csgo".into(), vec![esports_match("csgo", 7, "s")]);
    state.feeds.rosters.insert(
        2,
        vec![crate::feeds::Player {
            name: "ropz".into(),
            real_name: "Robin Kool".into(),
            nationality: "EE".into(),
            role: String::new(),
            age: Some(26),
            image: None,
            substitute: false,
        }],
    );
    assert!(
        controls(&state)
            .iter()
            .any(|c| c.target == Target::Match(0, 0))
    );
    state.focus = Target::Match(0, 0);
    assert_eq!(
        state.activate(),
        vec![Effect::Rosters("csgo".into(), vec![1])]
    );
    assert!(matches!(
        state.overlay,
        Some(Overlay::Match {
            instance: 0,
            index: 0,
            ..
        })
    ));
    let texts = scene_text(&state);
    assert!(texts.iter().any(|t| t == "ropz"));
    assert!(texts.iter().any(|t| t.contains("Robin Kool")), "real names");
    assert!(texts.iter().any(|t| t.contains("Twitch")), "streams");
    assert!(
        texts
            .iter()
            .any(|t| t.contains("1 000 000") || t.contains("1,000,000")),
        "prize pool"
    );
    state.back();
    assert!(state.overlay.is_none());
    assert_eq!(state.focus, Target::Match(0, 0));
}

#[test]
fn each_esports_widget_shows_its_own_disciplines_and_filter() {
    use crate::home_grid::{Block, BlockKind};
    use crate::model::EsportsWidget;
    let mut state = ready();
    state.feeds.matches.insert(
        "csgo".into(),
        vec![esports_match("csgo", 7, "s"), esports_match("csgo", 8, "c")],
    );
    state
        .feeds
        .matches
        .insert("dota2".into(), vec![esports_match("dota2", 9, "a")]);
    state
        .preferences
        .esports
        .insert(0, EsportsWidget::default());
    state.preferences.esports.insert(
        1,
        EsportsWidget {
            games: vec!["csgo".into(), "dota2".into()],
            major_only: false,
        },
    );
    let ids = |state: &State, n| {
        state
            .esports_matches(n)
            .iter()
            .map(|m| m.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&state, 0), [7], "Counter-Strike, major events only");
    assert_eq!(ids(&state, 1), [7, 8, 9], "live first, then by start");
    // A second widget is added as a new instance with the default settings.
    state.preferences.home_layout.push(Block {
        kind: BlockKind::Esports(0),
        x: 0,
        y: 6,
        w: 6,
        h: 2,
    });
    assert_eq!(state.missing_blocks().last(), Some(&BlockKind::Esports(1)));
}

#[test]
fn the_esports_widget_settings_take_a_key_disciplines_and_the_major_filter() {
    use crate::home_grid::{Block, BlockKind};
    use crate::state::{BlockAction, Subpage};
    let mut state = ready();
    state.preferences.home_layout.push(Block {
        kind: BlockKind::Esports(0),
        x: 0,
        y: 6,
        w: 6,
        h: 2,
    });
    let actions = state.block_actions(BlockKind::Esports(0), false);
    assert!(actions.contains(&BlockAction::Settings));
    state.focus = Target::Block(BlockKind::Esports(0));
    state.hold();
    state.focus = Target::BlockAction(
        actions
            .iter()
            .position(|a| *a == BlockAction::Settings)
            .unwrap(),
    );
    state.activate();
    assert_eq!(state.page, crate::state::Page::Settings);
    assert_eq!(state.subpage, Some(Subpage::Esports(0)));
    assert_eq!(state.focus, Target::EsportsKey);
    state.activate();
    assert!(state.text_input_active());
    state.text("ps_Key-123");
    assert_eq!(state.finish_text_input(), vec![Effect::Save]);
    assert_eq!(state.preferences.esports_key, "ps_Key-123");
    assert!(
        state.esports_widget(0).major_only,
        "major events by default"
    );
    state.focus = Target::EsportsMajor;
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(!state.esports_widget(0).major_only);
    state.focus = Target::EsportsGames;
    state.activate();
    assert_eq!(state.subpage, Some(Subpage::EsportsGames(0)));
    // Each discipline toggles; the list stays open, and the last one cannot be turned off.
    state.focus = Target::Choice(1);
    assert_eq!(state.activate(), vec![Effect::Save]);
    state.focus = Target::Choice(0);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(state.esports_widget(0).games, ["dota2"]);
    state.focus = Target::Choice(1);
    assert_eq!(state.activate(), vec![]);
    assert_eq!(state.esports_widget(0).games, ["dota2"]);
    assert_eq!(state.subpage, Some(Subpage::EsportsGames(0)));
    state.back();
    assert_eq!(state.subpage, Some(Subpage::Esports(0)));
    assert_eq!(state.focus, Target::EsportsGames);
}

#[test]
fn the_key_can_come_from_a_phone_through_a_qr_code() {
    use crate::state::Subpage;
    let mut state = ready();
    state.open(2);
    state.subpage = Some(Subpage::Esports(0));
    state.focus = Target::EsportsPhone;
    assert!(
        controls(&state)
            .iter()
            .any(|c| c.target == Target::EsportsPhone)
    );
    assert_eq!(state.activate(), vec![Effect::PhoneKeyStart]);
    assert!(matches!(state.overlay, Some(Overlay::PhoneKey { .. })));
    assert_eq!(state.focus, Target::CloseDialog);
    let qr = crate::phone_key::qr_image("http://192.168.0.218:8765/ab").unwrap();
    state.phone_key = Some((
        "http://192.168.0.218:8765/ab".into(),
        std::sync::Arc::new(crate::app_artwork::Artwork {
            key: "qr:ab".into(),
            pixels: qr,
            backdrop: [[255; 3]; 2],
        }),
    ));
    let texts = scene_text(&state);
    assert!(
        texts
            .iter()
            .any(|t| t.contains("http://192.168.0.218:8765/ab"))
    );
    assert!(texts.iter().any(|t| t.starts_with("1.")), "numbered steps");
    assert_eq!(
        state.phone_key_received("ps_123".into()),
        vec![Effect::Save]
    );
    assert_eq!(state.preferences.esports_key, "ps_123");
    assert!(state.overlay.is_none());
    assert!(state.phone_key.is_none());
    assert_eq!(state.focus, Target::EsportsPhone);
}

#[test]
fn up_reaches_a_wide_block_directly_above_rather_than_a_closer_centre() {
    use crate::home_grid::{Block, BlockKind};
    let mut state = ready();
    state.preferences.home_layout = vec![
        Block {
            kind: BlockKind::Greeting,
            x: 0,
            y: 0,
            w: 8,
            h: 2,
        },
        Block {
            kind: BlockKind::Esports(0),
            x: 0,
            y: 2,
            w: 12,
            h: 2,
        },
        Block {
            kind: BlockKind::Favorites,
            x: 0,
            y: 4,
            w: 12,
            h: 2,
        },
    ];
    state.direction(Direction::Down, &controls(&state));
    assert!(matches!(state.focus, Target::App(_)));
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::Block(BlockKind::Esports(0)));
}

#[test]
fn esports_widgets_load_team_logos_and_an_open_match_its_photos() {
    use crate::home_grid::{Block, BlockKind};
    let mut state = ready();
    state.preferences.home_layout.push(Block {
        kind: BlockKind::Esports(0),
        x: 0,
        y: 6,
        w: 6,
        h: 2,
    });
    let logo = "https://cdn-api.pandascore.co/navi.png".to_owned();
    let league = "https://cdn-api.pandascore.co/blast.png".to_owned();
    let photo = "https://cdn-api.pandascore.co/b1t.png".to_owned();
    let mut found = esports_match("csgo", 7, "s");
    found.teams[0].as_mut().unwrap().image = Some(logo.clone());
    found.league_image = Some(league.clone());
    state.feeds.matches.insert("csgo".into(), vec![found]);
    state.feeds.rosters.insert(
        1,
        vec![crate::feeds::Player {
            name: "b1t".into(),
            real_name: String::new(),
            nationality: String::new(),
            role: String::new(),
            age: None,
            image: Some(photo.clone()),
            substitute: false,
        }],
    );
    assert_eq!(state.wanted_images(), vec![logo.clone()]);
    state.focus = Target::Match(0, 0);
    state.activate();
    assert!(
        state.feeds.images_due,
        "opening a match asks for its images"
    );
    assert_eq!(state.wanted_images(), vec![logo, league, photo]);
}

#[test]
fn the_shared_feed_tells_how_old_its_data_is_and_which_lineups_it_lacks() {
    use crate::home_grid::{Block, BlockKind};
    let mut state = ready();
    state.preferences.home_layout.push(Block {
        kind: BlockKind::Esports(0),
        x: 0,
        y: 6,
        w: 6,
        h: 2,
    });
    state
        .feeds
        .matches
        .insert("csgo".into(), vec![esports_match("csgo", 7, "s")]);
    state.feeds.updated.insert("csgo".into(), 0);
    let as_of = |state: &State| {
        scene_text(state)
            .iter()
            .any(|t| t.starts_with("Data as of"))
    };
    assert!(as_of(&state), "shared data says when it was built");
    // Without a key the feed is the only source: a lineup it lacks is not coming.
    state.focus = Target::Match(0, 0);
    state.activate();
    let texts = scene_text(&state);
    assert!(texts.iter().any(|t| t == "Lineup not announced"));
    assert!(!texts.iter().any(|t| t == "Loading lineup…"));
    state.back();
    state.preferences.esports_key = "key".into();
    state.feeds.updated.clear();
    assert!(!as_of(&state), "a personal key polls live data");
    assert_eq!(view::prize("2,500 United States Dollar"), "$2 500");
    assert_eq!(view::prize("50000 Euro"), "€50 000");
    assert_eq!(view::prize("TBA"), "TBA");
}

fn lineup_player(name: &str, role: &str, substitute: bool) -> crate::feeds::Player {
    crate::feeds::Player {
        name: name.into(),
        real_name: String::new(),
        nationality: "BR".into(),
        role: role.into(),
        age: None,
        image: None,
        substitute,
    }
}

#[test]
fn a_match_dialog_lists_the_starters_and_puts_the_bench_on_a_line_of_its_own() {
    use crate::home_grid::{Block, BlockKind};
    let mut state = ready();
    state.preferences.home_layout.push(Block {
        kind: BlockKind::Esports(0),
        x: 0,
        y: 6,
        w: 6,
        h: 2,
    });
    state
        .feeds
        .matches
        .insert("csgo".into(), vec![esports_match("csgo", 7, "s")]);
    // PandaScore's whole roster, as the shared feed marks it from Liquipedia.
    state.feeds.rosters.insert(
        1,
        vec![
            lineup_player("tge", "coach", true),
            lineup_player("biguzera", "", false),
            lineup_player("vsm", "", true),
            lineup_player("saffee", "", false),
            lineup_player("Snowzin", "", false),
            lineup_player("piriajr", "", true),
            lineup_player("Tatu", "", false),
        ],
    );
    state.focus = Target::Match(0, 0);
    state.activate();
    let texts = scene_text(&state);
    let at = |name: &str| texts.iter().position(|t| t == name);
    assert!(
        at("biguzera") < at("saffee") && at("saffee") < at("Snowzin") && at("Snowzin") < at("Tatu"),
        "starters in the feed's order"
    );
    assert!(texts.iter().any(|t| t == "BENCH"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "vsm · piriajr"));
    assert!(texts.iter().any(|t| t == "COACH"));
    assert!(at("Tatu") < at("vsm · piriajr") && at("Tatu") < at("tge"));
    assert!(
        !texts.iter().any(|t| t == "BR · COACH"),
        "the bench has no rows of its own"
    );
    assert!(texts.iter().any(|t| t == "Lineups: Liquipedia (CC-BY-SA)"));
    state
        .feeds
        .matches
        .get_mut("csgo")
        .unwrap()
        .first_mut()
        .unwrap()
        .maps_source = "Liquipedia".into();
    let texts = scene_text(&state);
    assert!(
        texts
            .iter()
            .any(|t| t == "Maps and lineups: Liquipedia (CC-BY-SA)")
    );
    // A lineup Liquipedia did not mark: everyone on a row of their own, no credit.
    for player in state.feeds.rosters.get_mut(&1).unwrap() {
        player.substitute = false;
    }
    state.feeds.matches.get_mut("csgo").unwrap()[0]
        .maps_source
        .clear();
    let texts = scene_text(&state);
    assert!(!texts.iter().any(|t| t == "BENCH"));
    assert!(texts.iter().any(|t| t == "vsm") && texts.iter().any(|t| t == "BR · COACH"));
    assert!(!texts.iter().any(|t| t.contains("Liquipedia")));
}

#[test]
fn a_personal_keys_lineups_take_their_substitutes_from_the_shared_feed() {
    let mut state = ready();
    state.preferences.esports_key = "key".into();
    let pandascore = || {
        vec![(
            1,
            vec![
                lineup_player("biguzera", "", false),
                lineup_player("vsm", "", false),
            ],
        )]
    };
    let shared = || crate::feeds::SharedFeed {
        matches: vec![],
        rosters: vec![(
            1,
            vec![
                lineup_player("biguzera", "", false),
                lineup_player("vsm", "", true),
            ],
        )],
        updated: Some(0),
    };
    let marked = |state: &State| {
        state.feeds.rosters[&1]
            .iter()
            .map(|p| p.substitute)
            .collect::<Vec<_>>()
    };
    // The lineup arrives before the shared feed, and after it.
    state.rosters_received(pandascore());
    state.shared_received("csgo".into(), shared());
    assert_eq!(marked(&state), [false, true]);
    state.rosters_received(pandascore());
    assert_eq!(marked(&state), [false, true]);
}
