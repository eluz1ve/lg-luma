use super::*;
use crate::{
    renderer::{Draw, TextStyle},
    state::{Rect, Subpage, WIDGETS_SECTION},
};

fn settings(section: usize) -> State {
    let mut state = ready();
    state.page = Page::Settings;
    state.section = section;
    state.focus = Target::Sidebar(section);
    state
}

/// A rect as a comparable tuple.
fn xywh(rect: Rect) -> (f32, f32, f32, f32) {
    (rect.x, rect.y, rect.w, rect.h)
}

fn sidebar_item(state: &State, index: usize) -> (f32, f32, f32, f32) {
    xywh(
        controls(state)
            .into_iter()
            .find(|c| c.target == Target::Sidebar(index))
            .expect("the sidebar lists every section")
            .rect,
    )
}

fn texts(state: &State) -> Vec<(String, Rect, TextStyle)> {
    scene(state)
        .draw
        .into_iter()
        .filter_map(|draw| match draw {
            Draw::Text(text, rect, style, ..) => Some((text, rect, style)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_sidebar_is_an_icon_column_while_the_list_has_focus() {
    let mut state = settings(0);
    state.focus = Target::Setting(0);
    settle(&mut state);
    for index in 0..4 {
        assert_eq!(
            sidebar_item(&state, index),
            (88., 158. + index as f32 * 68., 60., 60.)
        );
    }
    state.focus = Target::Sidebar(1);
    settle(&mut state);
    assert_eq!(sidebar_item(&state, 1), (88., 226., 344., 60.));
}

#[test]
fn the_sidebar_eases_open_unless_motion_is_reduced() {
    let mut state = settings(0);
    state.focus = Target::Setting(0);
    settle(&mut state);
    state.focus = Target::Sidebar(0);
    state.animate(1. / 60.);
    let width = state.sidebar_width();
    assert!(width > 76. && width < 360., "{width}");
    state.preferences.reduced_motion = true;
    state.animate(1. / 60.);
    assert_eq!(state.sidebar_width(), 360.);
}

#[test]
fn the_page_title_names_the_section_and_the_subpage_path() {
    let mut state = settings(WIDGETS_SECTION);
    state.subpage = Some(Subpage::Weather);
    state.focus = Target::Choice(0);
    let titles: Vec<_> = texts(&state)
        .into_iter()
        .filter(|(_, rect, style)| rect.y == 150. && style.size <= 44. && style.size > 30.)
        .collect();
    assert_eq!(
        titles.iter().map(|t| t.0.as_str()).collect::<Vec<_>>(),
        ["Widgets", "Weather"]
    );
    assert_eq!(titles[0].1.x, 196.);
    assert!(titles.iter().all(|(_, _, style)| style.shadow && style.weight == 600));
    // The path is the page's title now, not a label inside the sidebar.
    assert_eq!(
        texts(&state).iter().filter(|t| t.0 == "Weather").count(),
        1
    );
}

/// The main sections' rows, in the order they are drawn.
fn setting_rects(state: &State) -> Vec<(f32, f32, f32, f32)> {
    controls(state)
        .into_iter()
        .filter(|c| matches!(c.target, Target::Setting(_) | Target::Widget(_)))
        .map(|c| xywh(c.rect))
        .collect()
}

/// The width tests measure text at: the renderer's fallback advances, without font metrics.
fn measured(text: &str, style: TextStyle) -> f32 {
    text.chars()
        .map(|c| (if c == ' ' { 0.28 } else { 0.53 }) + style.tracking)
        .sum::<f32>()
        * style.size
}

#[test]
fn rows_fill_the_820_list_under_the_title() {
    let mut state = settings(0);
    settle(&mut state);
    let rows = setting_rects(&state);
    assert_eq!(rows[0], (196., 224., 820., 68.));
    assert_eq!(rows[1].1 - rows[0].1, 78.);
    assert!(rows.iter().all(|row| row.0 == 196. && row.2 == 820. && row.3 == 68.));
}

#[test]
fn the_focused_row_grows_to_show_its_description() {
    let mut state = settings(0);
    state.focus = Target::Setting(1); // Clock, the second row
    settle(&mut state);
    let rows = setting_rects(&state);
    assert_eq!(rows[1].3, 104.);
    assert_eq!(rows[2].1, 224. + 2. * 78. + 36.);
    let english = crate::i18n::Language::En;
    let drawn = texts(&state);
    let (_, rect, style) = drawn
        .iter()
        .find(|t| t.0 == english.t("clockAbout"))
        .expect("the focused row describes itself");
    assert_eq!(style.size, 22.);
    assert!(rect.y > rows[1].1 + 40. && rect.y + rect.h <= rows[1].1 + 104., "{rect:?}");
    assert!(!drawn.iter().any(|t| t.0 == english.t("languageAbout")));
}

#[test]
fn values_sit_at_the_right_edge_of_the_list_and_titles_stop_before_them() {
    let mut state = settings(0);
    state.preferences.language = crate::i18n::Language::De;
    settle(&mut state);
    let german = crate::i18n::Language::De;
    let drawn = texts(&state);
    for (title, value) in [
        (german.t("language"), german.name()),
        (german.t("uiScale"), german.t("sizeNormal")),
    ] {
        let (_, value_rect, value_style) = drawn.iter().find(|t| t.0 == value).unwrap();
        assert_eq!(value_style.size, 24.);
        let right = value_rect.x + value_rect.w;
        assert!(right <= 1016. - 24. && right >= 1016. - 80., "{value}: {right}");
        let (_, title_rect, title_style) = drawn.iter().find(|t| t.0 == title).unwrap();
        assert!(
            title_rect.x + measured(title, *title_style) <= right - measured(value, *value_style),
            "{title} runs into {value}"
        );
    }
}

#[test]
fn rows_scrolled_under_the_title_are_masked_and_cannot_be_clicked() {
    let mut state = settings(0);
    state.subpage = Some(Subpage::Language);
    state.focus = Target::Choice(crate::i18n::Language::ALL.len() - 1);
    settle(&mut state);
    let controls = controls(&state);
    state.reveal_focus(&controls);
    let controls = super::controls(&state);
    assert!(state.scroll > 0.);
    let first = controls
        .iter()
        .find(|c| c.target == Target::Choice(0))
        .unwrap()
        .rect;
    assert!(first.y + first.h < 214., "{first:?}");
    assert_ne!(
        state.hit_test(&controls, first.x + 10., first.y + first.h - 2.),
        Some(Target::Choice(0))
    );
    assert!(
        scene(&state)
            .draw
            .iter()
            .any(|d| matches!(d, Draw::Clip(Some(r)) if r.y == 214.))
    );
}

fn test_assets() -> view::Assets {
    view::Assets {
        root: PathBuf::new(),
        version: "test".into(),
        metrics: BTreeMap::new(),
        artwork: crate::app_artwork::Service::new(false),
    }
}

/// The scene as the app draws it: with Home, small, in Settings' preview.
fn previewed(state: &mut State) -> view::Scene {
    let mut scene = scene(state);
    view::home_preview(
        &mut scene,
        state,
        &test_assets(),
        time::OffsetDateTime::UNIX_EPOCH,
    );
    scene
}

fn text_in(scene: &view::Scene, needle: &str) -> Option<(Rect, TextStyle)> {
    scene.draw.iter().find_map(|draw| match draw {
        Draw::Text(text, rect, style, ..) if text == needle => Some((*rect, *style)),
        _ => None,
    })
}

/// The greeting at the scenes' time, midnight UTC.
fn greeting_text(state: &State) -> String {
    crate::i18n::Language::En.greeting(0, &state.preferences.greeting_name)
}

const K: f32 = 776. / 1920.;

#[test]
fn general_and_widgets_show_home_small_beside_the_list() {
    for section in [0, WIDGETS_SECTION] {
        let mut state = settings(section);
        let scene = previewed(&mut state);
        let (rect, style) =
            text_in(&scene, &greeting_text(&state)).expect("Home's greeting in the preview");
        assert!((style.size - 76. * K).abs() < 0.01, "{}", style.size);
        assert!(rect.x >= 1064. && rect.x + rect.w <= 1840., "{rect:?}");
        assert!(rect.y >= 190. && rect.y <= 190. + 437., "{rect:?}");
        let targets = |scene: &view::Scene| -> Vec<Target> {
            scene.controls.iter().map(|c| c.target.clone()).collect()
        };
        assert_eq!(targets(&scene), targets(&super::scene(&state)), "the preview takes no focus");
        // Settings' own state is back once the preview is drawn.
        assert_eq!(state.page, Page::Settings);
        assert_eq!(state.focus, Target::Sidebar(section));
    }
    for section in [1, crate::state::SYSTEM_SECTION] {
        let mut state = settings(section);
        assert!(text_in(&previewed(&mut state), &greeting_text(&state)).is_none());
    }
}

#[test]
fn the_preview_rings_the_part_the_focused_setting_changes() {
    let mut state = settings(0);
    state.focus = Target::Setting(1); // Clock
    let mut home = ready();
    home.focus = Target::Nav(0);
    home.pointer = true;
    let clock = scene(&home)
        .regions
        .iter()
        .find(|(region, _)| *region == view::Region::Clock)
        .map(|(_, rect)| *rect)
        .expect("Home marks its clock");
    let clock = Rect::new(1064. + clock.x * K, 190. + clock.y * K, clock.w * K, clock.h * K);
    let scene = previewed(&mut state);
    assert!(scene.draw.iter().any(|draw| matches!(draw,
        Draw::Shape(r, _, width, _) if *width >= 2.
            && r.x < clock.x && r.y < clock.y
            && r.x + r.w > clock.x + clock.w && r.y + r.h > clock.y + clock.h
            && r.w < clock.w + 40.
    )));
}

#[test]
fn system_summarises_luma_and_its_updates_on_the_right() {
    let mut state = settings(crate::state::SYSTEM_SECTION);
    state.panel_started = None;
    let drawn = texts(&state);
    let version = crate::i18n::Language::En.format("currentVersion", &[("current", "test")]);
    assert!(
        drawn.iter().any(|(text, rect, _)| *text == version && rect.x >= 1064.),
        "the version sits in the summary on the right"
    );
    let check = controls(&state)
        .into_iter()
        .find(|c| c.target == Target::CheckUpdates)
        .unwrap();
    assert_eq!(xywh(check.rect), (196., 224., 820., 68.));
}

fn control(state: &State, target: Target) -> Rect {
    controls(state)
        .into_iter()
        .find(|c| c.target == target)
        .unwrap_or_else(|| panic!("{target:?} is drawn"))
        .rect
}

#[test]
fn the_gallery_fills_four_columns_with_upload_last() {
    let state = settings(1);
    assert_eq!(xywh(control(&state, Target::Background(0))), (196., 236., 390., 219.));
    assert_eq!(control(&state, Target::Background(1)).x, 614.);
    assert_eq!(control(&state, Target::Background(4)).y, 487.);
    let n = state.background_count();
    assert_eq!(
        xywh(control(&state, Target::UploadWallpaper)),
        (
            196. + (n % 4) as f32 * 418.,
            236. + (n / 4) as f32 * 251.,
            390.,
            219.
        )
    );
    let cycle = control(&state, Target::WallpaperCycle);
    let gradient = control(&state, Target::WallpaperGradient);
    assert_eq!(cycle.x + cycle.w, 1840.);
    assert!(gradient.x + gradient.w < cycle.x);
    assert!(cycle.y + cycle.h < 236. - 16., "{cycle:?}");
}

#[test]
fn the_remote_moves_on_the_four_column_grid() {
    let mut state = settings(1);
    let routes = [
        (Target::Background(0), Direction::Down, Target::Background(4)),
        (Target::Background(4), Direction::Up, Target::Background(0)),
        (Target::Background(0), Direction::Up, Target::WallpaperGradient),
        (Target::Background(3), Direction::Up, Target::WallpaperCycle),
        (Target::Background(4), Direction::Left, Target::Sidebar(1)),
        (Target::Background(3), Direction::Right, Target::Background(4)),
        (Target::WallpaperGradient, Direction::Down, Target::Background(2)),
        (Target::WallpaperCycle, Direction::Down, Target::Background(3)),
        (Target::WallpaperGradient, Direction::Left, Target::Sidebar(1)),
    ];
    for (from, direction, to) in routes {
        state.focus = from.clone();
        let c = controls(&state);
        state.direction(direction, &c);
        assert_eq!(state.focus, to, "from {from:?}");
        state.scroll = 0.;
    }
    state.focus = Target::Background(state.background_count() - 1);
    let c = controls(&state);
    state.direction(Direction::Right, &c);
    assert_eq!(state.focus, Target::UploadWallpaper);
    // The upload tile sits on the grid like any other.
    let c = controls(&state);
    state.direction(Direction::Up, &c);
    assert!(matches!(state.focus, Target::Background(_)));
}

#[test]
fn the_sidebar_enters_the_gallery_on_the_applied_background() {
    let mut state = settings(1);
    let c = controls(&state);
    state.direction(Direction::Right, &c);
    assert!(
        matches!(state.focus, Target::Background(i)
            if state.background_id(i) == Some(state.preferences.background.as_str())),
        "{:?}",
        state.focus
    );
}

#[test]
fn the_gallery_scrolls_under_its_title_through_a_mask() {
    let mut state = settings(1);
    // One row down: the first row has passed under the switches.
    state.scroll = 251.;
    let drawn = scene(&state);
    let viewport = state.backgrounds_viewport();
    assert!(viewport.y > 206. && viewport.y < 236.);
    assert!(drawn.draw.iter().any(|d| matches!(d, Draw::Clip(Some(r)) if r.y == viewport.y)));
    let first = control(&state, Target::Background(0));
    assert!(first.y + first.h < viewport.y);
    assert_ne!(
        state.hit_test(&drawn.controls, first.x + 20., first.y + first.h - 4.),
        Some(Target::Background(0))
    );
}
