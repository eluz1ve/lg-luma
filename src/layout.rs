//! The screen's frame, after tvOS: a safe area, fixed-width columns with a 40 px gutter, and the
//! text sizes everything on it uses (docs/superpowers/specs/2026-11-01-tv-layout-design.md).

/// Nothing that matters is drawn closer to the screen's sides.
pub const SAFE_X: f32 = 80.;
pub const GUTTER: f32 = 40.;
/// Width inside the safe area: exactly six 260 px columns and five gutters.
pub const WIDTH: f32 = 1920. - SAFE_X * 2.;
/// Below the tab bar, where the page starts; Home's header fills down to `HEADER_BOTTOM`.
pub const CONTENT_TOP: f32 = 150.;
pub const HEADER_BOTTOM: f32 = 400.;

/// One column's width when `count` of them span the safe area.
pub fn column(count: usize) -> f32 {
    let count = count.max(1) as f32;
    (WIDTH - GUTTER * (count - 1.)) / count
}

/// Settings: a sidebar that folds to a column of icons while the content has focus, an 820 px
/// list beside it, and Home drawn small on the right.
pub const SIDEBAR_COLLAPSED: f32 = 76.;
pub const SIDEBAR_EXPANDED: f32 = 360.;
pub const SIDEBAR_ITEM: f32 = 60.;
pub const SIDEBAR_PITCH: f32 = 68.;
pub const LIST_X: f32 = 196.;
pub const LIST_WIDTH: f32 = 820.;
/// Below the page's title.
pub const LIST_TOP: f32 = 224.;
pub const ROW_HEIGHT: f32 = 68.;
/// The focused row, with room for its description.
pub const ROW_GROWN: f32 = 104.;
pub const ROW_SPACING: f32 = 10.;
pub const PREVIEW_X: f32 = 1064.;
pub const PREVIEW_Y: f32 = 190.;
pub const PREVIEW_WIDTH: f32 = 776.;
pub const PREVIEW_HEIGHT: f32 = 437.;
/// Settings › Background: four columns from the list's left edge to the safe area's right.
pub const GALLERY_COLUMNS: usize = 4;
pub const GALLERY_TILE_WIDTH: f32 = 390.;
pub const GALLERY_TILE_HEIGHT: f32 = 219.;
pub const GALLERY_GAP_X: f32 = 28.;
pub const GALLERY_GAP_Y: f32 = 32.;
pub const GALLERY_TOP: f32 = 236.;

pub mod type_scale {
    pub const PAGE_TITLE: f32 = 44.;
    pub const GREETING: f32 = 76.;
    pub const ROW_TITLE: f32 = 28.;
    pub const BODY: f32 = 26.;
    pub const SECONDARY: f32 = 24.;
    pub const CAPTION: f32 = 22.;
    pub const EYEBROW: f32 = 22.;
}
