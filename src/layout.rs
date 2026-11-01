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

pub mod type_scale {
    pub const GREETING: f32 = 76.;
    pub const ROW_TITLE: f32 = 28.;
    pub const BODY: f32 = 26.;
    pub const SECONDARY: f32 = 24.;
    pub const CAPTION: f32 = 22.;
    pub const EYEBROW: f32 = 22.;
}
