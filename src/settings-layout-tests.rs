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
