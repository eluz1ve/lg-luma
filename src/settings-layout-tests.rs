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
