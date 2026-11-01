//! Screen geometry and styling mirror the web UI's 1920×1080 / 16px-rem layout.
use crate::{
    i18n::Language,
    model::{App, BACKGROUNDS, CATEGORIES, Input, InputMode, UiSize},
    renderer::{Color, Draw, TextStyle, WHITE},
    state::{
        APPS_COLUMNS, Control, HOME_FADE_LEFT, HOME_FADE_RIGHT, HOME_STRIP_LEFT, HOME_STRIP_RIGHT,
        HOME_TILE_PITCH, HOME_TILE_WIDTH, HOMEBREW_UPDATES_ROW, KEYS, Overlay, Page, Rect,
        SYSTEM_SECTION, State, Subpage, Target, TextField, WIDGETS_SECTION,
    },
    weather::{self, Unit},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use time::OffsetDateTime;

use crate::layout::{
    CONTENT_TOP, GUTTER, HEADER_BOTTOM, SAFE_X, WIDTH, column as column_width, type_scale,
};

const MAIN_TOP: f32 = 108.8;
const FACE_HEIGHT: f32 = 154.4;
const RECENTS_BACKDROP_OPACITY: f32 = 0.86;

#[path = "recents-view.rs"]
mod recents_view;

pub struct Assets {
    pub root: PathBuf,
    pub version: String,
    pub metrics: BTreeMap<String, BTreeMap<char, f32>>,
    pub artwork: crate::app_artwork::Service,
}
impl Assets {
    pub fn load(root: PathBuf, webos: bool) -> Result<Self, String> {
        let load = |file: &str| {
            std::fs::read(root.join(file))
                .map_err(|e| format!("Native assets are missing: {e}. Run pnpm assets."))
        };
        // Generated artwork may outlive a binary rebuild; version the running app, not the assets.
        let version = env!("CARGO_PKG_VERSION").to_owned();
        let metrics =
            serde_json::from_slice(&load("font-metrics.json")?).map_err(|e| e.to_string())?;
        Ok(Self {
            root,
            version,
            metrics,
            artwork: crate::app_artwork::Service::new(webos),
        })
    }
    fn path(&self, path: impl AsRef<Path>) -> String {
        self.root.join(path).to_string_lossy().into_owned()
    }

    /// A gallery card's picture; none for Apple TV until its place's still arrives.
    fn background_thumbnail(&self, state: &State, id: &str) -> Option<String> {
        if id == crate::aerials::ID {
            let place = &state.aerial_place().id;
            return state
                .aerials
                .iter()
                .find(|still| still.id == *place)
                .map(|still| still.thumbnail());
        }
        Some(
            state
                .wallpapers
                .iter()
                .find(|item| item.id == id)
                .map_or_else(
                    || self.path(format!("thumbnails/{id}.jpg")),
                    |item| item.thumbnail(),
                ),
        )
    }

    fn background(&self, state: &State, id: &str) -> String {
        // An Apple TV still: `aerials#place`.
        if let Some(place) = id
            .strip_prefix(crate::aerials::ID)
            .and_then(|rest| rest.strip_prefix('#'))
            && let Some(still) = state.aerials.iter().find(|still| still.id == place)
        {
            return if state.preferences.wallpaper_gradient {
                still.image()
            } else {
                still.unshaded_image()
            };
        }
        // A dynamic wallpaper's frame: `id#frame`.
        if let Some((id, frame)) = id.split_once('#')
            && let Some(item) = state.wallpapers.iter().find(|item| item.id == id)
            && let Ok(frame) = frame.parse::<usize>()
            && frame < item.frames.len()
        {
            return item.frame(frame, state.preferences.wallpaper_gradient);
        }
        let id = id.split_once('#').map_or(id, |(id, _)| id);
        if let Some(item) = state.wallpapers.iter().find(|item| item.id == id) {
            return if state.preferences.wallpaper_gradient {
                item.image()
            } else {
                item.unshaded_image()
            };
        }
        let id = if BACKGROUNDS.iter().any(|(known, _)| *known == id) {
            id
        } else {
            "dunes"
        };
        let suffix = if state.preferences.wallpaper_gradient {
            ""
        } else {
            "-plain"
        };
        self.path(format!("wallpapers/{id}{suffix}.jpg"))
    }

    fn width(&self, value: &str, style: TextStyle) -> f32 {
        let key = format!(
            "{}{}",
            if style.size >= 32. { "display-" } else { "" },
            style.weight
        );
        let advances = self.metrics.get(&key);
        let fallback = self.metrics.get(if style.weight >= 500 {
            "fallback-500"
        } else {
            "fallback-400"
        });
        value
            .chars()
            .map(|c| {
                (advances
                    .and_then(|a| a.get(&c))
                    .or_else(|| fallback.and_then(|a| a.get(&c)))
                    .copied()
                    .unwrap_or(if c == ' ' { 0.28 } else { 0.53 })
                    + style.tracking)
                    * style.size
            })
            .sum()
    }

    fn fit_text(&self, value: &str, mut style: TextStyle, width: f32) -> TextStyle {
        // Leave a pixel for floating-point differences from the renderer's
        // font advances. Re-measure if shrinking crosses the display-font
        // threshold at 32px, where a different optical font is selected.
        let available = width - 1.;
        for _ in 0..2 {
            let measured = self.width(value, style);
            if measured <= available {
                break;
            }
            style.size *= available / measured;
        }
        style
    }

    fn lines(&self, value: &str, style: TextStyle, width: f32) -> usize {
        let key = format!(
            "{}{}",
            if style.size >= 32. { "display-" } else { "" },
            style.weight
        );
        let advances = self.metrics.get(&key);
        let fallback = self.metrics.get(if style.weight >= 500 {
            "fallback-500"
        } else {
            "fallback-400"
        });
        let advance = |c: char| {
            (advances
                .and_then(|a| a.get(&c))
                .or_else(|| fallback.and_then(|a| a.get(&c)))
                .copied()
                .unwrap_or(if c == ' ' { 0.28 } else { 0.53 })
                + style.tracking)
                * style.size
        };
        let mut lines = 1;
        let mut used = 0.;
        for word in value.split_inclusive(char::is_whitespace) {
            let w: f32 = word.chars().map(advance).sum();
            if used > 0. && used + w > width {
                lines += 1;
                used = 0.;
            }
            for c in word.chars() {
                if c == '\n' {
                    lines += 1;
                    used = 0.;
                    continue;
                }
                let w = advance(c);
                if used > 0. && used + w > width {
                    lines += 1;
                    used = 0.;
                }
                used += w;
            }
        }
        lines
    }
}

pub struct Scene {
    pub draw: Vec<Draw>,
    pub controls: Vec<Control>,
    pub modal: Option<Rect>,
}
impl Scene {
    fn new() -> Self {
        Self {
            draw: Vec::new(),
            controls: Vec::new(),
            modal: None,
        }
    }
    fn shape(&mut self, rect: Rect, radius: f32, fill: Color) {
        self.draw.push(Draw::Shape(rect, radius, 0., fill));
    }
    fn border(&mut self, rect: Rect, radius: f32, width: f32, color: Color) {
        self.draw.push(Draw::Shape(rect, radius, width, color));
    }
    fn text(
        &mut self,
        text: impl Into<String>,
        rect: Rect,
        style: TextStyle,
        color: Color,
        center: bool,
    ) {
        let color = if style.shadow { solid(color) } else { color };
        self.draw
            .push(Draw::Text(text.into(), rect, style, color, center));
    }
    fn image(&mut self, path: String, rect: Rect, radius: f32, contain: bool) {
        self.draw.push(Draw::Image(path, rect, radius, contain));
    }
    fn icon(&mut self, assets: &Assets, name: &str, rect: Rect, color: Color) {
        self.draw.push(Draw::TintedImage(
            assets.path(format!("ui-icons/{name}.lvec")),
            rect,
            0.,
            true,
            color,
        ));
    }
    /// An icon straight on the wallpaper, over a soft shadow like its text's.
    fn shadowed_icon(&mut self, assets: &Assets, name: &str, rect: Rect, color: Color) {
        let path = assets.path(format!("ui-icons/{name}.lvec"));
        let (taps, alpha) = crate::renderer::shadow_taps(rect.h * 0.8);
        let shade = [0., 0., 0., alpha * color[3]];
        for [dx, dy] in taps {
            let rect = Rect::new(rect.x + dx, rect.y + dy, rect.w, rect.h);
            self.draw
                .push(Draw::TintedImage(path.clone(), rect, 0., true, shade));
        }
        self.draw
            .push(Draw::TintedImage(path, rect, 0., true, color));
    }
    fn control(&mut self, target: Target, rect: Rect, scrollable: bool) {
        self.controls.push(Control {
            target,
            rect,
            scrollable,
        });
    }
    fn ring(&mut self, rect: Rect, radius: f32) {
        self.border(rect.inset(-6.9), radius + 6.9, 3.7, c(0xffffff, 0.10));
        self.border(rect.inset(-3.2), radius + 3.2, 3.2, c(0xf0f0f0, 1.));
    }
    fn shadow(&mut self, rect: Rect, radius: f32, blur: f32, y: f32, alpha: f32) {
        self.draw.push(Draw::Shadow(
            Rect::new(rect.x, rect.y + y, rect.w, rect.h),
            radius,
            blur,
            c(0, alpha),
        ));
    }
    fn pill(&mut self, rect: Rect, focused: bool, selected: bool) {
        self.shadow(rect, rect.h / 2., 18., 4., 0.06);
        if focused {
            self.draw.push(Draw::RoundedGradient(
                rect,
                rect.h / 2.,
                c(0xfafafa, 1.),
                c(0xe5e5e5, 1.),
            ));
        } else {
            self.shape(
                rect,
                rect.h / 2.,
                if selected {
                    c(0x303438, 1.)
                } else {
                    c(0x171c20, 0.72)
                },
            );
            self.border(rect, rect.h / 2., 1., c(0xffffff, 0.07));
        }
    }
    fn fade(&mut self, from: usize, alpha: f32, brightness: f32) {
        for draw in &mut self.draw[from..] {
            let apply = |color: &mut Color| {
                for channel in &mut color[..3] {
                    *channel *= brightness;
                }
                color[3] *= alpha;
            };
            match draw {
                Draw::Shape(_, _, _, color)
                | Draw::Shadow(_, _, _, color)
                | Draw::Text(_, _, _, color, _)
                | Draw::TintedImage(_, _, _, _, color)
                | Draw::AppImage(_, _, _, color)
                | Draw::Picture(_, _, _, color)
                | Draw::Cover(_, _, _, color) => apply(color),
                Draw::RoundedGradient(_, _, a, b) => {
                    apply(a);
                    apply(b);
                }
                Draw::Image(path, rect, radius, contain) => {
                    *draw = Draw::TintedImage(
                        path.clone(),
                        *rect,
                        *radius,
                        *contain,
                        [brightness, brightness, brightness, alpha],
                    );
                }
                Draw::Clip(_) | Draw::EdgeFade(_) | Draw::EdgeFadeX(_) => {}
            }
        }
    }
}

/// The strings a companion dialog shows around its QR code, as message keys.
struct Companion {
    title: &'static str,
    keep_open: &'static str,
    private: &'static str,
    error: &'static str,
    starting: &'static str,
}

fn wallpaper_upload(out: &mut Scene, state: &State, assets: &Assets) {
    companion(
        out,
        state,
        assets,
        state.wallpaper_upload.as_ref(),
        Companion {
            title: "uploadWallpaper",
            keep_open: "wallpaperUploadKeepOpen",
            private: "wallpaperUploadPrivate",
            error: "wallpaperUploadError",
            starting: "wallpaperUploadStarting",
        },
    );
}

fn tile_editor(out: &mut Scene, state: &State, assets: &Assets) {
    companion(
        out,
        state,
        assets,
        state.tile_editor.as_ref(),
        Companion {
            title: "editTilesElsewhere",
            keep_open: "tileEditorKeepOpen",
            private: "tileEditorPrivate",
            error: "tileEditorError",
            starting: "tileEditorStarting",
        },
    );
}

/// A QR code and address for a page on the owner's phone or computer, with a Close button.
fn companion(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    session: Option<&Result<crate::companion::Session, String>>,
    strings: Companion,
) {
    let language = state.preferences.language;
    let panel = Rect::new(520., 144., 880., 816.);
    out.modal = Some(panel);
    out.shadow(panel, 24., 40., 12., 0.3);
    out.shape(panel, 24., c(0x1b2023, 1.));
    out.border(panel, 24., 1., c(0xffffff, 0.09));
    out.text(
        language.t(strings.title),
        Rect::new(584., 198., 752., 48.),
        text(32., 550, -0.02),
        WHITE,
        true,
    );
    out.text(
        language.t("wallpaperUploadHint"),
        Rect::new(604., 264., 712., 32.),
        text(19., 400, 0.).leading(1.5),
        c(0xffffff, 0.65),
        true,
    );
    match session {
        Some(Ok(session)) => {
            let scale = (336. / (session.qr_size + 8) as f32).floor();
            let size = (session.qr_size + 8) as f32 * scale;
            let x = (1920. - size) / 2.;
            let y = 310.;
            // Invert the normal QR polarity to blend into the dark modal, while
            // retaining a high-contrast foreground and the full quiet zone.
            let background = c(0x1b2023, 1.);
            let modules = c(0xf5f4ef, 1.);
            out.shape(Rect::new(x, y, size, size), 0., background);
            for row in 0..session.qr_size {
                // Merge contiguous modules so QR rendering stays a few hundred quads.
                let mut column = 0;
                while column < session.qr_size {
                    let mark_start = (session.qr_size - 11) / 2;
                    let in_mark = (mark_start..mark_start + 11).contains(&row)
                        && (mark_start..mark_start + 11).contains(&column);
                    if in_mark {
                        column += 1;
                        continue;
                    }
                    if !session.qr[(row * session.qr_size + column) as usize] {
                        column += 1;
                        continue;
                    }
                    let start = column;
                    while column < session.qr_size {
                        let in_mark = (mark_start..mark_start + 11).contains(&row)
                            && (mark_start..mark_start + 11).contains(&column);
                        if in_mark || !session.qr[(row * session.qr_size + column) as usize] {
                            break;
                        }
                        column += 1;
                    }
                    out.shape(
                        Rect::new(
                            x + (start + 4) as f32 * scale,
                            y + (row + 4) as f32 * scale,
                            (column - start) as f32 * scale,
                            scale,
                        ),
                        0.,
                        modules,
                    );
                }
            }
            let mark_start = (session.qr_size - 11) / 2;
            let mark_plate = Rect::new(
                x + (mark_start + 4) as f32 * scale,
                y + (mark_start + 4) as f32 * scale,
                11. * scale,
                11. * scale,
            );
            out.shape(mark_plate, 1.1 * scale, background);
            let mark = Rect::new(
                mark_plate.x + 1.25 * scale,
                mark_plate.y + 1.25 * scale,
                8.5 * scale,
                8.5 * scale,
            );
            out.icon(assets, "luma-mark", mark, WHITE);
            out.text(
                language.t("wallpaperUploadBrowser"),
                Rect::new(604., 670., 712., 32.),
                text(19., 400, 0.).leading(1.5),
                c(0xffffff, 0.65),
                true,
            );
            let display_url = session.url.trim_end_matches('/');
            out.text(
                display_url,
                Rect::new(568., 716., 784., 36.),
                assets.fit_text(display_url, text(26., 600, 0.), 784.),
                WHITE,
                true,
            );
            out.text(
                language.t(strings.keep_open),
                Rect::new(604., 766., 712., 52.),
                text(16., 400, 0.).leading(1.4),
                c(0xffffff, 0.75),
                true,
            );
            out.text(
                language.t(strings.private),
                Rect::new(604., 820., 712., 22.),
                text(15., 400, 0.),
                c(0xffffff, 0.5),
                true,
            );
        }
        result => {
            let key = if result.is_some() {
                strings.error
            } else {
                strings.starting
            };
            out.text(
                language.t(key),
                Rect::new(604., 470., 712., 90.),
                text(22., 450, 0.).leading(1.5),
                c(0xffffff, 0.75),
                true,
            );
        }
    }
    let close = Rect::new(812., 862., 296., 56.);
    out.pill(close, state.focus == Target::CloseDialog, false);
    out.text(
        language.t("close"),
        Rect::new(close.x, close.y + 15., close.w, 28.),
        text(20., 550, 0.),
        c(0x171c20, 1.),
        true,
    );
    out.control(Target::CloseDialog, close, false);
}

/// Shadowed text is drawn opaque: through a translucent glyph its own shadow would show, greying
/// it. It takes the light grey of its white blended over a mid-tone, so secondary text stays a
/// step below white without turning dark.
fn solid(color: Color) -> Color {
    const BACKDROP: f32 = 120. / 255.;
    let [r, g, b, a] = color;
    let over = |value: f32| value * a + BACKDROP * (1. - a);
    [over(r), over(g), over(b), 1.]
}
fn c(rgb: u32, alpha: f32) -> Color {
    [
        ((rgb >> 16) & 255) as f32 / 255.,
        ((rgb >> 8) & 255) as f32 / 255.,
        (rgb & 255) as f32 / 255.,
        alpha,
    ]
}
/// A gallery card's picture, or Apple TV's night-sky placeholder until its first still arrives.
fn background_card(out: &mut Scene, assets: &Assets, state: &State, id: &str, rect: Rect) {
    match assets.background_thumbnail(state, id) {
        Some(path) => out.image(path, rect, 16., false),
        None => out.draw.push(Draw::RoundedGradient(
            rect,
            16.,
            c(0x23344a, 1.),
            c(0x0b1119, 1.),
        )),
    }
}

/// Apple TV's card says what it is and which place it is on, over a shade at its foot.
fn aerials_caption(out: &mut Scene, assets: &Assets, state: &State, rect: Rect) {
    let shade = Rect::new(rect.x, rect.y + rect.h * 0.42, rect.w, rect.h * 0.58);
    out.draw.push(Draw::RoundedGradient(
        shade,
        16.,
        c(0x000000, 0.),
        c(0x000000, 0.66),
    ));
    // Clear of the check mark a chosen card carries in its corner.
    let width = rect.w - 36. - 34.;
    let place = state.aerial_place().name(state.preferences.language);
    let label = assets.fit_text(crate::aerials::LABEL, text(20., 600, -0.01), width);
    out.text(
        crate::aerials::LABEL,
        Rect::new(rect.x + 18., rect.y + rect.h - 66., width, 26.),
        label,
        WHITE,
        false,
    );
    let name = assets.fit_text(place, text(16., 450, 0.), width);
    out.text(
        place,
        Rect::new(rect.x + 18., rect.y + rect.h - 38., width, 22.),
        name,
        c(0xffffff, 0.78),
        false,
    );
}

fn text(size: f32, weight: u16, tracking: f32) -> TextStyle {
    TextStyle::new(size).weight(weight).tracking(tracking)
}

pub struct Navigation {
    pub surface: Rect,
    pub items: [Rect; 3],
}
pub fn navigation(assets: &Assets, language: Language) -> Navigation {
    let widths = ["home", "apps", "settings"]
        .map(|key| (assets.width(language.t(key), text(20.8, 550, -0.02)) + 46.4).max(99.2));
    let width = widths.iter().sum::<f32>() + 25.6 + 19.6;
    let mut x = (1920. - width) / 2. + 9.8;
    let items = widths.map(|width| {
        let rect = Rect::new(x, 52.3, width, 49.);
        x += width + 12.8;
        rect
    });
    Navigation {
        surface: Rect::new((1920. - width) / 2., 42.5, width, 68.6),
        items,
    }
}

/// Fully shown Recents is drawn instead of the page, which is then not part of the scene.
pub fn recents_replaces_page(state: &State) -> bool {
    state.recents_open() && state.recents.visibility == 1.
}

pub fn scene_with_pill(
    state: &State,
    assets: &Assets,
    pill_x: f32,
    pill_width: f32,
    now: OffsetDateTime,
) -> Scene {
    let mut out = Scene::new();
    if let Some((previous, start)) = &state.previous_background {
        out.image(
            assets.background(state, previous),
            Rect::new(0., 0., 1920., 1080.),
            0.,
            false,
        );
        let alpha = (start.elapsed().as_secs_f32() / state.background_fade).min(1.);
        out.draw.push(Draw::TintedImage(
            assets.background(state, &state.background_key()),
            Rect::new(0., 0., 1920., 1080.),
            0.,
            false,
            [1., 1., 1., alpha],
        ));
    } else {
        out.image(
            assets.background(state, &state.background_key()),
            Rect::new(0., 0., 1920., 1080.),
            0.,
            false,
        );
    }
    let content_start = out.draw.len();
    if recents_replaces_page(state) {
        out.shape(
            Rect::new(0., 0., 1920., 1080.),
            0.,
            c(0x080b12, RECENTS_BACKDROP_OPACITY),
        );
        recents_view::render(&mut out, state, assets);
        if let Some((notice, _)) = &state.notice {
            toast(&mut out, state, assets, notice);
        }
        return out;
    }
    let viewport = state.content_viewport();
    let main_bottom = viewport.y + viewport.h;
    if state.page == Page::Home {
        out.draw.push(Draw::Clip(Some(viewport)));
    }
    match state.page {
        Page::Home => home(&mut out, state, assets, now),
        Page::Apps => library(&mut out, state, assets),
        Page::Settings => settings(&mut out, state, assets),
    }
    if let Some(error) = &state.error
        && !state.inventory.apps.is_empty()
        && !out
            .controls
            .iter()
            .any(|control| control.target == Target::Retry)
    {
        let rect = Rect::new(84., main_bottom - 66., 1752., 54.);
        out.shape(rect, 11.2, c(0xad7c3e, 0.07));
        out.border(rect, 11.2, 1., c(0xedc596, 0.19));
        out.icon(
            assets,
            "alert",
            Rect::new(104., rect.y + 18., 18., 18.),
            c(0xf3ddbd, 1.),
        );
        out.text(
            error,
            Rect::new(138., rect.y + 16., 1500., 35.),
            text(14.4, 400, 0.),
            c(0xf3ddbd, 1.),
            false,
        );
        let retry = Rect::new(1720., rect.y + 8., 96., 38.);
        out.text(
            state.preferences.language.t("retry"),
            Rect::new(retry.x, retry.y + 10., retry.w, 20.),
            text(14.4, 500, 0.),
            c(0xf3ddbd, 1.),
            true,
        );
        if state.focus == Target::Retry {
            out.border(retry, 6.4, 2., WHITE);
        }
        out.control(Target::Retry, retry, false);
    }
    out.draw.push(Draw::EdgeFade(None));
    out.draw.push(Draw::Clip(None));
    header(&mut out, state, assets, now, pill_x, pill_width);
    if let Some(
        overlay @ (Overlay::Options { .. }
        | Overlay::Delete { .. }
        | Overlay::BackgroundOptions { .. }
        | Overlay::BackgroundDelete { .. }
        | Overlay::Keyboard
        | Overlay::WallpaperUpload
        | Overlay::TileEditor
        | Overlay::Cs2Match { .. }),
    ) = &state.overlay
    {
        let anchor = match overlay {
            Overlay::BackgroundOptions { id, .. } => out.controls.iter()
                .find(|control| matches!(control.target, Target::Background(index) if state.background_id(index) == Some(id.as_str())))
                .map(|control| Rect::new(control.rect.x, control.rect.y, control.rect.w, 202.5)),
            Overlay::Options { id, .. } => out
                .controls
                .iter()
                .find(|control| control.target == Target::App(id.clone()))
                .map(|control| {
                    // Hit regions include captions, artwork does not. Reusing the
                    // full hit region here stretches the lifted card vertically.
                    let mut rect = control.rect;
                    // Home names only its focused card, outside the hit region.
                    let names = state.page != Page::Home
                        && state.preferences.app_names == crate::model::AppNames::Always;
                    if names
                        && let Some(app) = state.inventory.apps.iter().find(|app| &app.id == id)
                    {
                        rect.h -= 18.4 + caption_height(state, assets, app, rect.w);
                    }
                    rect
                }),
            _ => None,
        };
        out.controls.clear();
        let start = out.draw.len();
        out.shape(Rect::new(0., 0., 1920., 1080.), 0., c(0x080b12, 0.70));
        if matches!(
            overlay,
            Overlay::Options { .. } | Overlay::BackgroundOptions { .. }
        ) && let Some(opened) = state.panel_started
        {
            // The lifted card stays opaque while the backdrop and popup fade.
            // Fading it over its dimmed parent copy causes a brightness dip.
            out.fade(start, (opened.elapsed().as_secs_f32() / 0.12).min(1.), 1.);
        }
        match overlay {
            Overlay::Options { id, .. } | Overlay::BackgroundOptions { id, .. } => {
                options(&mut out, state, assets, id, anchor)
            }
            Overlay::Delete { id, .. } | Overlay::BackgroundDelete { id, .. } => {
                delete_confirmation(&mut out, state, assets, id)
            }
            Overlay::Keyboard => keyboard(&mut out, state, assets),
            Overlay::WallpaperUpload => wallpaper_upload(&mut out, state, assets),
            Overlay::TileEditor => tile_editor(&mut out, state, assets),
            Overlay::Cs2Match { id, .. } => cs2_dialog(&mut out, state, assets, id, now),
            Overlay::SystemKeyboard | Overlay::Recents { .. } => {}
        }
        if !matches!(
            overlay,
            Overlay::Options { .. } | Overlay::BackgroundOptions { .. }
        ) && let Some(opened) = state.panel_started
        {
            out.fade(start, (opened.elapsed().as_secs_f32() / 0.12).min(1.), 1.);
        }
    }
    if state.recents_open() || state.recents.visibility > 0. {
        // Crossfade the actual parent interface as well as the dimmed stage.
        // Retain outgoing visuals after dismissal, while input immediately
        // belongs to the restored parent. No geometry or hit targets move.
        let parent_controls = std::mem::take(&mut out.controls);
        let parent_modal = out.modal;
        out.fade(content_start, 1. - state.recents.visibility, 1.);
        let start = out.draw.len();
        out.shape(
            Rect::new(0., 0., 1920., 1080.),
            0.,
            c(0x080b12, RECENTS_BACKDROP_OPACITY),
        );
        recents_view::render(&mut out, state, assets);
        out.fade(start, state.recents.visibility, 1.);
        // Dim the wallpaper beneath the outgoing interface rather than over it. While both
        // fade, the backdrop and Recents' full-screen gradient then lie directly on the
        // wallpaper, and the renderer folds them into it instead of two more full-screen
        // passes per frame of the crossfade.
        let fills = out.draw[start..]
            .iter()
            .take_while(|draw| full_screen_fill(draw))
            .count();
        let moved: Vec<_> = out.draw.drain(start..start + fills).collect();
        out.draw.splice(content_start..content_start, moved);
        if !state.recents_open() {
            out.controls = parent_controls;
            out.modal = parent_modal;
        }
    }
    if let Some((notice, _)) = &state.notice {
        toast(&mut out, state, assets, notice);
    }
    out
}

fn header(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    now: OffsetDateTime,
    pill_x: f32,
    pill_width: f32,
) {
    let language = state.preferences.language;
    let clock = language.clock(
        now,
        state.preferences.hour_24,
        state.preferences.show_seconds,
    );
    let k = state.preferences.ui_scale.scale();
    out.text(
        clock,
        Rect::new(SAFE_X, 76.8 - 14.4 * k, 300. * k, 28.8 * k),
        text(24. * k, 500, -0.03).shadow(),
        WHITE,
        false,
    );
    let layout = navigation(assets, language);
    let nav = layout.surface;
    out.shadow(nav, 48., 18., 4., 0.06);
    out.shape(nav, 48., c(0x171c20, 0.72));
    out.border(nav, 48., 1., c(0xffffff, 0.07));
    let pill = Rect::new(pill_x, 52.3, pill_width, 49.);
    out.draw.push(Draw::RoundedGradient(
        pill,
        32.,
        c(0xfafafa, 1.),
        c(0xe5e5e5, 1.),
    ));
    if matches!(state.focus, Target::Nav(_)) && !state.pointer {
        // Follow the pill's animated position and width instead of jumping to
        // the destination tab's control rectangle.
        out.border(pill.inset(-5.), 37., 2., c(0xf0f0f0, 0.95));
    }
    for (index, key) in ["home", "apps", "settings"].into_iter().enumerate() {
        let rect = layout.items[index];
        out.text(
            language.t(key),
            Rect::new(rect.x, 64.3, rect.w, 25.),
            text(20.8, 550, -0.02),
            if state.page_index() == index {
                c(0x2c2c2c, 1.)
            } else {
                c(0xeeeeee, 0.73)
            },
            true,
        );
        out.control(Target::Nav(index), rect, false);
    }
    let button_size = 60.;
    let rect = Rect::new(
        SAFE_X + WIDTH - button_size,
        nav.y + (nav.h - button_size) / 2.,
        button_size,
        button_size,
    );
    let focused = state.focus == Target::TvSettings;
    out.pill(rect, focused, false);
    out.icon(
        assets,
        "settings",
        Rect::new(
            rect.x + (rect.w - 23.2) / 2.,
            rect.y + (rect.h - 23.2) / 2.,
            23.2,
            23.2,
        ),
        if focused {
            c(0x2c2c2c, 1.)
        } else {
            c(0xeeeeee, 0.73)
        },
    );
    out.control(Target::TvSettings, rect, false);
}

/// Home's shared focus lift (`motion.rs`): a card grows and rises toward focus, with a soft glow.
/// Returns where to draw the card and how far it has lifted; its hit target stays at `rect`.
fn lift(out: &mut Scene, state: &State, target: &Target, rect: Rect, radius: f32) -> (Rect, f32) {
    let amount = state.motion.focus(target);
    let grow = rect.w.min(rect.h) * 0.07 * amount;
    let rect = Rect::new(
        rect.x - grow / 2.,
        rect.y - grow / 2. - 5. * amount,
        rect.w + grow,
        rect.h + grow,
    );
    out.shadow(
        rect,
        radius,
        4. + 30. * amount,
        1. + 13. * amount,
        0.01 + 0.39 * amount,
    );
    focus_glow(out, state, rect, radius, amount);
    (rect, amount)
}

/// The focused card's glow in the colour chosen in Settings › General: two inexpensive GPU
/// distance-field glows, with no texture blur or CPU readback.
fn focus_glow(out: &mut Scene, state: &State, rect: Rect, radius: f32, amount: f32) {
    let Some(rgb) = state.preferences.focus_glow.rgb().filter(|_| amount > 0.) else {
        return;
    };
    out.draw.push(Draw::Shadow(
        rect.inset(-3.),
        radius + 3.,
        32.,
        c(rgb, 0.2 * amount),
    ));
    out.draw
        .push(Draw::Shadow(rect, radius, 10., c(rgb, 0.24 * amount)));
}

/// The lifted card's edge, fading in with the lift.
fn lift_edge(out: &mut Scene, rect: Rect, radius: f32, amount: f32) {
    if amount > 0. {
        out.border(rect, radius, 1.4, c(0xffffff, 0.5 * amount));
    }
}

/// As on Apple TV, the focused card is named on one line below it, centred, as wide as a
/// column and its gutters; other cards go unnamed.
fn focused_name(out: &mut Scene, state: &State, assets: &Assets, app: &App, rect: Rect) {
    if state.focus != Target::App(app.id.clone()) {
        return;
    }
    let style = text(type_scale::CAPTION * caption_scale(state), 600, -0.01).shadow();
    let room = rect.w + 2. * GUTTER;
    out.text(
        &app.title,
        Rect::new(
            rect.x + (rect.w - room) / 2.,
            rect.y + rect.h + 14.,
            room,
            style.size * 1.2,
        ),
        assets.fit_text(&app.title, style, room),
        WHITE,
        true,
    );
}

fn tile(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    app: &App,
    rect: Rect,
    names: bool,
    pinned: bool,
) {
    let caption_height = if names {
        caption_height(state, assets, app, rect.w)
    } else {
        0.
    };
    let label_height = if names { 18.4 + caption_height } else { 0. };
    out.control(
        Target::App(app.id.clone()),
        Rect::new(rect.x, rect.y, rect.w, rect.h + label_height),
        true,
    );
    // Home's favorites are drawn where their scroll has eased to; hit targets stay put.
    let rect = if state.page == Page::Home {
        Rect::new(
            rect.x + state.home_scroll
                - state
                    .motion
                    .shown(crate::motion::Lane::Favorites, state.home_scroll),
            rect.y,
            rect.w,
            rect.h,
        )
    } else {
        rect
    };
    let viewport = if state.page == Page::Home {
        state.favorites_viewport()
    } else {
        state.content_viewport()
    };
    let top = if state.page == Page::Apps {
        0.
    } else {
        MAIN_TOP
    };
    if rect.y + rect.h + label_height < top
        || rect.y > 1054.
        || rect.x + rect.w + 40. < viewport.x
        || rect.x - 40. > viewport.x + viewport.w
    {
        return;
    }
    let start = out.draw.len();
    let focused = state.focus == Target::App(app.id.clone());
    let home = state.page == Page::Home;
    let amount = if home {
        state.motion.focus(&Target::App(app.id.clone()))
    } else if focused {
        1.
    } else {
        0.
    };
    let base_rect = rect;
    // Keep hitboxes and navigation geometry fixed, including when using the Magic Remote.
    let rect = if home {
        let scale = 1. + 0.07 * amount;
        Rect::new(
            rect.x - rect.w * (scale - 1.) / 2.,
            rect.y - rect.h * (scale - 1.) / 2. - 5. * amount,
            rect.w * scale,
            rect.h * scale,
        )
    } else {
        rect
    };
    out.shadow(
        rect,
        19.2,
        22. + 12. * amount,
        9. + 5. * amount,
        0.165 + 0.235 * amount,
    );

    if home {
        focus_glow(out, state, rect, 19.2, amount);
    }
    let favorite_color = app_face(out, assets, app, rect, state.preferences.language, 19.2);

    if pinned {
        favorite_badge(out, assets, rect, favorite_color);
    }
    if state.moving.as_ref() == Some(&app.id) {
        let indicator = Rect::new(
            rect.x + rect.w / 2. - 37.6,
            rect.y + rect.h - 33.6,
            75.2,
            24.,
        );
        out.shape(indicator, 16., c(0x101318, 0.79));
        out.icon(
            assets,
            "left",
            Rect::new(indicator.x + 8.8, indicator.y + 4., 16., 16.),
            c(0xededed, 1.),
        );
        out.icon(
            assets,
            "arrow",
            Rect::new(indicator.x + 40., indicator.y + 4., 16., 16.),
            c(0xededed, 1.),
        );
    }
    if home {
        lift_edge(out, rect, 19.2, amount);
    } else if focused {
        out.ring(rect, 19.2);
    }
    if state.page == Page::Apps && state.filter == 5 {
        out.fade(start, 0.45, 1.);
    }
    if state.pressed.as_ref() == Some(&Target::App(app.id.clone())) {
        out.fade(start, 1., 0.85);
    }
    // Labels remain anchored so toggling focus does not shift text or neighbouring controls.
    let rect = base_rect;
    if names {
        out.text(
            format!(
                "{}{}",
                app.title,
                if state.filter == 5 {
                    format!(" · {}", state.preferences.language.t("hiddenLabel"))
                } else {
                    String::new()
                }
            ),
            Rect::new(
                rect.x + 4.,
                rect.y + rect.h + 18.4,
                rect.w - 8.,
                caption_height,
            ),
            text(
                20.8 * caption_scale(state),
                if focused { 600 } else { 550 },
                -0.02,
            )
            .leading(1.4)
            .shadow(),
            if focused {
                c(0xffffff, 1.)
            } else {
                c(0xececec, 0.66)
            },
            true,
        );
    }
    if state.moving.is_some() && state.moving.as_ref() != Some(&app.id) {
        out.fade(start, 0.48, 1.);
    }
}

fn favorite_badge(out: &mut Scene, assets: &Assets, card: Rect, color: Color) {
    let icon = Rect::new(card.x + card.w - 27.2, card.y + 11.2, 16., 16.);
    out.icon(assets, "heart-filled", icon, color);
}

/// Shared card artwork for Home, Apps and Recents. Curated vectors always take
/// precedence over installed artwork, with the same relative logo proportions.
fn app_face(
    out: &mut Scene,
    assets: &Assets,
    app: &App,
    rect: Rect,
    language: Language,
    radius: f32,
) -> Color {
    let logo = crate::branding::for_key(&app.brand);
    let artwork = logo
        .is_none()
        .then(|| {
            app.icon
                .as_ref()
                .and_then(|source| assets.artwork.get(source))
        })
        .flatten();
    // The heart's centre is 19.2px below the top, outside the inset installed icon.
    // Bundled and generic surfaces are dark, so their badge remains white.
    let favorite_color = if artwork
        .as_ref()
        .is_some_and(|image| image.dark_badge(19.2 / rect.h))
    {
        c(0x000000, 1.)
    } else {
        c(0xffffff, 1.)
    };
    let installed = artwork.is_some();
    if let Some(image) = artwork {
        out.draw.push(Draw::AppImage(image, rect, radius, [1.; 4]));
    } else {
        out.image(
            assets.path(format!("surfaces/{}.png", app.brand)),
            rect,
            radius,
            false,
        );
    }
    out.border(rect, radius, 1., c(0xffffff, 0.063));
    if let Some(brand) = logo {
        let (ratio, height) = brand.artwork_size();
        let height = height * rect.h / FACE_HEIGHT;
        out.image(
            assets.path(format!("logos/{}.lvec", app.brand)),
            Rect::new(
                rect.x + rect.w * (1. - ratio) / 2.,
                rect.y + (rect.h - height) / 2.,
                rect.w * ratio,
                height,
            ),
            0.,
            true,
        );
    } else if !installed {
        brand_mark(out, assets, app, rect, language);
    }
    favorite_color
}

fn caption_height(state: &State, assets: &Assets, app: &App, width: f32) -> f32 {
    let title = format!(
        "{}{}",
        app.title,
        if state.filter == 5 {
            format!(" · {}", state.preferences.language.t("hiddenLabel"))
        } else {
            String::new()
        }
    );
    let style = text(
        20.8 * caption_scale(state),
        if state.focus == Target::App(app.id.clone()) {
            600
        } else {
            550
        },
        -0.02,
    )
    .leading(1.4);
    (assets.lines(&title, style, width - 8.) as f32 * style.size * style.line_height)
        .max(58.24 * caption_scale(state))
}

fn caption_scale(state: &State) -> f32 {
    state.preferences.ui_scale.scale()
}

fn brand_mark(out: &mut Scene, assets: &Assets, app: &App, rect: Rect, language: Language) {
    let scale = rect.h / FACE_HEIGHT;
    let rect = Rect::new(
        rect.x / scale,
        rect.y / scale,
        rect.w / scale,
        rect.h / scale,
    );
    let start = out.draw.len();
    let (cx, cy) = rect.center();
    match app.brand.as_str() {
        "plex" => {
            out.text(
                "ple",
                Rect::new(cx - 63., cy - 35.2, 130., 70.4),
                text(70.4, 650, -0.08).leading(1.),
                WHITE,
                false,
            );
            out.text(
                "x",
                Rect::new(cx + 20., cy - 35.2, 60., 70.4),
                text(70.4, 800, -0.08).leading(1.),
                c(0xe6aa25, 1.),
                false,
            );
        }
        "spotify" => {
            out.icon(
                assets,
                "spotify",
                Rect::new(cx - 77., cy - 20., 40., 40.),
                c(0x5ae08c, 1.),
            );
            out.text(
                "Spotify",
                Rect::new(cx - 29., cy - 14.4, 120., 36.),
                text(28.8, 650, -0.04).leading(1.),
                c(0x5ae08c, 1.),
                false,
            );
        }
        "kodi" => {
            out.icon(
                assets,
                "kodi",
                Rect::new(cx - 76., cy - 22.5, 45., 45.),
                WHITE,
            );
            out.text(
                "kodi",
                Rect::new(cx - 18., cy - 24.8, 130., 60.),
                text(49.6, 350, -0.04).leading(1.),
                WHITE,
                false,
            );
        }
        brand => {
            let (icon, label) = match brand {
                "live-tv" => ("tv", language.t("liveTV")),
                "homebrew" => ("coffee", language.t("homebrew")),
                "browser" => ("globe", language.t("browser")),
                "moonlight" => ("moonlight", "Moonlight"),
                "gaming" => ("gaming", app.title.as_str()),
                "custom" => (app.mark, app.title.as_str()),
                _ => ("generic", app.title.as_str()),
            };
            let font = if brand == "generic" {
                text(18.4, 500, -0.035).leading(1.35)
            } else {
                text(24., 500, -0.035).leading(1.2)
            };
            let w = (assets.width(label, font) + 54.4).min(rect.w - 32.);
            let label_w = rect.w - 54.4 - 32.;
            let label_h =
                (assets.lines(label, font, label_w) as f32 * font.size * font.line_height)
                    .min(rect.h - 32.);
            out.icon(
                assets,
                icon,
                Rect::new(cx - w / 2., cy - 20.8, 41.6, 41.6),
                WHITE,
            );
            out.text(
                label,
                Rect::new(cx - w / 2. + 54.4, cy - label_h / 2., label_w, label_h),
                font,
                WHITE,
                false,
            );
        }
    }
    if scale != 1. {
        for draw in &mut out.draw[start..] {
            let rect = match draw {
                Draw::Text(_, rect, style, _, _) => {
                    style.size *= scale;
                    rect
                }
                Draw::TintedImage(_, rect, _, _, _) => rect,
                _ => continue,
            };
            *rect = Rect::new(
                rect.x * scale,
                rect.y * scale,
                rect.w * scale,
                rect.h * scale,
            );
        }
    }
}

/// Where the card beside the greeting sits: the header's right columns, from the fifth's gutter.
pub(crate) const TODAY_RECT: Rect =
    Rect::new(1160., CONTENT_TOP, 680., HEADER_BOTTOM - CONTENT_TOP);
/// A row's heading, then its cards this far below the heading's top.
const ROW_HEADING_GAP: f32 = 50.;
/// Between one row's bottom and the next row's heading.
const ROW_GAP: f32 = 48.;
/// Room under the favorites for the focused card's name.
const CAPTION_ROOM: f32 = 40.;
const HOME_FACE_HEIGHT: f32 = 150.;
/// Inputs and crypto prices: four chips a row.
const CHIP_HEIGHT: f32 = 96.;
const CHIP_ROW_PITCH: f32 = 120.;

/// A Home row's heading, straight on the wallpaper.
fn row_heading(out: &mut Scene, label: &str, y: f32) -> TextStyle {
    let style = text(type_scale::ROW_TITLE, 600, -0.02).shadow();
    out.text(
        label,
        Rect::new(SAFE_X, y, 1000., type_scale::ROW_TITLE * 1.2),
        style,
        WHITE,
        false,
    );
    style
}

fn home(out: &mut Scene, state: &State, assets: &Assets, now: OffsetDateTime) {
    let language = state.preferences.language;
    let s = state.page_scroll();
    // The header's left column yields to the card beside it.
    let today = today_lines(state, now);
    let column = if !today.lines.is_empty() {
        TODAY_RECT.x - GUTTER - SAFE_X
    } else {
        WIDTH
    };
    let summary = state
        .preferences
        .show_weather
        .then(|| weather_summary_layout(state, assets, column));
    let date_height = if state.preferences.show_date {
        type_scale::EYEBROW * 1.2 + 14.
    } else {
        0.
    };
    let greeting_line = (type_scale::GREETING * 1.05).ceil();
    let greeting_height = if state.preferences.show_greeting {
        greeting_line + 16.
    } else {
        0.
    };
    // Centred in the header, whose height is fixed: the favorites stay put.
    let block = date_height + greeting_height + summary.as_ref().map_or(0., |s| s.height);
    // Centred, and never past the header's bottom onto the favorites.
    let top = (CONTENT_TOP + ((HEADER_BOTTOM - CONTENT_TOP - block) / 2.).max(0.))
        .min(HEADER_BOTTOM - block)
        - s;
    if state.preferences.show_date {
        out.text(
            state.preferences.date_format.format(language, now),
            Rect::new(SAFE_X, top, column, type_scale::EYEBROW * 1.2),
            text(type_scale::EYEBROW, 600, 0.12).shadow(),
            c(0xf5f5f5, 0.72),
            false,
        );
    }
    if state.preferences.show_greeting {
        let greeting = language.greeting(now.hour(), &state.preferences.greeting_name);
        let style = text(type_scale::GREETING, 600, -0.04)
            .leading(1.05)
            .shadow();
        let fitted = assets.fit_text(&greeting, style, column);
        let height = greeting_line * fitted.size / style.size;
        out.text(
            &greeting,
            Rect::new(
                SAFE_X,
                top + date_height + (greeting_line - height) / 2.,
                column,
                height,
            ),
            fitted,
            WHITE,
            false,
        );
    }
    if let Some(summary) = summary {
        weather_summary(
            out,
            state,
            assets,
            summary,
            top + date_height + greeting_height,
        );
    }
    let heading_y = HEADER_BOTTOM + 16. - s;
    row_heading(out, language.t("favorites"), heading_y);
    if state.moving.is_some() {
        out.text(
            language.t("moveApps"),
            Rect::new(SAFE_X + WIDTH - 600., heading_y + 4., 600., 28.),
            text(type_scale::CAPTION, 450, 0.).right().shadow(),
            c(0xdedede, 1.),
            false,
        );
    }
    let cards_y = heading_y + ROW_HEADING_GAP;
    let apps = state.apps();
    let names = state.preferences.show_home_names;
    let mut grid_height = HOME_FACE_HEIGHT;
    if state.loading && state.inventory.apps.is_empty() {
        for index in 0..6 {
            out.shape(
                Rect::new(
                    SAFE_X + index as f32 * HOME_TILE_PITCH,
                    cards_y,
                    HOME_TILE_WIDTH,
                    HOME_FACE_HEIGHT,
                ),
                19.2,
                c(0xffffff, 0.06),
            );
        }
    } else if apps.is_empty() {
        let error = state.error.is_some() && state.inventory.apps.is_empty();
        empty(
            out,
            state,
            assets,
            EmptyState {
                rect: Rect::new(SAFE_X, cards_y - 11.2, WIDTH, 258.7),
                icon: if error { "alert" } else { "heart" },
                title: if error {
                    language.t("aLittleHelp")
                } else {
                    language.t("makeFavorites")
                },
                hint: if error {
                    state.error.as_deref().unwrap_or("")
                } else {
                    language.t("pinHint")
                },
                action: Some(if error { Target::Retry } else { Target::Browse }),
            },
        );
        grid_height = 247.5;
    }
    if !apps.is_empty() && names {
        grid_height += CAPTION_ROOM;
    }
    // Like Apple TV's rows, the favourites run to the screen edge: the next tile shows cut off
    // at the right (and a scrolled-away one at the left), so it is clear the row continues.
    out.draw.push(Draw::Clip(Some(state.favorites_viewport())));
    // Soft edges: out to the right from the end of the sixth tile, and in from the left once
    // the row has scrolled.
    let left = if state
        .motion
        .shown(crate::motion::Lane::Favorites, state.home_scroll)
        > 0.
    {
        [HOME_FADE_LEFT, HOME_STRIP_LEFT]
    } else {
        [0., 0.]
    };
    out.draw.push(Draw::EdgeFadeX(Some([
        left[0],
        left[1],
        HOME_STRIP_RIGHT,
        HOME_FADE_RIGHT,
    ])));
    for (index, app) in apps.iter().enumerate() {
        let rect = Rect::new(
            SAFE_X + index as f32 * HOME_TILE_PITCH - state.home_scroll,
            cards_y,
            HOME_TILE_WIDTH,
            HOME_FACE_HEIGHT,
        );
        tile(out, state, assets, app, rect, false, false);
        if names {
            let x = rect.x + state.home_scroll
                - state
                    .motion
                    .shown(crate::motion::Lane::Favorites, state.home_scroll);
            focused_name(
                out,
                state,
                assets,
                app,
                Rect::new(x, rect.y, rect.w, rect.h),
            );
        }
    }
    out.draw.push(Draw::EdgeFadeX(None));
    out.draw.push(Draw::Clip(Some(state.content_viewport())));
    let input_y = cards_y + grid_height + ROW_GAP;
    let inputs: Vec<_> = state
        .inventory
        .inputs
        .iter()
        .filter(|input| state.preferences.input_visible(input))
        .collect();
    if !inputs.is_empty() {
        row_heading(out, language.t("inputsHeading"), input_y);
    }
    let chip_width = column_width(4);
    for (index, input) in inputs.iter().enumerate() {
        let rect = Rect::new(
            SAFE_X + (index % 4) as f32 * (chip_width + GUTTER),
            input_y + ROW_HEADING_GAP + (index / 4) as f32 * CHIP_ROW_PITCH,
            chip_width,
            CHIP_HEIGHT,
        );
        let focused = state.focus == Target::Input(input.id.clone());
        let hit = rect;
        let (rect, lift_amount) = lift(out, state, &Target::Input(input.id.clone()), rect, 18.4);
        out.draw.push(Draw::RoundedGradient(
            rect,
            18.4,
            if focused {
                c(0x30363c, 0.70)
            } else {
                c(0x20252b, 0.60)
            },
            if focused {
                c(0x242a30, 0.65)
            } else {
                c(0x171c22, 0.55)
            },
        ));
        out.border(
            rect,
            18.4,
            1.,
            c(0xffffff, if focused { 0.24 } else { 0.063 }),
        );
        let art = Rect::new(rect.x + 20., rect.y + 20., 56., 56.);
        out.draw.push(Draw::RoundedGradient(
            art,
            12.8,
            c(0xd6d6d6, 0.24),
            c(0xa0a0a0, 0.10),
        ));
        let name = state.preferences.input_name(input);
        let icon = input_icon(input, name);
        let size = if icon == "apple-tv" { 41.6 } else { 35.2 };
        out.icon(
            assets,
            &format!("input-{icon}"),
            Rect::new(
                art.x + (56. - size) / 2.,
                art.y + (56. - size) / 2.,
                size,
                size,
            ),
            c(0xdedede, 1.),
        );
        // The status line grows with the setting, the larger title by half as much.
        let k = state.preferences.ui_scale.scale();
        let title_k = 1. + (k - 1.) / 2.;
        let title_line = type_scale::BODY * 1.2 * title_k;
        let status_line = type_scale::CAPTION * 1.2 * k;
        let top = (CHIP_HEIGHT - title_line - status_line - 2.) / 2.;
        let width = rect.w - 92. - 20.;
        let title = state.preferences.input_title(input);
        let title_style = text(type_scale::BODY * title_k, 550, -0.02);
        out.text(
            title,
            Rect::new(rect.x + 92., rect.y + top, width, title_line),
            assets.fit_text(title, title_style, width),
            WHITE,
            false,
        );
        let status = input_status(input, name.is_some(), language);
        let status_style = text(type_scale::CAPTION * k, 450, 0.);
        out.text(
            &status,
            Rect::new(
                rect.x + 92.,
                rect.y + top + title_line + 2.,
                width,
                status_line,
            ),
            assets.fit_text(&status, status_style, width),
            c(0xe8e8e8, 0.62),
            false,
        );
        lift_edge(out, rect, 18.4, lift_amount);
        out.control(Target::Input(input.id.clone()), hit, true);
    }
    // Crypto prices, CS2 Score and TV Shows, stacked below the inputs.
    let rows = inputs.len().div_ceil(4) as f32;
    let mut y = if inputs.is_empty() {
        input_y
    } else {
        input_y + ROW_HEADING_GAP + (rows - 1.) * CHIP_ROW_PITCH + CHIP_HEIGHT + ROW_GAP
    };
    today_card(out, state, assets, &today, now);
    // Prices in the Today card have no row of their own.
    if state.preferences.show_crypto
        && !state.preferences.crypto_beside
        && !state.preferences.crypto_coins.is_empty()
    {
        crypto_row(out, state, y);
        y += ROW_HEADING_GAP + CHIP_HEIGHT + ROW_GAP;
    }
    // Widgets in the Today card have no row of their own.
    if state.preferences.show_cs2 && !state.preferences.cs2_today {
        cs2_row(out, state, assets, y, now);
        y += ROW_HEADING_GAP + CS2_CARD_HEIGHT + ROW_GAP;
    }
    if state.preferences.show_shows && !state.preferences.shows_today {
        shows_row(out, state, assets, y);
    }
}

/// Show cards share the favorites' width and pitch, with a poster's 2:3 face.
const SHOW_WIDTH: f32 = HOME_TILE_WIDTH;
const SHOW_POSTER_HEIGHT: f32 = HOME_TILE_WIDTH * 1.5;

/// Home's TV Shows: a heading crediting TVmaze, then posters in a row that slides six at a time.
fn shows_row(out: &mut Scene, state: &State, assets: &Assets, y: f32) {
    let language = state.preferences.language;
    let heading = language.t("shows");
    let style = row_heading(out, heading, y);
    out.text(
        "TVmaze",
        Rect::new(
            SAFE_X + assets.width(heading, style) + 20.,
            y + 5.,
            600.,
            27.,
        ),
        text(type_scale::CAPTION, 400, 0.).shadow(),
        c(0xf5f5f5, 0.5),
        false,
    );
    let list = &state.shows.list;
    if list.is_empty() {
        out.text(
            language.t(if state.shows.failed {
                "showsUnavailable"
            } else {
                "showsLoading"
            }),
            Rect::new(SAFE_X, y + ROW_HEADING_GAP, 1200., 29.),
            text(type_scale::SECONDARY, 400, 0.).shadow(),
            c(0xf5f5f5, 0.6),
            false,
        );
        return;
    }
    let visible = crate::shows::VISIBLE;
    let first = state.shows_first.min(list.len().saturating_sub(visible));
    if list.len() > visible {
        out.text(
            format!(
                "{}–{} / {}",
                first + 1,
                (first + visible).min(list.len()),
                list.len()
            ),
            Rect::new(SAFE_X + WIDTH - 400., y + 5., 400., 27.),
            text(type_scale::CAPTION, 500, 0.02).right().shadow(),
            c(0xf5f5f5, 0.6),
            false,
        );
    }
    carousel(
        out,
        state,
        Carousel {
            lane: crate::motion::Lane::Shows,
            first: state.shows_first,
            count: list.len(),
            visible,
            pitch: HOME_TILE_PITCH,
            card: Rect::new(SAFE_X, y + ROW_HEADING_GAP, SHOW_WIDTH, SHOW_POSTER_HEIGHT),
            below: 60.,
        },
        |out, index, rect, hit| show_card(out, state, assets, &list[index], index, rect, hit),
    );
}

/// A show's card: its poster, then the title and its details below.
fn show_card(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    show: &crate::shows::Show,
    index: usize,
    rect: Rect,
    hit: Option<Rect>,
) {
    let target = Target::Show(index);
    let (face, lift_amount) = lift(out, state, &target, rect, 18.4);
    match state.shows.poster(show) {
        Some(poster) => out.draw.push(Draw::Cover(poster, face, 18.4, [1.; 4])),
        None => {
            out.draw.push(Draw::RoundedGradient(
                face,
                18.4,
                c(0x262c33, 0.8),
                c(0x171c22, 0.8),
            ));
            out.icon(
                assets,
                "tv",
                Rect::new(
                    face.x + face.w / 2. - 22.,
                    face.y + face.h / 2. - 22.,
                    44.,
                    44.,
                ),
                c(0xf5f5f5, 0.35),
            );
        }
    }
    lift_edge(out, face, 18.4, lift_amount);
    // Below the poster: the title, then genre, year and rating, then the service.
    let below = rect.y + rect.h + 12.;
    out.text(
        &show.title,
        Rect::new(rect.x, below, rect.w, 26.),
        assets.fit_text(&show.title, text(19.2, 550, -0.01), rect.w),
        WHITE,
        false,
    );
    let meta = match show.service {
        Some(service) if !show.meta.is_empty() => format!("{} · {service}", show.meta),
        Some(service) => service.to_owned(),
        None => show.meta.clone(),
    };
    out.text(
        &meta,
        Rect::new(rect.x, below + 29., rect.w, 20.),
        assets.fit_text(&meta, text(14.4, 400, 0.), rect.w),
        c(0xf5f5f5, if show.app.is_some() { 0.6 } else { 0.4 }),
        false,
    );
    if let Some(hit) = hit {
        out.control(target, hit, true);
    }
}

fn shows_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let s = state.scroll;
    row(
        out,
        state,
        assets,
        Row {
            target: Target::ShowsToggle,
            rect: Rect::new(640., 164.8 - s, 1120., 72.),
            title: language.t("showWidget"),
            icon: Some("eye"),
            value: Some(language.t(if state.preferences.show_shows {
                "stateOn"
            } else {
                "stateOff"
            })),
            description: None,
            selected: false,
            chevron: false,
        },
    );
    row(
        out,
        state,
        assets,
        Row {
            target: Target::ShowsLayout,
            rect: Rect::new(640., 164.8 + 84. - s, 1120., 72.),
            title: language.t("cryptoLayout"),
            icon: Some("grid"),
            value: Some(place(language, state.preferences.shows_today)),
            description: None,
            selected: false,
            chevron: false,
        },
    );
    out.text(
        language.t("showsSource"),
        Rect::new(660., 164.8 + 2. * 84. + 6. - s, 1080., 24.),
        text(16., 400, 0.),
        c(0xf5f5f5, 0.55),
        false,
    );
}

/// Where a widget sits on Home: in the Today card, or in a row of its own.
fn place(language: Language, today: bool) -> &'static str {
    language.t(if today { "placeToday" } else { "placeRow" })
}

/// The Today card's lines for this moment, over the TV's local day.
fn today_lines(state: &State, now: OffsetDateTime) -> crate::today::Today {
    let midnight = now.replace_time(time::Time::MIDNIGHT).unix_timestamp();
    crate::today::today(state, now.unix_timestamp(), (midnight, midnight + 86_400))
}

const TODAY_PAD: f32 = 20.;
const TODAY_EYEBROW: f32 = 28.;
const TODAY_LINE: f32 = 58.;

/// Home's Today card beside the greeting: live matches, today's episodes and the prices, each
/// line with its picture, what it is, and when. None without lines; the header keeps its height.
fn today_card(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    today: &crate::today::Today,
    now: OffsetDateTime,
) {
    use crate::today::Line;
    if today.lines.is_empty() {
        return;
    }
    let language = state.preferences.language;
    let height = TODAY_PAD * 2. + TODAY_EYEBROW + TODAY_LINE * today.lines.len() as f32;
    let rect = Rect::new(
        TODAY_RECT.x,
        TODAY_RECT.y + ((TODAY_RECT.h - height) / 2.).max(0.) - state.page_scroll(),
        TODAY_RECT.w,
        height,
    );
    out.shadow(rect, 24., 4., 1., 0.01);
    out.draw.push(Draw::RoundedGradient(
        rect,
        24.,
        c(0x20252b, 0.60),
        c(0x171c22, 0.55),
    ));
    out.border(rect, 24., 1., c(0xffffff, 0.063));
    let x = rect.x + 24.;
    let inner = rect.w - 48.;
    let mut eyebrow = language.t("today").to_owned();
    if today.more > 0 {
        eyebrow = format!(
            "{eyebrow} · {}",
            language.format("todayMore", &[("count", &today.more.to_string())])
        );
    }
    out.text(
        eyebrow.to_uppercase(),
        Rect::new(x, rect.y + TODAY_PAD, inner, 24.),
        text(20., 600, 0.12),
        c(0xf5f5f5, 0.6),
        false,
    );
    let main = text(type_scale::BODY, 550, -0.01);
    let side = text(type_scale::CAPTION, 450, 0.);
    let unix = now.unix_timestamp();
    for (index, line) in today.lines.iter().enumerate() {
        let row = Rect::new(
            rect.x + 8.,
            rect.y + TODAY_PAD + TODAY_EYEBROW + index as f32 * TODAY_LINE,
            rect.w - 16.,
            TODAY_LINE,
        );
        let icon = Rect::new(x, row.y + 9., 40., 40.);
        let text_x = icon.x + icon.w + 16.;
        let target = match line {
            Line::Match { id } => Some(Target::TodayMatch(id.clone())),
            Line::Episode { show, .. } => Some(Target::TodayShow(*show)),
            Line::Crypto => None,
        };
        if let Some(target) = &target {
            let amount = state.motion.focus(target);
            if amount > 0. {
                out.shape(row, 14., c(0xffffff, 0.1 * amount));
                lift_edge(out, row, 14., amount);
            }
        }
        // (what, when, when's colour)
        let (title, when, tone) = match line {
            Line::Match { id } => {
                let Some(item) = state.cs2.find(id) else {
                    continue;
                };
                match item.icon.as_ref().and_then(|url| state.cs2.image(url)) {
                    Some(image) => out.draw.push(Draw::Picture(image, icon, 0., [1.; 4])),
                    None => out.icon(assets, "gaming", icon.inset(6.), c(0xf5f5f5, 0.7)),
                }
                let side = |index: usize| {
                    item.teams[index]
                        .as_ref()
                        .map_or("TBD", |team| team.short.as_str())
                        .to_owned()
                };
                let live = item.state == crate::cs2::State::Live;
                let title = match item.score.filter(|_| live) {
                    Some([a, b]) => format!("{} {a}–{b} {}", side(0), side(1)),
                    None => format!("{} – {}", side(0), side(1)),
                };
                if live {
                    let map = item.current_map().map(|(_, map)| map.name.clone());
                    let when = [Some(language.t("cs2Live").to_owned()), map]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" · ");
                    (title, when, c(0xff8a80, 0.95))
                } else {
                    (title, cs2_start(state, item.begins, now), c(0xe8e8e8, 0.62))
                }
            }
            Line::Episode { show, airs } => {
                let Some(show) = state.shows.list.iter().find(|s| s.id == *show) else {
                    continue;
                };
                match state.shows.poster(show) {
                    Some(poster) => out.draw.push(Draw::Cover(poster, icon, 8., [1.; 4])),
                    None => out.icon(assets, "tv", icon.inset(6.), c(0xf5f5f5, 0.7)),
                }
                let episode = show.episodes.iter().find(|e| e.airs == *airs);
                let title = match episode.and_then(|e| Some((e.season?, e.number?))) {
                    Some((season, number)) => format!("{} — S{season}E{number}", show.title),
                    None => show.title.clone(),
                };
                let time = if *airs >= unix {
                    OffsetDateTime::from_unix_timestamp(*airs)
                        .map(|t| {
                            language.clock(
                                t.to_offset(now.offset()),
                                state.preferences.hour_24,
                                false,
                            )
                        })
                        .unwrap_or_default()
                } else {
                    language.t("aired").to_owned()
                };
                let when = match show.service {
                    Some(service) => format!("{service} · {time}"),
                    None => time,
                };
                (title, when, c(0xe8e8e8, 0.62))
            }
            Line::Crypto => {
                out.icon(assets, "globe", icon.inset(6.), c(0xf5f5f5, 0.7));
                if state.crypto.quotes.is_empty() {
                    let status = language.t(if state.crypto.failed {
                        "cryptoUnavailable"
                    } else {
                        "cryptoLoading"
                    });
                    out.text(
                        status,
                        Rect::new(text_x, row.y + 17., inner - text_x + x, 28.),
                        side,
                        c(0xe8e8e8, 0.62),
                        false,
                    );
                    continue;
                }
                let coins = &state.preferences.crypto_coins;
                let slot = (rect.x + rect.w - 24. - text_x) / coins.len().max(1) as f32;
                for (index, symbol) in coins.iter().enumerate() {
                    let label = match state.crypto.quotes.iter().find(|q| &q.symbol == symbol) {
                        Some(quote) => format!(
                            "{symbol} {} {}{:.1}%",
                            crate::crypto::format_compact(quote.price),
                            if quote.change < 0. { "▼" } else { "▲" },
                            quote.change.abs()
                        ),
                        None => symbol.clone(),
                    };
                    let style = text(type_scale::CAPTION, 550, 0.);
                    out.text(
                        &label,
                        Rect::new(text_x + index as f32 * slot, row.y + 16., slot - 8., 28.),
                        assets.fit_text(&label, style, slot - 8.),
                        WHITE,
                        false,
                    );
                }
                continue;
            }
        };
        let when_width = assets.width(&when, side).min(inner * 0.45);
        let right = rect.x + rect.w - 24.;
        out.text(
            &when,
            Rect::new(right - when_width, row.y + 17., when_width, 28.),
            assets.fit_text(&when, side.right(), when_width),
            tone,
            false,
        );
        let room = right - when_width - 16. - text_x;
        out.text(
            &title,
            Rect::new(text_x, row.y + 14., room, 32.),
            assets.fit_text(&title, main, room),
            WHITE,
            false,
        );
        if let Some(target) = target {
            out.control(target, row, true);
        }
    }
}

/// Home's crypto prices: a heading and up to four cards in the inputs' style, each with the
/// coin, its price in US dollars and the 24-hour change.
fn crypto_row(out: &mut Scene, state: &State, y: f32) {
    let language = state.preferences.language;
    row_heading(out, language.t("crypto"), y);
    let gap = GUTTER;
    let width = column_width(crate::crypto::MAX_COINS);
    for (index, symbol) in state.preferences.crypto_coins.iter().enumerate() {
        let rect = Rect::new(
            SAFE_X + index as f32 * (width + gap),
            y + ROW_HEADING_GAP,
            width,
            CHIP_HEIGHT,
        );
        let focused = state.focus == Target::Coin(index) && !state.pointer;
        let hit = rect;
        let (rect, lift_amount) = lift(out, state, &Target::Coin(index), rect, 18.4);
        out.draw.push(Draw::RoundedGradient(
            rect,
            18.4,
            if focused {
                c(0x30363c, 0.70)
            } else {
                c(0x20252b, 0.60)
            },
            if focused {
                c(0x242a30, 0.65)
            } else {
                c(0x171c22, 0.55)
            },
        ));
        out.border(
            rect,
            18.4,
            1.,
            c(0xffffff, if focused { 0.24 } else { 0.063 }),
        );
        let name = match crate::crypto::name(symbol) {
            name if name == symbol => "",
            name => name,
        };
        let half = rect.w / 2. - 24.;
        out.text(
            symbol,
            Rect::new(rect.x + 24., rect.y + 16., half, 31.2),
            text(type_scale::BODY, 550, -0.02),
            WHITE,
            false,
        );
        out.text(
            name,
            Rect::new(rect.x + 24., rect.y + 51., half, 26.4),
            text(type_scale::CAPTION, 450, 0.),
            c(0xe8e8e8, 0.55),
            false,
        );
        let right = Rect::new(rect.x + rect.w / 2., 0., half, 0.);
        if let Some(quote) = state
            .crypto
            .quotes
            .iter()
            .find(|quote| &quote.symbol == symbol)
        {
            out.text(
                crate::crypto::format_price(quote.price),
                Rect::new(right.x, rect.y + 16., right.w, 31.2),
                text(type_scale::BODY, 550, -0.02).right(),
                WHITE,
                false,
            );
            out.text(
                crate::crypto::format_change(quote.change),
                Rect::new(right.x, rect.y + 51., right.w, 26.4),
                text(type_scale::CAPTION, 550, 0.).right(),
                if quote.change < -0.005 {
                    c(0xff8a80, 0.95)
                } else {
                    c(0x7ee2a8, 0.95)
                },
                false,
            );
        } else {
            out.text(
                language.t(if state.crypto.failed {
                    "cryptoUnavailable"
                } else {
                    "cryptoLoading"
                }),
                Rect::new(right.x, rect.y + 35., right.w, 26.4),
                text(type_scale::CAPTION, 400, 0.).right(),
                c(0xe8e8e8, 0.55),
                false,
            );
        }
        lift_edge(out, rect, 18.4, lift_amount);
        out.control(Target::Coin(index), hit, true);
    }
}

const CS2_CARD_HEIGHT: f32 = 150.;
const CS2_CARD_PITCH: f32 = 600.;

/// Home's CS2 Score: a heading crediting Liquipedia, then up to three match cards in the inputs'
/// style, live ones first.
fn cs2_row(out: &mut Scene, state: &State, assets: &Assets, y: f32, now: OffsetDateTime) {
    let language = state.preferences.language;
    let heading = language.t("cs2Score");
    let style = row_heading(out, heading, y);
    let credit = language.t("cs2Credit");
    let credit_x = SAFE_X + assets.width(heading, style) + 20.;
    let credit_style = text(type_scale::CAPTION, 400, 0.).shadow();
    out.text(
        credit,
        Rect::new(credit_x, y + 5., 600., 27.),
        credit_style,
        c(0xf5f5f5, 0.5),
        false,
    );
    let preferences = &state.preferences;
    let unix = now.unix_timestamp();
    // Scores from an old feed say how old they are.
    if state.cs2.stale(unix)
        && let Some(updated) = cs2_updated(state, now)
    {
        out.text(
            format!("·  {updated}"),
            Rect::new(
                credit_x + assets.width(credit, credit_style) + 10.,
                y + 5.,
                600.,
                27.,
            ),
            text(type_scale::CAPTION, 550, 0.).shadow(),
            c(0xf2c27a, 0.9),
            false,
        );
    }
    let Some(feed) = state.cs2.current(unix) else {
        out.text(
            language.t(
                if state.cs2.loading || (state.cs2.feed.is_none() && !state.cs2.failed) {
                    "cs2Loading"
                } else {
                    "cs2Unavailable"
                },
            ),
            Rect::new(SAFE_X, y + ROW_HEADING_GAP, 1200., 29.),
            text(type_scale::SECONDARY, 400, 0.).shadow(),
            c(0xf5f5f5, 0.6),
            false,
        );
        return;
    };
    let matches = crate::cs2::shown(feed, preferences.cs2_major_only, preferences.cs2_results);
    if matches.is_empty() {
        out.text(
            language.t(if preferences.cs2_major_only {
                "cs2NoMajor"
            } else {
                "cs2None"
            }),
            Rect::new(SAFE_X, y + ROW_HEADING_GAP, 1200., 29.),
            text(type_scale::SECONDARY, 400, 0.).shadow(),
            c(0xf5f5f5, 0.6),
            false,
        );
        return;
    }
    // Three cards at a time from `cs2_first`; Left and Right scroll the rest into view.
    let visible = crate::cs2::VISIBLE;
    let first = state.cs2_first.min(matches.len().saturating_sub(visible));
    if matches.len() > visible {
        let last = (first + visible).min(matches.len());
        out.text(
            format!("{}–{} / {}", first + 1, last, matches.len()),
            Rect::new(SAFE_X + WIDTH - 400., y + 5., 400., 27.),
            text(type_scale::CAPTION, 500, 0.02).right().shadow(),
            c(0xf5f5f5, 0.6),
            false,
        );
    }
    carousel(
        out,
        state,
        Carousel {
            lane: crate::motion::Lane::Cs2,
            first: state.cs2_first,
            count: matches.len(),
            visible,
            pitch: CS2_CARD_PITCH,
            card: Rect::new(
                SAFE_X,
                y + ROW_HEADING_GAP,
                column_width(3),
                CS2_CARD_HEIGHT,
            ),
            below: 0.,
        },
        |out, index, rect, hit| cs2_card(out, state, assets, matches[index], rect, hit, index, now),
    );
}

/// A row of cards on Home that slides as one (`motion.rs`). `card` is where the first card in
/// view sits; `below` is extra room under each card, for labels.
struct Carousel {
    lane: crate::motion::Lane,
    first: usize,
    count: usize,
    visible: usize,
    pitch: f32,
    card: Rect,
    below: f32,
}

/// The carousel every row of cards on Home uses, so all of them move alike: the cards slide on
/// the row's lane toward `first`, those off screen are skipped, and only the `visible` ones take
/// focus and clicks, at their settled places. `draw` draws card `index` at `rect` and registers
/// `hit` when given.
fn carousel(
    out: &mut Scene,
    state: &State,
    row: Carousel,
    mut draw: impl FnMut(&mut Scene, usize, Rect, Option<Rect>),
) {
    let last_first = row.count.saturating_sub(row.visible);
    let first = row.first.min(last_first);
    let shown = state
        .motion
        .shown(row.lane, row.first as f32)
        .clamp(0., last_first as f32);
    let content = state.content_viewport();
    let top = (row.card.y - 38.).max(content.y);
    out.draw.push(Draw::Clip(Some(Rect::new(
        content.x,
        top,
        content.w,
        (row.card.y + row.card.h + row.below + 40. - top).max(0.),
    ))));
    for index in 0..row.count {
        let x = |at: f32| row.card.x + (index as f32 - at) * row.pitch;
        let rect = Rect::new(x(shown), row.card.y, row.card.w, row.card.h);
        if rect.x + rect.w < content.x - 40. || rect.x > content.x + content.w + 40. {
            continue;
        }
        let hit = (first..first + row.visible)
            .contains(&index)
            .then(|| Rect::new(x(first as f32), rect.y, rect.w, rect.h + row.below));
        draw(out, index, rect, hit);
    }
    out.draw.push(Draw::Clip(Some(content)));
}

#[allow(clippy::too_many_arguments)]
fn cs2_card(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    item: &crate::cs2::Match,
    rect: Rect,
    hit: Option<Rect>,
    index: usize,
    now: OffsetDateTime,
) {
    use crate::cs2::State as Phase;
    let language = state.preferences.language;
    let focused = state.focus == Target::Cs2Match(index) && !state.pointer;
    let live = item.state == Phase::Live;
    let (rect, lift_amount) = lift(out, state, &Target::Cs2Match(index), rect, 18.4);
    // A deep card, so scores stand out against any wallpaper, with the tournament's logo as a
    // faint watermark at its right.
    out.draw.push(Draw::RoundedGradient(
        rect,
        18.4,
        if focused {
            c(0x262c33, 0.95)
        } else {
            c(0x171b20, 0.90)
        },
        if focused {
            c(0x1a1f25, 0.96)
        } else {
            c(0x0e1115, 0.92)
        },
    ));
    if let Some(icon) = item.icon.as_ref().and_then(|url| state.cs2.image(url)) {
        out.draw.push(Draw::Picture(
            icon,
            Rect::new(rect.x + rect.w - 170., rect.y + 12., 150., rect.h - 24.),
            0.,
            [1., 1., 1., if focused { 0.16 } else { 0.11 }],
        ));
    }
    out.border(
        rect,
        18.4,
        1.,
        if live {
            c(0xff5a52, 0.38)
        } else {
            c(0xffffff, if focused { 0.24 } else { 0.08 })
        },
    );
    // Top line: LIVE, the start or "Final", then the tournament.
    let x = rect.x + 22.;
    let mut caption_x = x;
    let top = rect.y + 18.;
    if live {
        let label = language.t("cs2Live");
        let style = text(13.6, 650, 0.06);
        let width = assets.width(label, style) + 18.;
        let badge = Rect::new(x, top, width, 24.);
        out.shape(badge, 7., c(0xe5484d, 0.95));
        out.text(
            label,
            Rect::new(x, top + 4.4, width, 17.),
            style,
            WHITE,
            true,
        );
        caption_x += width + 12.;
    } else {
        let label = if item.state == Phase::Finished {
            language.t("cs2Final").to_owned()
        } else {
            cs2_start(state, item.begins, now)
        };
        let style = text(16., 600, 0.);
        out.text(
            &label,
            Rect::new(x, top + 2., 260., 21.),
            style,
            WHITE,
            false,
        );
        caption_x += assets.width(&label, style) + 12.;
    }
    let caption = text(15.2, 400, 0.);
    let room = rect.x + rect.w - 22. - caption_x;
    out.text(
        &item.tournament,
        Rect::new(caption_x, top + 3., room, 20.),
        assets.fit_text(&item.tournament, caption, room),
        c(0xf5f5f5, 0.55),
        false,
    );
    // The two sides with the maps they won.
    let scored = item.state != Phase::Upcoming && item.score.is_some();
    for side in 0..2 {
        let line = rect.y + 50. + side as f32 * 32.;
        let name = item.teams[side]
            .as_ref()
            .map_or("TBD", |team| team.name.as_str());
        let lead = item
            .score
            .is_none_or(|score| score[side] >= score[1 - side]);
        cs2_logo(
            out,
            state,
            item.teams[side].as_ref(),
            Rect::new(x, line, 28., 28.),
        );
        out.text(
            name,
            Rect::new(x + 40., line, rect.w - 160., 28.),
            assets.fit_text(name, text(20.8, 550, -0.02), rect.w - 160.),
            if item.teams[side].is_some() {
                WHITE
            } else {
                c(0xf5f5f5, 0.55)
            },
            false,
        );
        if scored && let Some(score) = item.score {
            out.text(
                score[side].to_string(),
                Rect::new(rect.x + rect.w - 82., line - 2., 60., 30.),
                text(24., 650, -0.02).right(),
                c(0xffffff, if lead { 1. } else { 0.42 }),
                false,
            );
        }
    }
    // Bottom line: the map being played, else the format and stage.
    let bottom = match item.current_map().filter(|_| live) {
        Some((number, map)) => {
            let mut parts = vec![
                language.format("cs2Map", &[("number", &number.to_string())]),
                map.name.clone(),
            ];
            if let Some([a, b]) = map.score {
                parts.push(format!("{a}:{b}"));
            }
            parts.join(" · ")
        }
        None => [
            item.best_of.map(|n| format!("BO{n}")).unwrap_or_default(),
            cs2_round(language, item).unwrap_or_else(|| item.stage.clone()),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · "),
    };
    out.text(
        bottom,
        Rect::new(x, rect.y + rect.h - 34., rect.w - 44., 20.),
        text(15.2, 500, 0.),
        if live {
            c(0xffb4ae, 0.9)
        } else {
            c(0xf5f5f5, 0.55)
        },
        false,
    );
    lift_edge(out, rect, 18.4, lift_amount);
    if let Some(hit) = hit {
        out.control(Target::Cs2Match(index), hit, true);
    }
}

/// A match's round in the TV's language: "Upper bracket · Semifinals", "Grand final"…
fn cs2_round(language: Language, item: &crate::cs2::Match) -> Option<String> {
    use crate::cs2::{Bracket, Round};
    let (round, bracket) = item.round.as_ref()?;
    let name = match round {
        Round::GrandFinal => language.t("cs2RoundGrandFinal").to_owned(),
        Round::Final => language.t("cs2RoundFinal").to_owned(),
        Round::Semifinals => language.t("cs2RoundSemifinals").to_owned(),
        Round::Quarterfinals => language.t("cs2RoundQuarterfinals").to_owned(),
        Round::Top(number) => language.format("cs2RoundOf", &[("number", &number.to_string())]),
        Round::ThirdPlace => language.t("cs2RoundThirdPlace").to_owned(),
        Round::Named(name) => name.clone(),
    };
    Some(match bracket {
        Some(Bracket::Upper) => format!("{} · {name}", language.t("cs2UpperBracket")),
        Some(Bracket::Lower) => format!("{} · {name}", language.t("cs2LowerBracket")),
        None => name,
    })
}

/// A team's logo, or its initials on a tile while there is none.
fn cs2_logo(out: &mut Scene, state: &State, team: Option<&crate::cs2::Team>, rect: Rect) {
    let radius = rect.w * 0.24;
    if let Some(image) = team.and_then(|team| state.cs2.logo(team)) {
        out.draw.push(Draw::AppImage(image, rect, radius, [1.; 4]));
        return;
    }
    out.shape(rect, radius, c(0xffffff, 0.09));
    let initials: String = team
        .map_or("?", |team| team.short.as_str())
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(2)
        .collect::<String>()
        .to_uppercase();
    let size = rect.h * 0.38;
    out.text(
        initials,
        Rect::new(
            rect.x,
            rect.y + (rect.h - size * 1.2) / 2.,
            rect.w,
            size * 1.2,
        ),
        text(size, 650, 0.02),
        c(0xf5f5f5, 0.8),
        true,
    );
}

/// A CS2 Score match in detail: the series, its maps and both lineups, live while it is open.
fn cs2_dialog(out: &mut Scene, state: &State, assets: &Assets, id: &str, now: OffsetDateTime) {
    use crate::cs2::State as Phase;
    let language = state.preferences.language;
    let panel = Rect::new(296., 60., 1328., 960.);
    out.modal = Some(panel);
    out.shadow(panel, 28., 100., 24., 0.47);
    out.draw.push(Draw::RoundedGradient(
        panel,
        28.,
        c(0x34383b, 0.98),
        c(0x24282d, 0.99),
    ));
    out.border(panel, 28., 1., c(0xffffff, 0.12));
    let close = Rect::new(panel.x + panel.w - 92., panel.y + 36., 56., 56.);
    let close_focused = state.focus == Target::Cs2Close && !state.pointer;
    out.shape(
        close,
        28.,
        c(0xffffff, if close_focused { 0.96 } else { 0.08 }),
    );
    out.icon(
        assets,
        "close",
        close.inset(17.),
        if close_focused {
            c(0x2c2c2c, 1.)
        } else {
            c(0xeeeeee, 0.85)
        },
    );
    out.control(Target::Cs2Close, close, true);
    let Some(item) = state.cs2.find(id) else {
        out.text(
            language.t("cs2Unavailable"),
            Rect::new(panel.x + 48., panel.y + 48., 900., 30.),
            text(22.4, 550, 0.),
            c(0xf5f5f5, 0.7),
            false,
        );
        return;
    };
    let x = panel.x + 48.;
    let inner = panel.w - 96.;
    let live = item.state == Phase::Live;

    // Header: the tournament with its logo, the stage and chips.
    let icon = item.icon.as_ref().and_then(|url| state.cs2.image(url));
    let title_x = if let Some(icon) = icon {
        out.draw.push(Draw::Picture(
            icon,
            Rect::new(x, panel.y + 40., 60., 60.),
            0.,
            [1.; 4],
        ));
        x + 78.
    } else {
        x
    };
    out.text(
        &item.tournament,
        Rect::new(title_x, panel.y + 40., inner - 120. - (title_x - x), 36.),
        assets.fit_text(
            &item.tournament,
            text(28., 650, -0.02),
            inner - 120. - (title_x - x),
        ),
        WHITE,
        false,
    );
    let round = cs2_round(language, item);
    let subtitle = ["Counter-Strike 2", item.stage.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    out.text(
        subtitle,
        Rect::new(title_x, panel.y + 82., inner - 120. - (title_x - x), 24.),
        text(17.6, 400, 0.),
        c(0xf5f5f5, 0.6),
        false,
    );
    let mut chip_x = x;
    // The tier chip is gold for the biggest events.
    // Deciding matches stand out: the final, the grand final and the match for third place.
    let deciding = matches!(
        item.round,
        Some((
            crate::cs2::Round::Final
                | crate::cs2::Round::GrandFinal
                | crate::cs2::Round::ThirdPlace,
            _
        ))
    );
    for (chip, gold) in [
        round.clone().map(|round| (round, deciding)),
        item.best_of.map(|n| (format!("BO{n}"), false)),
        item.tier.map(|tier| {
            (
                language.format("cs2Tier", &[("number", &tier.to_string())]),
                item.major(),
            )
        }),
    ]
    .into_iter()
    .flatten()
    {
        let style = text(15.2, 600, 0.02);
        let width = assets.width(&chip, style) + 28.;
        let rect = Rect::new(chip_x, panel.y + 118., width, 30.);
        out.shape(
            rect,
            15.,
            if gold {
                c(0xe8c26a, 0.22)
            } else {
                c(0xffffff, 0.08)
            },
        );
        out.text(
            chip,
            Rect::new(rect.x, rect.y + 6., width, 20.),
            style,
            WHITE,
            true,
        );
        chip_x += width + 10.;
    }

    // Hero: both teams around the score.
    let hero = Rect::new(x, panel.y + 168., inner, 226.);
    // The map being played (or the last one) behind the teams, darkened for the text.
    match item
        .featured_map()
        .and_then(|map| map.image.as_ref())
        .and_then(|url| state.cs2.image(url))
    {
        Some(image) => {
            out.draw.push(Draw::Cover(image, hero, 22., [1.; 4]));
            out.draw.push(Draw::RoundedGradient(
                hero,
                22.,
                c(0x0b0e12, 0.62),
                c(0x0b0e12, 0.84),
            ));
        }
        None => out.shape(hero, 22., c(0xffffff, 0.04)),
    }
    if live {
        out.shape(hero, 22., c(0xe5484d, 0.12));
    }
    out.border(
        hero,
        22.,
        1.,
        if live {
            c(0xff5a52, 0.32)
        } else {
            c(0xffffff, 0.06)
        },
    );
    for side in 0..2 {
        let center = if side == 0 {
            hero.x + 230.
        } else {
            hero.x + hero.w - 230.
        };
        let team = item.teams[side].as_ref();
        cs2_logo(
            out,
            state,
            team,
            Rect::new(center - 56., hero.y + 26., 112., 112.),
        );
        let name = team.map_or("TBD", |team| team.name.as_str());
        out.text(
            name,
            Rect::new(center - 200., hero.y + 152., 400., 32.),
            assets.fit_text(name, text(24., 650, -0.02), 400.),
            WHITE,
            true,
        );
    }
    let middle = hero.x + hero.w / 2.;
    match item.score.filter(|_| item.state != Phase::Upcoming) {
        Some(score) => {
            let style = text(72., 650, -0.02);
            for side in 0..2 {
                let lead = score[side] >= score[1 - side];
                let offset = if side == 0 { -150. } else { 30. };
                out.text(
                    score[side].to_string(),
                    Rect::new(middle + offset, hero.y + 36., 120., 86.),
                    if side == 0 { style.right() } else { style },
                    c(0xffffff, if lead { 1. } else { 0.42 }),
                    false,
                );
            }
            out.text(
                ":",
                Rect::new(middle - 30., hero.y + 32., 60., 86.),
                text(64., 650, 0.),
                c(0xffffff, 0.5),
                true,
            );
        }
        None => out.text(
            "VS",
            Rect::new(middle - 100., hero.y + 58., 200., 60.),
            text(44., 650, 0.04),
            c(0xffffff, 0.45),
            true,
        ),
    }
    let line = if live {
        let label = language.t("cs2Live");
        let style = text(13.6, 650, 0.06);
        let width = assets.width(label, style) + 18.;
        let badge = Rect::new(middle - width / 2., hero.y + 132., width, 24.);
        out.shape(badge, 7., c(0xe5484d, 0.95));
        out.text(
            label,
            Rect::new(badge.x, badge.y + 4.4, width, 17.),
            style,
            WHITE,
            true,
        );
        item.current_map().map(|(number, map)| {
            let mut parts = vec![
                language.format("cs2Map", &[("number", &number.to_string())]),
                map.name.clone(),
            ];
            if let Some([a, b]) = map.score {
                parts.push(format!("{a}:{b}"));
            }
            parts.join(" · ")
        })
    } else if item.state == Phase::Finished {
        Some(language.t("cs2Final").to_owned())
    } else {
        Some(cs2_start(state, item.begins, now))
    };
    if let Some(line) = line {
        out.text(
            line,
            Rect::new(
                middle - 240.,
                hero.y + if live { 164. } else { 140. },
                480.,
                28.,
            ),
            text(if live { 21. } else { 22.4 }, 650, 0.),
            if live { c(0xffe1de, 1.) } else { WHITE },
            true,
        );
    }
    // Liquipedia's editors add a live map's rounds as they go; until then, say so.
    if live
        && item
            .current_map()
            .is_some_and(|(_, map)| map.score.is_none())
    {
        out.text(
            language.t("cs2NoRounds"),
            Rect::new(middle - 240., hero.y + 196., 480., 20.),
            text(14.4, 500, 0.02),
            c(0xffffff, 0.6),
            true,
        );
    }

    // Maps: one tile per map of the series.
    let maps_y = hero.y + hero.h + 30.;
    out.text(
        language.t("cs2Maps").to_uppercase(),
        Rect::new(x, maps_y, 400., 20.),
        text(14.4, 650, 0.12),
        c(0xf5f5f5, 0.55),
        false,
    );
    let slots = item
        .best_of
        .unwrap_or(1)
        .max(item.maps.len() as u32)
        .clamp(1, 7) as usize;
    let gap = 14.;
    let tile_w = (inner - gap * (slots - 1) as f32) / slots as f32;
    let playing = item
        .current_map()
        .filter(|_| live)
        .map(|(number, _)| number);
    for slot in 0..slots {
        let tile = Rect::new(x + slot as f32 * (tile_w + gap), maps_y + 30., tile_w, 112.);
        let map = item.maps.get(slot);
        let is_live = playing == Some(slot + 1);
        let picture = map
            .and_then(|map| map.image.as_ref())
            .and_then(|url| state.cs2.image(url));
        match picture {
            Some(picture) => {
                out.draw.push(Draw::Cover(picture, tile, 18., [1.; 4]));
                out.draw.push(Draw::RoundedGradient(
                    tile,
                    18.,
                    c(0x0b0e12, 0.55),
                    c(0x0b0e12, 0.82),
                ));
            }
            None => out.shape(
                tile,
                18.,
                c(
                    0xffffff,
                    if map.is_some_and(|map| map.winner.is_some()) {
                        0.07
                    } else {
                        0.03
                    },
                ),
            ),
        }
        if is_live {
            out.shape(tile, 18., c(0xe5484d, 0.18));
        }
        out.border(
            tile,
            18.,
            1.,
            if is_live {
                c(0xff5a52, 0.5)
            } else {
                c(0xffffff, 0.07)
            },
        );
        out.text(
            format!("{:02}", slot + 1),
            Rect::new(tile.x + tile.w - 70., tile.y + 12., 56., 40.),
            text(34., 650, 0.).right(),
            c(0xffffff, 0.07),
            false,
        );
        let title = map.map_or_else(
            || language.format("cs2Map", &[("number", &(slot + 1).to_string())]),
            |map| map.name.clone(),
        );
        out.text(
            &title,
            Rect::new(tile.x + 18., tile.y + 16., tile.w - 90., 28.),
            assets.fit_text(&title, text(21., 650, -0.01), tile.w - 90.),
            c(0xffffff, if map.is_some() { 1. } else { 0.45 }),
            false,
        );
        let bottom = tile.y + tile.h - 40.;
        if let Some([a, b]) = map.and_then(|map| map.score) {
            out.text(
                format!("{a}:{b}"),
                Rect::new(tile.x + tile.w - 118., bottom, 100., 26.),
                text(21., 650, 0.).right(),
                WHITE,
                false,
            );
        }
        if is_live {
            out.text(
                language.t("cs2Live"),
                Rect::new(tile.x + 18., bottom + 3., 120., 20.),
                text(15.2, 650, 0.06),
                c(0xff7b72, 1.),
                false,
            );
        } else if let Some(winner) = map
            .and_then(|map| map.winner)
            .and_then(|side| item.teams[side].as_ref())
        {
            cs2_logo(
                out,
                state,
                Some(winner),
                Rect::new(tile.x + 18., bottom, 26., 26.),
            );
            out.text(
                &winner.short,
                Rect::new(tile.x + 52., bottom + 2., tile.w - 180., 24.),
                text(17.6, 600, 0.),
                WHITE,
                false,
            );
        }
    }

    // Lineups.
    let lists_y = maps_y + 30. + 112. + 26.;
    let column_w = (inner - 24.) / 2.;
    for side in 0..2 {
        let column = Rect::new(x + side as f32 * (column_w + 24.), lists_y, column_w, 290.);
        out.shape(column, 20., c(0xffffff, 0.04));
        let team = item.teams[side].as_ref();
        cs2_logo(
            out,
            state,
            team,
            Rect::new(column.x + 18., column.y + 16., 26., 26.),
        );
        out.text(
            language.format(
                "cs2Lineup",
                &[("team", team.map_or("TBD", |team| team.short.as_str()))],
            ),
            Rect::new(column.x + 54., column.y + 18., column.w - 72., 24.),
            text(17.6, 600, 0.),
            WHITE,
            false,
        );
        let players = &item.lineups[side];
        if players.is_empty() {
            out.text(
                language.t("cs2NoLineup"),
                Rect::new(column.x + 18., column.y + 64., column.w - 36., 24.),
                text(17.6, 400, 0.),
                c(0xf5f5f5, 0.5),
                false,
            );
        }
        let coaches = team.map_or(&[][..], |team| {
            state
                .cs2
                .feed
                .as_ref()
                .map_or(&[][..], |feed| feed.coaches(team))
        });
        if !coaches.is_empty() {
            let line = column.y + column.h - 40.;
            out.shape(
                Rect::new(column.x + 18., line - 10., column.w - 36., 1.),
                0.,
                c(0xffffff, 0.1),
            );
            let label = language.t("cs2Coach").to_uppercase();
            let style = text(13.6, 650, 0.1);
            out.text(
                &label,
                Rect::new(column.x + 18., line + 3., 160., 20.),
                style,
                c(0xf5f5f5, 0.5),
                false,
            );
            let names = coaches
                .iter()
                .map(|coach| coach.name.as_str())
                .collect::<Vec<_>>()
                .join(" · ");
            out.text(
                names,
                Rect::new(
                    column.x + 30. + assets.width(&label, style),
                    line,
                    column.w - 60.,
                    24.,
                ),
                text(17.6, 600, 0.),
                WHITE,
                false,
            );
        }
        for (row, player) in players.iter().take(6).enumerate() {
            let y = column.y + 58. + row as f32 * 32.;
            out.text(
                &player.name,
                Rect::new(column.x + 18., y, column.w * 0.6, 26.),
                text(19.2, 600, -0.01),
                WHITE,
                false,
            );
            out.text(
                player.flag.to_uppercase(),
                Rect::new(
                    column.x + column.w * 0.55,
                    y + 3.,
                    column.w * 0.45 - 18.,
                    22.,
                ),
                text(13.6, 600, 0.08).right(),
                c(0xf5f5f5, 0.5),
                false,
            );
        }
    }

    // Footer: where the data comes from.
    out.text(
        format!("{} · CC BY-SA 3.0", language.t("cs2Credit")),
        Rect::new(x, panel.y + panel.h - 44., inner, 20.),
        text(14.4, 500, 0.).right(),
        c(0xf5f5f5, 0.5),
        false,
    );
    // When the feed was last rebuilt, so a stalled feed shows.
    if let Some(updated) = cs2_updated(state, now) {
        out.text(
            updated,
            Rect::new(x, panel.y + panel.h - 44., inner - 340., 20.),
            text(14.4, 500, 0.),
            c(0xf5f5f5, 0.6),
            false,
        );
    }
}

/// "Last updated 21:05" for the feed, in the TV's time; the date too when not today.
fn cs2_updated(state: &State, now: OffsetDateTime) -> Option<String> {
    let language = state.preferences.language;
    let feed = state.cs2.feed.as_ref()?;
    let time = OffsetDateTime::from_unix_timestamp(feed.updated)
        .ok()?
        .to_offset(now.offset());
    let mut clock = language.clock(time, state.preferences.hour_24, false);
    if time.date() != now.date() {
        clock = format!("{:02}.{:02} {clock}", time.day(), time.month() as u8);
    }
    Some(language.format("cs2Updated", &[("time", &clock)]))
}

/// A match's start in the TV's time: "21:00" today, "Tomorrow 21:00", else its date.
fn cs2_start(state: &State, begins: i64, now: OffsetDateTime) -> String {
    let Ok(start) = OffsetDateTime::from_unix_timestamp(begins).map(|t| t.to_offset(now.offset()))
    else {
        return String::new();
    };
    let preferences = &state.preferences;
    let clock = preferences
        .language
        .clock(start, preferences.hour_24, false);
    let days = (start.date() - now.date()).whole_days();
    match days {
        0 => clock,
        1 => preferences
            .language
            .format("cs2Tomorrow", &[("time", &clock)]),
        _ => {
            let (day, month) = (start.day(), start.month() as u8);
            let month_first = preferences.date_format == crate::model::DateFormat::MonthDayYear
                || (preferences.date_format == crate::model::DateFormat::Localized
                    && preferences.language == Language::En);
            if month_first {
                format!("{month:02}/{day:02} {clock}")
            } else {
                format!("{day:02}.{month:02} {clock}")
            }
        }
    }
}

const WEATHER_TEXT: f32 = 28.;
const WEATHER_ICON: f32 = 30.;
/// The weather's text starts past its icon.
const WEATHER_INDENT: f32 = WEATHER_ICON + 12.;
struct WeatherSummary {
    height: f32,
    width: f32,
    icon: String,
    parts: Vec<(String, TextStyle, Color, Rect)>,
}
type WeatherPart = (String, TextStyle, Color, f32, f32);
/// The header's weather line, wrapping within `column` (the left column's width).
fn weather_summary_layout(state: &State, assets: &Assets, column: f32) -> WeatherSummary {
    let language = state.preferences.language;
    let base = text(WEATHER_TEXT, 400, -0.02).leading(1.5);
    let mut parts = Vec::new();
    let mut icon = "location".to_owned();
    if let Some(place) = &state.preferences.weather_location {
        parts.push((place.name.clone(), base.weight(450), c(0xf5f5f5, 0.72)));
        parts.push(("·".into(), base, c(0xf5f5f5, 0.324)));
        if let Some(reading) = state.weather.reading(place) {
            let condition = weather::condition(reading);
            icon = format!("weather-{}", condition.icon);
            parts.push((
                weather::format_temperature(
                    reading.temperature,
                    state.preferences.temperature_unit,
                ),
                base.weight(550),
                c(0xf5f5f5, 0.91),
            ));
            parts.push((language.t(condition.key).into(), base, c(0xf5f5f5, 0.58)));
            if state.weather.failed {
                parts.push((
                    language.t("lastReading").into(),
                    text(type_scale::CAPTION, 400, 0.).leading(1.5),
                    c(0xf2d5a5, 0.65),
                ));
            }
        } else {
            parts.push((
                if state.weather.loading {
                    language.t("loadingWeather")
                } else {
                    language.t("weatherUnavailable")
                }
                .into(),
                base,
                c(0xf5f5f5, 0.58),
            ));
        }
    } else {
        parts.push((
            language.t("setWeatherLocation").into(),
            base,
            c(0xf5f5f5, 0.72),
        ));
    }
    let mut rows: Vec<Vec<WeatherPart>> = vec![Vec::new()];
    let mut used = 0.;
    for (label, style, color) in parts {
        // A part wider than the column (a very long place name) shrinks onto one line rather than
        // wrapping the header down into the favorites.
        let style = assets.fit_text(&label, style, column - WEATHER_INDENT);
        let measured_width = assets.width(&label, style);
        let width = measured_width.min(column - WEATHER_INDENT);
        // Use the same extra pixel as the rendered text rectangle. Counting
        // lines at the exact advance width can round into a phantom second
        // line even though the renderer displays the label on one line.
        let text_width = width + 1.;
        let lines = if measured_width <= text_width && !label.contains('\n') {
            1
        } else {
            assets.lines(&label, style, text_width)
        };
        let height = lines as f32 * style.size * style.line_height;
        if used > 0. && used + 11.2 + width > column - WEATHER_INDENT {
            rows.push(Vec::new());
            used = 0.;
        }
        rows.last_mut()
            .unwrap()
            .push((label, style, color, width, height));
        used += width + if used > 0. { 11.2 } else { 0. };
    }
    let mut positioned = Vec::new();
    let mut top = 1.6;
    let mut right: f32 = 0.;
    for row in rows {
        let height = row
            .iter()
            .map(|part| part.4)
            .fold(WEATHER_TEXT * 1.5, f32::max);
        let mut x = SAFE_X + WEATHER_INDENT;
        for (label, style, color, width, item_height) in row {
            positioned.push((
                label,
                style,
                color,
                Rect::new(
                    x,
                    top + (height - item_height) / 2.,
                    width + 1.,
                    item_height,
                ),
            ));
            x += width + 11.2;
        }
        right = right.max(x - 11.2);
        top += height + 11.2;
    }
    WeatherSummary {
        height: top - 11.2 + 1.6,
        width: right - SAFE_X,
        icon,
        parts: positioned,
    }
}
fn weather_summary(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    summary: WeatherSummary,
    y: f32,
) {
    let label_focus = summary.parts.first().map(|(_, style, _, rect)| {
        // The label's layout line box includes leading above and below the
        // text. Keep the focus ring snug to the label rather than the whole
        // weather row, which also contains the location icon.
        let leading = style.size * (style.line_height - 1.).max(0.);
        Rect::new(
            rect.x,
            y + rect.y + leading / 2.,
            rect.w,
            (rect.h - leading).max(style.size),
        )
    });
    out.shadowed_icon(
        assets,
        &summary.icon,
        Rect::new(
            SAFE_X,
            y + (WEATHER_TEXT * 1.5 - WEATHER_ICON) / 2. + 1.6,
            WEATHER_ICON,
            WEATHER_ICON,
        ),
        c(0xdedede, 0.83),
    );
    for (label, style, color, rect) in summary.parts {
        out.text(
            label,
            Rect::new(rect.x, rect.y + y, rect.w, rect.h),
            style.shadow(),
            color,
            false,
        );
    }
    if state.preferences.weather_location.is_none() {
        let rect = Rect::new(SAFE_X, y, summary.width, summary.height);
        if state.focus == Target::WeatherConfigure
            && !state.pointer
            && let Some(label) = label_focus
        {
            out.border(label.inset(-5.), 8.6, 2., c(0xf0f0f0, 0.95));
        }
        out.control(Target::WeatherConfigure, rect, true);
    }
}

fn library(out: &mut Scene, state: &State, assets: &Assets) {
    library_grid(out, state, assets);
    library_toolbar(out, state, assets);
}

/// The toolbar stays centred on the line it sits on at normal scale, clear of the grid.
/// The Apps page: one row of filters and search from y 150, then six 260 px columns of cards,
/// rows 206 apart, so four rows fit the screen.
const APPS_TOOLBAR_TOP: f32 = 150.;
const TOOLBAR_HEIGHT: f32 = 60.;
const TOOLBAR_CENTER: f32 = APPS_TOOLBAR_TOP + TOOLBAR_HEIGHT / 2.;
const APPS_GRID_TOP: f32 = APPS_TOOLBAR_TOP + TOOLBAR_HEIGHT + GUTTER;
const APPS_FACE_HEIGHT: f32 = 150.;
const APPS_ROW_PITCH: f32 = 206.;

fn library_toolbar(out: &mut Scene, state: &State, assets: &Assets) {
    let k = state.preferences.ui_scale.scale();
    let x = category_chips(out, state, assets, SAFE_X, TOOLBAR_CENTER);
    if state.filter != 0 {
        let h = TOOLBAR_HEIGHT * k;
        let rect = Rect::new(x, TOOLBAR_CENTER - h / 2., h, h);
        let focused = state.focus == Target::ClearFilter;
        out.pill(rect, focused, false);
        out.icon(
            assets,
            "close",
            rect.inset(16. * k),
            if focused {
                c(0x2c2c2c, 1.)
            } else {
                c(0xeeeeee, 0.73)
            },
        );
        out.control(Target::ClearFilter, rect, false);
    }
    search_field(out, state, assets, SAFE_X + WIDTH, TOOLBAR_CENTER, true);
}

/// The category filter chips from `x`, centred on `y`; returns where the row ends.
fn category_chips(out: &mut Scene, state: &State, assets: &Assets, mut x: f32, y: f32) -> f32 {
    let language = state.preferences.language;
    let k = state.preferences.ui_scale.scale();
    let style = text(24. * k, 550, -0.02);
    let h = TOOLBAR_HEIGHT * k;
    // All, with how many apps there are.
    let count = state
        .inventory
        .apps
        .iter()
        .filter(|app| !state.preferences.hidden.contains(&app.id))
        .count();
    let all = language.format("allApps", &[("count", &count.to_string())]);
    let width = assets.width(&all, style) + 40. * k;
    filter(
        out,
        state,
        Target::Filter(0),
        Rect::new(x, y - h / 2., width, h),
        &all,
        state.filter == 0,
    );
    x += width + 7.2 * k;
    for (index, label) in CATEGORIES.into_iter().enumerate() {
        let label = language.category(label);
        let width = assets.width(label, style) + 40. * k;
        filter(
            out,
            state,
            Target::Filter(index + 1),
            Rect::new(x, y - h / 2., width, h),
            label,
            state.filter == index + 1,
        );
        x += width + 7.2 * k;
    }
    if state.hidden_count() > 0 {
        let label = language.format("hidden", &[("count", &state.hidden_count().to_string())]);
        let rect = Rect::new(x, y - h / 2., assets.width(&label, style) + 40. * k, h);
        filter(
            out,
            state,
            Target::Filter(5),
            rect,
            &label,
            state.filter == 5,
        );
        x += rect.w + 7.2 * k;
    }
    x
}

/// The app search field, right-aligned at `right` and centred on `y`.
fn search_field(out: &mut Scene, state: &State, assets: &Assets, right: f32, y: f32, live: bool) {
    let language = state.preferences.language;
    let k = state.preferences.ui_scale.scale();
    let h = TOOLBAR_HEIGHT * k;
    let search = Rect::new(right - 320. * k, y - h / 2., 320. * k, h);
    let focused = live && matches!(state.focus, Target::Search | Target::ClearSearch);
    let query = if live { state.query.as_str() } else { "" };
    out.shape(
        search,
        48.,
        if focused {
            c(0x303438, 1.)
        } else {
            c(0x171c20, 0.72)
        },
    );
    out.border(
        search,
        48.,
        1.,
        c(0xffffff, if focused { 0.70 } else { 0.07 }),
    );
    if focused {
        out.border(search.inset(-3.), 51., 3., c(0xffffff, 0.063));
    }
    out.icon(
        assets,
        "search",
        Rect::new(
            search.x + 19.4 * k,
            search.y + (search.h - 19.2 * k) / 2.,
            19.2 * k,
            19.2 * k,
        ),
        c(0xeeeeee, 0.73),
    );
    out.text(
        if query.is_empty() {
            language.t("findApp")
        } else {
            query
        },
        Rect::new(
            search.x + 49.8 * k,
            search.y + (search.h - 33.6 * k) / 2.,
            if query.is_empty() { 250. * k } else { 218. * k },
            33.6 * k,
        ),
        text(24. * k, 550, -0.02).leading(1.4),
        c(0xeeeeee, if query.is_empty() { 0.73 } else { 1. }),
        false,
    );
    out.control(Target::Search, search, false);
    if !query.is_empty() {
        let clear = Rect::new(
            search.x + search.w - 45.8 * k,
            search.y + 14.8 * k,
            26.4 * k,
            26.4 * k,
        );
        out.shape(clear, 13.2 * k, c(0xffffff, 0.055));
        out.icon(assets, "close", clear.inset(5.2 * k), WHITE);
        if state.focus == Target::ClearSearch {
            out.border(clear.inset(-2.), 13.2 * k + 2., 2., WHITE);
        }
        out.control(Target::ClearSearch, clear, false);
    }
}

fn library_grid(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let s = state.scroll;
    let favorites = state.preferences.favorites(&state.inventory.apps);
    let apps = state.apps();
    if apps.is_empty() {
        let title = if state.loading {
            language.t("findingApps")
        } else if !state.query.is_empty() {
            language.t("noMatches")
        } else if state.filter == 5 {
            language.t("noHidden")
        } else {
            language.t("roomForNew")
        };
        let hint = state.error.clone().unwrap_or_else(|| {
            if !state.query.is_empty() {
                language.format("noMatchesHint", &[("query", &state.query)])
            } else if state.filter == 5 {
                language.t("hiddenHint").into()
            } else {
                language.t("noCategoryApps").into()
            }
        });
        empty(
            out,
            state,
            assets,
            EmptyState {
                rect: Rect::new(SAFE_X, APPS_GRID_TOP - s, WIDTH, 304.),
                icon: "grid",
                title,
                hint: &hint,
                action: state.error.as_ref().map(|_| Target::Retry),
            },
        );
    }
    let mut y = APPS_GRID_TOP;
    let names = state.preferences.app_names == crate::model::AppNames::Always;
    let on_focus = state.preferences.app_names == crate::model::AppNames::Focus;
    let width = crate::layout::column(APPS_COLUMNS);
    for row in apps.chunks(APPS_COLUMNS) {
        // Names under every card need their room; the focused card's name fits the gap.
        let row_height = if names {
            APPS_FACE_HEIGHT
                + 18.4
                + row
                    .iter()
                    .map(|app| caption_height(state, assets, app, width))
                    .fold(58.24 * caption_scale(state), f32::max)
                + 38.4
        } else {
            APPS_ROW_PITCH
        };
        for (column, app) in row.iter().enumerate() {
            let rect = Rect::new(
                SAFE_X + column as f32 * (width + GUTTER),
                y - s,
                width,
                APPS_FACE_HEIGHT,
            );
            tile(
                out,
                state,
                assets,
                app,
                rect,
                names,
                favorites.contains(&app.id),
            );
            if on_focus {
                focused_name(out, state, assets, app, rect);
            }
        }
        y += row_height;
    }
}

fn filter(out: &mut Scene, state: &State, target: Target, rect: Rect, label: &str, selected: bool) {
    let focused = state.focus == target;
    let k = rect.h / TOOLBAR_HEIGHT;
    out.pill(rect, focused, selected);
    if selected && !focused {
        out.border(rect, rect.h / 2., 1.5, c(0xffffff, 0.35));
    }
    out.text(
        label,
        Rect::new(rect.x, rect.y + (rect.h - 33.6 * k) / 2., rect.w, 33.6 * k),
        text(24. * k, 550, -0.02).leading(1.4),
        if focused {
            c(0x2c2c2c, 1.)
        } else {
            c(0xeeeeee, if selected { 1. } else { 0.73 })
        },
        true,
    );
    out.control(target, rect, false);
}

/// The existing System status card owns the only update action.
fn update_value(state: &State) -> String {
    use crate::update::{InstallStatus, Status};
    let language = state.preferences.language;
    match &state.update {
        InstallStatus::Idle => {
            if let Some(version) = &state.available_update {
                language.format("installUpdate", &[("version", version)])
            } else {
                language
                    .t(if state.update_status == Status::Checking {
                        "checkingUpdates"
                    } else {
                        "checkAgain"
                    })
                    .to_owned()
            }
        }
        InstallStatus::Downloading(percent) => {
            language.format("updateDownloading", &[("percent", &percent.to_string())])
        }
        InstallStatus::Installing(_) => language.t("updateInstalling").to_owned(),
        InstallStatus::Failed(_) => language.t("updateFailed").to_owned(),
    }
}

struct Row<'a> {
    target: Target,
    rect: Rect,
    title: &'a str,
    icon: Option<&'a str>,
    value: Option<&'a str>,
    description: Option<&'a str>,
    selected: bool,
    chevron: bool,
}
fn rotation_value_width(assets: &Assets, language: Language) -> f32 {
    ["stateOn", "stateOff"]
        .into_iter()
        .map(|key| assets.width(language.t(key), text(18.4, 450, 0.)))
        .fold(0., f32::max)
        .ceil()
}

fn row(out: &mut Scene, state: &State, assets: &Assets, row: Row<'_>) {
    let focused = state.focus == row.target;
    let bright = focused;
    let radius = if row.description.is_some() { 20. } else { 48. };
    if bright {
        out.draw.push(Draw::RoundedGradient(
            row.rect,
            radius,
            c(0xfafafa, 1.),
            c(0xe5e5e5, 1.),
        ));
    } else {
        settings_surface(out, row.rect, radius);
    }
    let color = if bright {
        c(0x2c2c2c, 1.)
    } else {
        c(0xeeeeee, 0.73)
    };
    let mut x = row.rect.x + 24.2;
    if let Some(icon) = row.icon {
        out.icon(
            assets,
            icon,
            Rect::new(x, row.rect.y + (row.rect.h - 23.2) / 2., 23.2, 23.2),
            if bright {
                c(0x444444, 1.)
            } else {
                c(0xe4e4e4, 0.61)
            },
        );
        x += 39.2;
    }
    let title_y = row.rect.y
        + if row.description.is_some() {
            17.
        } else {
            (row.rect.h - 29.12) / 2.
        };
    let compact_value_width = matches!(
        row.target,
        Target::WallpaperCycle | Target::WallpaperGradient
    )
    .then(|| rotation_value_width(assets, state.preferences.language));
    let right_inset = if let Some(width) = compact_value_width {
        width + 24.2 + 24.
    } else if row.value.is_some() {
        210.
    } else if matches!(row.target, Target::UploadWallpaper | Target::WallpaperCycle) {
        24.2
    } else {
        64.
    };
    let title_width = row.rect.w - (x - row.rect.x) - right_inset;
    let title_height = if row.description.is_some() {
        assets.lines(row.title, text(20.8, 550, -0.02), title_width) as f32 * 29.12
    } else {
        29.12
    };
    out.text(
        row.title,
        Rect::new(x, title_y, title_width, title_height),
        text(20.8, 550, -0.02).leading(1.4),
        color,
        false,
    );
    if let Some(description) = row.description {
        out.text(
            description,
            Rect::new(
                x,
                title_y + title_height + 4.8,
                row.rect.w - 100.,
                (row.rect.h - 34. - title_height - 4.8).max(22.8),
            ),
            text(15.2, 400, 0.).leading(1.5),
            if bright {
                c(0x555555, 1.)
            } else {
                c(0xf5f5f5, 0.56)
            },
            false,
        );
    }
    if let Some(value) = row.value {
        let value_width = compact_value_width.unwrap_or(170.);
        out.text(
            value,
            Rect::new(
                row.rect.x + row.rect.w - value_width - 24.2 - if row.chevron { 34.4 } else { 0. },
                row.rect.y + (row.rect.h - 23.) / 2.,
                value_width,
                24.,
            ),
            text(18.4, 450, 0.).right(),
            if bright {
                c(0x444444, 1.)
            } else {
                c(0xeeeeee, 0.73)
            },
            false,
        );
    }
    if row.chevron || row.selected {
        out.icon(
            assets,
            if row.selected { "check" } else { "right" },
            Rect::new(
                row.rect.x + row.rect.w - 42.6,
                row.rect.y + (row.rect.h - 18.4) / 2.,
                18.4,
                18.4,
            ),
            if bright {
                c(0x444444, 1.)
            } else {
                c(0xe4e4e4, 0.61)
            },
        );
    }
    let scrollable = !matches!(
        row.target,
        Target::UploadWallpaper | Target::WallpaperCycle | Target::WallpaperGradient
    );
    out.control(row.target, row.rect, scrollable);
}

fn weather_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let s = state.scroll;
    row(
        out,
        state,
        assets,
        Row {
            target: Target::WeatherToggle,
            rect: Rect::new(640., 164.8 - s, 1120., 72.),
            title: language.t("showWidget"),
            icon: Some("eye"),
            value: Some(if state.preferences.show_weather {
                language.t("stateOn")
            } else {
                language.t("stateOff")
            }),
            description: None,
            selected: false,
            chevron: false,
        },
    );
    let city = state
        .preferences
        .weather_location
        .as_ref()
        .map(|place| place.name.as_str());
    let style = text(18.4, 450, 0.).leading(1.4);
    let city_lines = city.map_or(1, |city| assets.lines(city, style, 450.));
    let height = (34. + city_lines as f32 * 25.76).max(72.);
    let rect = Rect::new(640., 264.8 - s, 1120., height);
    row(
        out,
        state,
        assets,
        Row {
            target: Target::WeatherLocation,
            rect,
            title: language.t("location"),
            icon: Some("location"),
            value: None,
            description: None,
            selected: false,
            chevron: true,
        },
    );
    if let Some(city) = city {
        out.text(
            city,
            Rect::new(
                rect.x + rect.w - 508.6,
                rect.y + (height - city_lines as f32 * 25.76) / 2.,
                450.,
                city_lines as f32 * 25.76,
            ),
            style.right(),
            if state.focus == Target::WeatherLocation {
                c(0x444444, 1.)
            } else {
                c(0xeeeeee, 0.73)
            },
            false,
        );
    }
    let y = rect.y + height + 12.;
    row(
        out,
        state,
        assets,
        Row {
            target: Target::WeatherTemperature,
            rect: Rect::new(640., y, 1120., 72.),
            title: language.t("temperature"),
            icon: Some("thermometer"),
            value: Some(state.preferences.temperature_unit.symbol()),
            description: None,
            selected: false,
            chevron: true,
        },
    );
}

fn crypto_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let s = state.scroll;
    let on_off = |on: bool| {
        Some(if on {
            language.t("stateOn")
        } else {
            language.t("stateOff")
        })
    };
    let chosen = |symbol: &str| state.preferences.crypto_coins.iter().any(|c| c == symbol);
    // BNB and XRP are their own names.
    let title = |symbol: &str| match crate::crypto::name(symbol) {
        name if name == symbol => symbol.to_owned(),
        name => format!("{name} · {symbol}"),
    };
    let simple = |out: &mut Scene, target, y: f32, title: &str, icon, value| {
        row(
            out,
            state,
            assets,
            Row {
                target,
                rect: Rect::new(640., y - s, 1120., 72.),
                title,
                icon,
                value,
                description: None,
                selected: false,
                chevron: false,
            },
        );
    };
    simple(
        out,
        Target::CryptoToggle,
        164.8,
        language.t("showWidget"),
        Some("eye"),
        on_off(state.preferences.show_crypto),
    );
    simple(
        out,
        Target::CryptoLayout,
        248.8,
        language.t("cryptoLayout"),
        Some("sparkles"),
        Some(place(language, state.preferences.crypto_beside)),
    );
    let mut y = 348.8;
    out.text(
        language.t("cryptoCoins"),
        Rect::new(660., y - s, 1080., 24.),
        text(16., 400, 0.),
        c(0xf5f5f5, 0.55),
        false,
    );
    y += 36.;
    let search = &state.crypto.search;
    simple(
        out,
        Target::CryptoSearch,
        y,
        language.t("cryptoFind"),
        Some("search"),
        (!search.query.is_empty()).then_some(search.query.as_str()),
    );
    y += 84.;
    if !search.query.is_empty() {
        let matches = search.matches();
        let status = if search.loading {
            Some("cryptoSearching")
        } else if search.failed && search.catalog.is_empty() {
            Some("cryptoCatalogError")
        } else if matches.is_empty() {
            Some("cryptoNoMatch")
        } else {
            None
        };
        if let Some(key) = status {
            out.text(
                language.t(key),
                Rect::new(660., y + 4. - s, 1080., 24.),
                text(17.6, 400, 0.),
                c(0xf5f5f5, 0.6),
                false,
            );
            y += 52.;
        }
        for (index, symbol) in matches.into_iter().enumerate() {
            simple(
                out,
                Target::CryptoResult(index),
                y,
                &title(symbol),
                None,
                on_off(chosen(symbol)),
            );
            y += 84.;
        }
        y += 20.;
    }
    for (index, symbol) in state.crypto_choices().iter().enumerate() {
        simple(
            out,
            Target::CryptoCoin(index),
            y,
            &title(symbol),
            None,
            on_off(chosen(symbol)),
        );
        y += 84.;
    }
}

fn cs2_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let s = state.scroll;
    let preferences = &state.preferences;
    for (index, (target, key, icon, on)) in [
        (Target::Cs2Toggle, "showWidget", "eye", preferences.show_cs2),
        (
            Target::Cs2Layout,
            "cryptoLayout",
            "grid",
            preferences.cs2_today,
        ),
        (
            Target::Cs2Major,
            "cs2MajorOnly",
            "sparkles",
            preferences.cs2_major_only,
        ),
        (
            Target::Cs2Results,
            "cs2Results",
            "check",
            preferences.cs2_results,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let layout = target == Target::Cs2Layout;
        row(
            out,
            state,
            assets,
            Row {
                target,
                rect: Rect::new(640., 164.8 + index as f32 * 84. - s, 1120., 72.),
                title: language.t(key),
                icon: Some(icon),
                value: Some(if layout {
                    place(language, on)
                } else {
                    language.t(if on { "stateOn" } else { "stateOff" })
                }),
                description: None,
                selected: false,
                chevron: false,
            },
        );
    }
    out.text(
        language.t("cs2Source"),
        Rect::new(660., 164.8 + 4. * 84. + 6. - s, 1080., 24.),
        text(16., 400, 0.),
        c(0xf5f5f5, 0.55),
        false,
    );
}

fn location_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let mut y = 164.8 - state.scroll;
    if let Some(place) = &state.preferences.weather_location {
        let rect = Rect::new(640., y, 1120., 76.2);
        out.shape(rect, 20., c(0x171c20, 0.72));
        out.border(rect, 20., 1., c(0xffffff, 0.04));
        out.icon(
            assets,
            "location",
            Rect::new(664.2, y + 29.1, 18., 18.),
            c(0xcccccc, 1.),
        );
        out.text(
            &place.name,
            Rect::new(698.2, y + 17., 920., 25.),
            text(18.4, 500, 0.),
            WHITE,
            false,
        );
        out.text(
            place.detail(),
            Rect::new(698.2, y + 42.3, 920., 22.),
            text(14.4, 400, 0.),
            c(0xf5f5f5, 0.56),
            false,
        );
        let clear = Rect::new(1695.8, y + 18.1, 40., 40.);
        out.shape(clear, 20., c(0x23282c, 0.5));
        out.border(clear, 20., 1., c(0xffffff, 0.07));
        out.icon(assets, "close", clear.inset(12.), WHITE);
        if state.focus == Target::WeatherClear {
            out.border(clear.inset(-2.), 22., 2., c(0xf0f0f0, 0.95));
        }
        out.control(Target::WeatherClear, clear, true);
        y += 96.2;
    }
    let query = Rect::new(640., y, 1044., 64.);
    let focused = state.focus == Target::WeatherQuery;
    out.shape(
        query,
        48.,
        if focused {
            c(0x303438, 1.)
        } else {
            c(0x171c20, 0.72)
        },
    );
    out.border(
        query,
        48.,
        1.,
        c(0xffffff, if focused { 0.70 } else { 0.07 }),
    );
    if focused {
        out.border(query.inset(-3.), 51., 3., c(0xffffff, 0.063));
    }
    out.icon(
        assets,
        "search",
        Rect::new(664.2, y + 21.2, 21.6, 21.6),
        c(0xeeeeee, 0.73),
    );
    out.text(
        if state.weather.query.is_empty() {
            language.t("cityPostcode")
        } else {
            &state.weather.query
        },
        Rect::new(699.4, y + 20.9, 950., 24.),
        text(18.4, 400, -0.02),
        c(0xeeeeee, 0.73),
        false,
    );
    out.control(Target::WeatherQuery, query, true);
    let search = Rect::new(1696., y, 64., 64.);
    out.shape(search, 32., c(0x171c20, 0.72));
    out.border(search, 32., 1., c(0xffffff, 0.07));
    out.icon(
        assets,
        if state.weather.searching {
            "refresh"
        } else {
            "search"
        },
        search.inset(20.8),
        WHITE,
    );
    if state.focus == Target::WeatherSearch {
        out.border(search.inset(-7.), 39., 2., c(0xf0f0f0, 0.95));
    }
    out.control(Target::WeatherSearch, search, true);
    y += 84.;
    if state.weather.searching || state.weather.message.is_some() {
        out.text(
            if state.weather.searching {
                language.t("searching")
            } else {
                language.t(state.weather.message.as_deref().unwrap_or(""))
            },
            Rect::new(664.2, y, 1071.6, 58.),
            text(16., 400, 0.).leading(1.5),
            c(0xe0e0e0, 0.68),
            false,
        );
    }
    for (index, place) in state.weather.results.iter().enumerate() {
        let detail = place.detail();
        let height = (34.
            + assets.lines(&place.name, text(20.8, 550, -0.02), 992.6) as f32 * 29.12
            + 4.8
            + assets.lines(&detail, text(15.2, 400, 0.), 1020.) as f32 * 22.8)
            .max(90.7);
        let rect = Rect::new(640., y, 1120., height);
        row(
            out,
            state,
            assets,
            Row {
                target: Target::WeatherResult(index),
                rect,
                title: &place.name,
                icon: Some("location"),
                value: None,
                description: Some(&detail),
                selected: false,
                chevron: false,
            },
        );
        if state
            .preferences
            .weather_location
            .as_ref()
            .is_some_and(|saved| saved.key() == place.key())
        {
            out.icon(
                assets,
                "check",
                Rect::new(1717.4, y + (height - 18.4) / 2., 18.4, 18.4),
                if state.focus == Target::WeatherResult(index) {
                    c(0x444444, 1.)
                } else {
                    c(0xe4e4e4, 0.61)
                },
            );
        }
        y += height + 12.;
    }
}

fn settings(out: &mut Scene, state: &State, assets: &Assets) {
    settings_content(out, state, assets);
    settings_sidebar(out, state, assets);
}

fn settings_sidebar(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    for (index, key) in ["interface", "background", "widgets", "system"]
        .into_iter()
        .enumerate()
    {
        let label = language.t(key);
        let rect = Rect::new(160., 164.8 + index as f32 * 84., 403.2, 72.);
        let focused = state.focus == Target::Sidebar(index);
        out.pill(rect, focused, state.section == index);
        let color = if focused {
            c(0x2c2c2c, 1.)
        } else if state.section == index {
            WHITE
        } else {
            c(0xeeeeee, 0.73)
        };
        if state.section == index && state.subpage.is_some() {
            let nested_path = match &state.subpage {
                Some(Subpage::DateFormat) => {
                    Some(vec![language.t("date"), language.t("dateFormat")])
                }
                Some(Subpage::ClockFormat) => {
                    Some(vec![language.t("clock"), language.t("clockFormat")])
                }
                Some(Subpage::TimeFormat) => {
                    Some(vec![language.t("clock"), language.t("timeFormat")])
                }
                Some(Subpage::WeatherLocation | Subpage::Temperature) => Some(vec![
                    language.t("weather"),
                    language.t(if state.subpage == Some(Subpage::WeatherLocation) {
                        "location"
                    } else {
                        "temperature"
                    }),
                ]),
                Some(Subpage::Input(id)) => Some(vec![
                    language.t("inputs"),
                    state
                        .inventory
                        .inputs
                        .iter()
                        .find(|input| &input.id == id)
                        .map_or(language.t("inputs"), |input| {
                            state.preferences.input_title(input)
                        }),
                ]),
                _ => None,
            };
            let sub = match &state.subpage {
                Some(Subpage::AppNames) => language.t("appNames"),
                Some(Subpage::Greeting) => language.t("greeting"),
                Some(Subpage::Date) => language.t("date"),
                Some(Subpage::Weather) => language.t("weather"),
                Some(Subpage::Shortcuts) => language.t("shortcuts"),
                Some(Subpage::Crypto) => language.t("crypto"),
                Some(Subpage::Inputs) => language.t("inputs"),
                Some(Subpage::Clock) => language.t("clock"),
                Some(Subpage::Cs2) => language.t("cs2Score"),
                Some(Subpage::Shows) => language.t("shows"),
                Some(Subpage::Language) => language.t("language"),
                Some(Subpage::Appearance) => language.t("uiScale"),
                Some(Subpage::TextRendering) => language.t("textRendering"),
                Some(Subpage::HomeTakeover) => language.t("homeTakeover"),
                _ => "",
            };
            let mut path = vec![label];
            path.extend(nested_path.unwrap_or_else(|| vec![sub]));
            let gap = 6.;
            let chevron_size = 17.;
            let separators_width = (path.len() - 1) as f32 * (gap * 2. + chevron_size);
            let mut style = text(20.8, 550, -0.02);
            let text_width: f32 = path.iter().map(|part| assets.width(part, style)).sum();
            style.size *= ((rect.w - 48.4 - separators_width) / text_width).min(1.);
            let muted = if focused {
                c(0x2c2c2c, 0.7)
            } else {
                c(0xeeeeee, 0.73)
            };
            let mut x = rect.x + 24.2;
            for (position, &part) in path.iter().enumerate() {
                if position > 0 {
                    out.icon(
                        assets,
                        "right",
                        Rect::new(x + gap, rect.y + 27., chevron_size, chevron_size),
                        muted,
                    );
                    x += gap * 2. + chevron_size;
                }
                let width = assets.width(part, style);
                out.text(
                    part,
                    Rect::new(x, rect.y + 23.5, width + 2., 25.),
                    style,
                    if position == path.len() - 1 {
                        color
                    } else {
                        muted
                    },
                    false,
                );
                x += width;
            }
        } else {
            out.text(
                label,
                Rect::new(rect.x + 24.2, rect.y + 23.5, 330., 25.),
                text(20.8, 550, -0.02),
                color,
                false,
            );
            if state.section == index {
                out.icon(
                    assets,
                    "right",
                    Rect::new(rect.x + rect.w - 42.6, rect.y + 26.8, 18.4, 18.4),
                    if focused {
                        c(0x2c2c2c, 0.6)
                    } else {
                        c(0xeeeeee, 0.5)
                    },
                );
            }
        }
        out.control(Target::Sidebar(index), rect, false);
    }
}

fn settings_content(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let s = state.scroll;
    if let Some(subpage) = &state.subpage {
        let start = out.draw.len();
        if matches!(
            subpage,
            Subpage::WeatherLocation
                | Subpage::Cs2
                | Subpage::Shows
                | Subpage::Weather
                | Subpage::Shortcuts
                | Subpage::Inputs
                | Subpage::Crypto
                | Subpage::Greeting
                | Subpage::Date
                | Subpage::AppNames
                | Subpage::Clock
        ) {
            match subpage {
                Subpage::WeatherLocation => location_settings(out, state, assets),
                Subpage::Cs2 => cs2_settings(out, state, assets),
                Subpage::Shows => shows_settings(out, state, assets),
                Subpage::Weather => weather_settings(out, state, assets),
                Subpage::Shortcuts => shortcut_settings(out, state, assets),
                Subpage::Inputs => inputs_settings(out, state, assets),
                Subpage::Crypto => crypto_settings(out, state, assets),
                Subpage::Greeting => greeting_settings(out, state, assets),
                Subpage::Date => date_settings(out, state, assets),
                Subpage::AppNames => app_name_settings(out, state, assets),
                Subpage::Clock => clock_settings(out, state, assets),
                _ => unreachable!(),
            }
            if let Some(opened) = state.panel_started {
                out.fade(start, (opened.elapsed().as_secs_f32() / 0.15).min(1.), 1.);
            }
            return;
        }
        let options: Vec<(&str, Option<&str>, bool)> = match subpage {
            Subpage::DateFormat => crate::model::DateFormat::ALL
                .iter()
                .map(|format| {
                    (
                        format.label(language),
                        None,
                        *format == state.preferences.date_format,
                    )
                })
                .collect(),
            Subpage::ClockFormat => vec![
                (language.t("hour12"), None, !state.preferences.hour_24),
                (language.t("hour24"), None, state.preferences.hour_24),
            ],
            Subpage::TimeFormat => vec![
                ("HH:MM", None, !state.preferences.show_seconds),
                ("HH:MM:SS", None, state.preferences.show_seconds),
            ],
            Subpage::Input(id) => {
                let current = state
                    .preferences
                    .input_modes
                    .get(id)
                    .copied()
                    .unwrap_or_default();
                vec![
                    (
                        language.t("stateOn"),
                        Some(language.t("inputAlways")),
                        current == InputMode::On,
                    ),
                    (
                        language.t("stateOff"),
                        Some(language.t("inputNever")),
                        current == InputMode::Off,
                    ),
                    (
                        language.t("stateAuto"),
                        Some(language.t(if id == "airplay" {
                            "airplayInputAuto"
                        } else {
                            "inputWhenConnected"
                        })),
                        current == InputMode::Auto,
                    ),
                ]
            }
            Subpage::Temperature => vec![
                (
                    language.t("celsius"),
                    None,
                    state.preferences.temperature_unit == Unit::Celsius,
                ),
                (
                    language.t("fahrenheit"),
                    None,
                    state.preferences.temperature_unit == Unit::Fahrenheit,
                ),
            ],
            Subpage::WeatherLocation
            | Subpage::Cs2
            | Subpage::Shows
            | Subpage::Weather
            | Subpage::Shortcuts
            | Subpage::Inputs
            | Subpage::Crypto
            | Subpage::Greeting
            | Subpage::Date
            | Subpage::AppNames
            | Subpage::Clock => {
                unreachable!()
            }
            Subpage::Language => Language::ALL
                .into_iter()
                .map(|option| (option.name(), None, option == language))
                .collect(),
            Subpage::Appearance => UiSize::ALL
                .into_iter()
                .map(|size| {
                    let (name, hint) = size.keys();
                    (
                        language.t(name),
                        Some(language.t(hint)),
                        size == state.preferences.ui_scale,
                    )
                })
                .collect(),
            Subpage::TextRendering => crate::text_rendering::TextRendering::ALL
                .into_iter()
                .map(|mode| {
                    (
                        language.t(mode.label_key()),
                        Some(language.t("textRenderingRestart")),
                        mode == state.preferences.text_rendering,
                    )
                })
                .collect(),
            Subpage::HomeTakeover => vec![
                (
                    language.t("stateOn"),
                    Some(language.t("homeTakeoverOnHint")),
                    state.home_takeover,
                ),
                (
                    language.t("stateOff"),
                    Some(language.t("homeTakeoverOffHint")),
                    !state.home_takeover,
                ),
            ],
        };
        for (index, (title, description, selected)) in options.into_iter().enumerate() {
            let h = if description.is_some() { 90.7 } else { 72. };
            row(
                out,
                state,
                assets,
                Row {
                    target: Target::Choice(index),
                    rect: Rect::new(640., 164.8 + index as f32 * (h + 12.) - s, 1120., h),
                    title,
                    icon: None,
                    value: None,
                    description,
                    selected,
                    chevron: false,
                },
            );
        }
        if let Subpage::Input(id) = subpage {
            input_name_rows(out, state, assets, id, 164.8 + 3. * 102.7 + 16. - s);
        }
        if *subpage == Subpage::Appearance {
            appearance_preview(out, state, assets, 164.8 + 3. * 102.7 + 8. - s);
        }
        if let Some(opened) = state.panel_started {
            out.fade(start, (opened.elapsed().as_secs_f32() / 0.15).min(1.), 1.);
        }
        return;
    }
    match state.section {
        0 => {
            for (position, (index, key, icon, value, chevron)) in [
                (0, "appNames", "type", None, true),
                (1, "clock", "clock", None, true),
                (4, "language", "language", Some(language.name()), true),
                (
                    2,
                    "reducedMotion",
                    "motion",
                    Some(if state.preferences.reduced_motion {
                        language.t("stateOn")
                    } else {
                        language.t("stateOff")
                    }),
                    false,
                ),
                (
                    crate::state::FOCUS_GLOW_ROW,
                    "focusGlow",
                    "sparkles",
                    Some(language.t(state.preferences.focus_glow.key())),
                    false,
                ),
                (
                    crate::state::INTERFACE_SCALE_ROW,
                    "uiScale",
                    "grid",
                    Some(language.t(state.preferences.ui_scale.keys().0)),
                    true,
                ),
            ]
            .into_iter()
            .enumerate()
            {
                row(
                    out,
                    state,
                    assets,
                    Row {
                        target: Target::Setting(index),
                        rect: Rect::new(640., 164.8 + position as f32 * 84. - s, 1120., 72.),
                        title: language.t(key),
                        icon: Some(icon),
                        value,
                        description: None,
                        selected: false,
                        chevron,
                    },
                );
            }
        }
        1 => {
            let actions_start = out.draw.len();
            let upload_label = language.t("uploadWallpaper");
            let upload_width = (assets.width(upload_label, text(20.8, 550, -0.02)) + 88.)
                .ceil()
                .min(520.);
            let cycle_label = language.t("wallpaperCycle");
            let gradient_label = language.t("wallpaperGradient");
            let gradient_width = (assets.width(gradient_label, text(20.8, 550, -0.02))
                + rotation_value_width(assets, language)
                + 111.6)
                .ceil()
                .min(360.);
            let cycle_width = (assets.width(cycle_label, text(20.8, 550, -0.02))
                + rotation_value_width(assets, language)
                + 111.6)
                .ceil()
                .min(520.);
            row(
                out,
                state,
                assets,
                Row {
                    target: Target::WallpaperCycle,
                    rect: Rect::new(1760. - cycle_width, 164.8, cycle_width, 64.),
                    title: cycle_label,
                    icon: Some(if state.preferences.wallpaper_cycle {
                        "refresh"
                    } else {
                        "refresh-off"
                    }),
                    value: Some(language.t(if state.preferences.wallpaper_cycle {
                        "stateOn"
                    } else {
                        "stateOff"
                    })),
                    description: None,
                    selected: false,
                    chevron: false,
                },
            );
            row(
                out,
                state,
                assets,
                Row {
                    target: Target::WallpaperGradient,
                    rect: Rect::new(
                        1760. - cycle_width - 20. - gradient_width,
                        164.8,
                        gradient_width,
                        64.,
                    ),
                    title: gradient_label,
                    icon: Some(if state.preferences.wallpaper_gradient {
                        "eye"
                    } else {
                        "eye-off"
                    }),
                    value: Some(language.t(if state.preferences.wallpaper_gradient {
                        "stateOn"
                    } else {
                        "stateOff"
                    })),
                    description: None,
                    selected: false,
                    chevron: false,
                },
            );
            row(
                out,
                state,
                assets,
                Row {
                    target: Target::UploadWallpaper,
                    rect: Rect::new(
                        1760. - cycle_width - 40. - gradient_width - upload_width,
                        164.8,
                        upload_width,
                        64.,
                    ),
                    title: upload_label,
                    icon: Some("upload"),
                    value: None,
                    description: None,
                    selected: false,
                    chevron: false,
                },
            );
            // Fixed actions sit above the gallery, without cutting off scrolling cards.
            let actions = out.draw.split_off(actions_start);
            for index in 0..state.background_count() {
                let id = state.background_id(index).unwrap();
                let rect = Rect::new(
                    640. + (index % 3) as f32 * 380.,
                    252.8 + (index / 3) as f32 * 226.5 - s,
                    360.,
                    202.5,
                );
                out.shadow(rect, 16., 15., 3., 0.13);
                background_card(out, assets, state, id, rect);
                if id == crate::aerials::ID {
                    aerials_caption(out, assets, state, rect);
                }
                out.border(rect, 16., 1., c(0xffffff, 0.086));
                if state.preferences.background == id {
                    out.border(rect.inset(-2.), 18., 2., c(0xdedede, 1.));
                    out.border(rect.inset(-7.), 23., 3., c(0xdedede, 0.10));
                    let check = Rect::new(rect.x + rect.w - 28., rect.y + rect.h - 28., 21.6, 21.6);
                    out.shape(check, 10.8, c(0xf3f3f3, 1.));
                    out.icon(assets, "check", check.inset(3.6), c(0x333333, 1.));
                }
                if state.focus == Target::Background(index) {
                    out.ring(rect, 16.);
                }
                out.control(Target::Background(index), rect, true);
            }
            out.draw.extend(actions);
        }
        WIDGETS_SECTION => {
            for (index, (widget, key, icon)) in [
                (4, "date", "clock"),
                (0, "greeting", "type"),
                (1, "weather", "weather-sun"),
                (2, "shortcuts", "sparkles"),
                (3, "inputs", "input-hdmi"),
                (5, "crypto", "globe"),
                // Last, so the rows above keep their places.
                (6, "cs2Score", "gaming"),
                (7, "shows", "tv"),
            ]
            .into_iter()
            .enumerate()
            {
                row(
                    out,
                    state,
                    assets,
                    Row {
                        target: Target::Widget(widget),
                        rect: Rect::new(640., 164.8 + index as f32 * 84. - s, 1120., 72.),
                        title: language.t(key),
                        icon: Some(icon),
                        value: None,
                        description: None,
                        selected: false,
                        chevron: true,
                    },
                );
            }
        }
        SYSTEM_SECTION => {
            let rect = Rect::new(640., 164.8 - s, 1120., 220.);
            let title = if matches!(state.update, crate::update::InstallStatus::Failed(_)) {
                "updateError"
            } else if state.available_update.is_some() {
                "updateAvailable"
            } else {
                match state.update_status {
                    crate::update::Status::Unchecked => "checkForUpdates",
                    crate::update::Status::Checking => "checkingUpdates",
                    crate::update::Status::Checked => "latestVersionInstalled",
                    crate::update::Status::Failed => "updateCheckFailed",
                }
            };
            settings_surface(out, rect, 20.);
            out.text(
                language.t(title),
                Rect::new(rect.x + 24.2, rect.y + 24., rect.w - 48.4, 34.),
                text(24., 550, -0.02).leading(1.4),
                WHITE,
                false,
            );
            out.text(
                if let Some(latest) = &state.available_update {
                    language.format(
                        "updateVersions",
                        &[("current", assets.version.trim()), ("latest", latest)],
                    )
                } else {
                    language.format("currentVersion", &[("current", assets.version.trim())])
                },
                Rect::new(rect.x + 24.2, rect.y + 74., rect.w - 48.4, 32.),
                text(20.8, 450, -0.02).leading(1.4),
                c(0xeeeeee, 0.73),
                false,
            );
            row(
                out,
                state,
                assets,
                Row {
                    target: if state.available_update.is_some() {
                        Target::InstallUpdate
                    } else {
                        Target::CheckUpdates
                    },
                    rect: Rect::new(rect.x + 24., rect.y + 132., rect.w - 48., 64.),
                    title: &update_value(state),
                    icon: Some(if state.available_update.is_some() {
                        "arrow"
                    } else {
                        "refresh"
                    }),
                    value: None,
                    description: None,
                    selected: false,
                    chevron: false,
                },
            );
            for (position, (index, title, icon, value, chevron)) in [
                (
                    3,
                    language.t(if state.loading {
                        "refreshNotice"
                    } else {
                        "refreshApps"
                    }),
                    "refresh",
                    None,
                    false,
                ),
                (
                    7,
                    language.t("textRendering"),
                    "type",
                    Some(language.t(state.preferences.text_rendering.label_key())),
                    true,
                ),
                (
                    HOMEBREW_UPDATES_ROW,
                    language.t("homebrewUpdates"),
                    "refresh",
                    Some(language.t(if state.preferences.homebrew_updates {
                        "stateOn"
                    } else {
                        "stateOff"
                    })),
                    false,
                ),
                (5, language.t("quitApp"), "power", None, false),
                (
                    crate::state::HOME_TAKEOVER_ROW,
                    language.t("homeTakeover"),
                    "tv",
                    Some(language.t(if state.home_takeover {
                        "stateOn"
                    } else {
                        "stateOff"
                    })),
                    true,
                ),
            ]
            .into_iter()
            .enumerate()
            {
                row(
                    out,
                    state,
                    assets,
                    Row {
                        target: Target::Setting(index),
                        rect: Rect::new(640., 408.8 + position as f32 * 84. - s, 1120., 72.),
                        title,
                        icon: Some(icon),
                        value,
                        description: None,
                        selected: false,
                        chevron,
                    },
                );
            }
        }
        _ => {}
    }
}

/// General › Interface scale: below the options, the chosen scale at its real size, drawn with
/// the pages' own code (category chips, the search field, favourite tiles with names). The clock
/// and the hints are on this screen already and change in place.
fn appearance_preview(out: &mut Scene, state: &State, assets: &Assets, top: f32) {
    let language = state.preferences.language;
    let k = state.preferences.ui_scale.scale();
    let panel = Rect::new(640., top, 1120., 982.4 - top);
    out.shape(panel, 22., c(0x0a0d12, 0.42));
    out.border(panel, 22., 1., c(0xffffff, 0.12));
    out.text(
        language.t("sample"),
        Rect::new(panel.x + 26., panel.y + 16., 600., 24.),
        text(17.6, 500, 0.04),
        c(0xf0f0f0, 0.55),
        false,
    );
    // Its controls are dropped, so the preview never takes focus.
    let controls = out.controls.len();
    let chips = panel.y + 56. + 28. * k;
    category_chips(out, state, assets, panel.x + 32., chips);
    search_field(out, state, assets, panel.x + panel.w - 32., chips, false);
    let favorites = state.preferences.favorites(&state.inventory.apps);
    let apps = favorites
        .iter()
        .filter(|id| !state.preferences.hidden.contains(id))
        .filter_map(|id| state.inventory.apps.iter().find(|app| &app.id == id))
        .take(3);
    for (index, app) in apps.enumerate() {
        let rect = Rect::new(
            panel.x + 32. + index as f32 * HOME_TILE_PITCH,
            chips + 28. * k + 28.,
            HOME_TILE_WIDTH,
            FACE_HEIGHT,
        );
        tile(out, state, assets, app, rect, true, false);
    }
    out.controls.truncate(controls);
}

fn inputs_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let s = state.scroll;
    for (index, input) in state.inventory.inputs.iter().enumerate() {
        let mode = state
            .preferences
            .input_modes
            .get(&input.id)
            .copied()
            .unwrap_or_default();
        row(
            out,
            state,
            assets,
            Row {
                target: Target::InputMode(input.id.clone()),
                rect: Rect::new(640., 164.8 + index as f32 * 102.7 - s, 1120., 90.7),
                title: state.preferences.input_title(input),
                icon: Some(&format!(
                    "input-{}",
                    input_icon(input, state.preferences.input_name(input))
                )),
                value: Some(language.t(match mode {
                    InputMode::On => "stateOn",
                    InputMode::Off => "stateOff",
                    InputMode::Auto => "stateAuto",
                })),
                description: Some(&input_status(
                    input,
                    state.preferences.input_name(input).is_some(),
                    language,
                )),
                selected: false,
                chevron: true,
            },
        );
    }
    let mut y = 164.8 + state.inventory.inputs.len() as f32 * 102.7 + 16. - s;
    if state.inventory.inputs.is_empty() {
        out.text(
            language.t("noInputs"),
            Rect::new(664., 165. - s, 1000., 26.),
            text(16.8, 400, 0.).leading(1.5),
            c(0xf5f5f5, 0.72),
            false,
        );
        y += 36.;
    }
    row(
        out,
        state,
        assets,
        Row {
            target: Target::Setting(3),
            rect: Rect::new(640., y, 1120., 72.),
            title: if state.loading {
                language.t("refreshNotice")
            } else {
                language.t("refreshInputs")
            },
            icon: Some("refresh"),
            value: None,
            description: None,
            selected: false,
            chevron: false,
        },
    );
}

fn greeting_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let description = if state.text_input_active() && state.keyboard_field == TextField::Name {
        format!("{}|", state.greeting_name_draft)
    } else if state.preferences.greeting_name.is_empty() {
        language.t("stateOff").into()
    } else {
        state.preferences.greeting_name.clone()
    };
    row(
        out,
        state,
        assets,
        Row {
            target: Target::GreetingName,
            rect: Rect::new(640., 248.8 - state.scroll, 1120., 90.7),
            title: language.t("name"),
            icon: Some("type"),
            value: None,
            description: Some(&description),
            selected: false,
            chevron: true,
        },
    );
    // Always present so completing name entry cannot move a clicked control.
    row(
        out,
        state,
        assets,
        Row {
            target: Target::GreetingNameClear,
            rect: Rect::new(640., 351.5 - state.scroll, 1120., 72.),
            title: language.t("clearName"),
            icon: Some("refresh"),
            value: None,
            description: None,
            selected: false,
            chevron: false,
        },
    );
    row(
        out,
        state,
        assets,
        Row {
            target: Target::GreetingToggle,
            rect: Rect::new(640., 164.8 - state.scroll, 1120., 72.),
            title: language.t("showWidget"),
            icon: None,
            value: Some(language.t(if state.preferences.show_greeting {
                "stateOn"
            } else {
                "stateOff"
            })),
            description: None,
            selected: false,
            chevron: false,
        },
    );
}

fn date_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    for (index, (target, title, value, chevron)) in [
        (
            Target::DateToggle,
            "showWidget",
            language.t(if state.preferences.show_date {
                "stateOn"
            } else {
                "stateOff"
            }),
            false,
        ),
        (
            Target::DateFormat,
            "dateFormat",
            state.preferences.date_format.label(language),
            true,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        row(
            out,
            state,
            assets,
            Row {
                target,
                rect: Rect::new(640., 164.8 + index as f32 * 84. - state.scroll, 1120., 72.),
                title: language.t(title),
                icon: None,
                value: Some(value),
                description: None,
                selected: false,
                chevron,
            },
        );
    }
}

fn clock_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    for (index, (target, key, value)) in [
        (
            Target::ClockFormat,
            "clockFormat",
            language.t(if state.preferences.hour_24 {
                "hour24"
            } else {
                "hour12"
            }),
        ),
        (
            Target::ClockTimeFormat,
            "timeFormat",
            if state.preferences.show_seconds {
                "HH:MM:SS"
            } else {
                "HH:MM"
            },
        ),
    ]
    .into_iter()
    .enumerate()
    {
        row(
            out,
            state,
            assets,
            Row {
                target,
                rect: Rect::new(640., 164.8 + index as f32 * 84. - state.scroll, 1120., 72.),
                title: language.t(key),
                icon: None,
                value: Some(value),
                description: None,
                selected: false,
                chevron: true,
            },
        );
    }
}

fn app_name_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    for (index, (target, title, enabled)) in [
        (
            Target::AppNamesHome,
            "showOnHome",
            state.preferences.show_home_names,
        ),
        (
            Target::AppNamesApps,
            "showOnApps",
            state.preferences.app_names != crate::model::AppNames::Off,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let value = if target == Target::AppNamesApps {
            language.t(state.preferences.app_names.key())
        } else {
            language.t(if enabled { "stateOn" } else { "stateOff" })
        };
        row(
            out,
            state,
            assets,
            Row {
                target,
                rect: Rect::new(640., 164.8 + index as f32 * 84. - state.scroll, 1120., 72.),
                title: language.t(title),
                icon: None,
                value: Some(value),
                description: None,
                selected: false,
                chevron: false,
            },
        );
    }
}

fn input_name_rows(out: &mut Scene, state: &State, assets: &Assets, id: &str, y: f32) {
    let language = state.preferences.language;
    let custom = state.preferences.input_names.get(id);
    let editing = state.text_input_active() && state.keyboard_field == TextField::InputName;
    let description = if editing {
        format!("{}|", state.input_name)
    } else {
        custom
            .cloned()
            .unwrap_or_else(|| language.t("inputNameHint").into())
    };
    row(
        out,
        state,
        assets,
        Row {
            target: Target::InputRename,
            rect: Rect::new(640., y, 1120., 90.7),
            title: language.t("renameInput"),
            icon: Some("type"),
            value: None,
            description: Some(&description),
            selected: false,
            chevron: false,
        },
    );
    if custom.is_some() {
        row(
            out,
            state,
            assets,
            Row {
                target: Target::InputNameReset,
                rect: Rect::new(640., y + 102.7, 1120., 72.),
                title: language.t("clearName"),
                icon: Some("refresh"),
                value: None,
                description: None,
                selected: false,
                chevron: false,
            },
        );
    }
}

fn settings_surface(out: &mut Scene, rect: Rect, radius: f32) {
    out.shape(rect, radius, c(0x171c20, 0.72));
    out.border(rect, radius, 1., c(0xffffff, 0.07));
}

fn shortcut_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let tiles = &state.preferences.custom_tiles;
    for (index, tile) in tiles.iter().enumerate() {
        let description = format!("{} · {}", tile.method.label(), shorten(&tile.url));
        let y = 248.8 + index as f32 * 102.7 - state.scroll;
        row(
            out,
            state,
            assets,
            Row {
                target: Target::Tile(tile.id.clone()),
                rect: Rect::new(640., y, 1120., 90.7),
                title: &tile.title,
                icon: Some(tile.icon()),
                value: None,
                description: Some(&description),
                selected: false,
                chevron: false,
            },
        );
    }
    if tiles.is_empty() {
        let rect = Rect::new(640., 248.8 - state.scroll, 1120., 220.);
        settings_surface(out, rect, 20.);
        out.icon(
            assets,
            "sparkles",
            Rect::new(rect.x + (rect.w - 32.) / 2., rect.y + 28., 32., 32.),
            c(0xeeeeee, 0.73),
        );
        out.text(
            language.t("noTiles"),
            Rect::new(rect.x + 32., rect.y + 80., rect.w - 64., 34.),
            text(24., 550, -0.02).leading(1.4),
            WHITE,
            true,
        );
        out.text(
            language.t("addFirstShortcutHint"),
            Rect::new(rect.x + 80., rect.y + 130., rect.w - 160., 56.),
            text(18.4, 400, 0.).leading(1.5),
            c(0xf5f5f5, 0.73),
            true,
        );
    }
    row(
        out,
        state,
        assets,
        Row {
            target: Target::TileEditor,
            rect: Rect::new(640., 164.8 - state.scroll, 1120., 72.),
            title: language.t("configure"),
            icon: Some("settings"),
            value: None,
            description: None,
            selected: false,
            chevron: false,
        },
    );
}

/// URLs have no break opportunities, so long ones are cut to a single line.
fn shorten(text: &str) -> String {
    const MAX: usize = 96;
    let count = text.chars().count();
    if count <= MAX {
        text.into()
    } else {
        format!("{}…", text.chars().take(MAX - 1).collect::<String>())
    }
}

/// A custom name such as "PS5" can choose the icon; otherwise the TV's label.
fn input_icon(input: &Input, custom: Option<&str>) -> &'static str {
    if input.id == "airplay" {
        return "airplay";
    }
    custom
        .map(device_icon)
        .filter(|icon| *icon != "hdmi")
        .unwrap_or_else(|| device_icon(&input.title))
}

fn device_icon(name: &str) -> &'static str {
    let name = name.to_lowercase().replace(' ', "");
    if name.contains("playstation") || ["ps1", "ps2", "ps3", "ps4", "ps5"].contains(&name.as_str())
    {
        "playstation"
    } else if name.contains("appletv") {
        "apple-tv"
    } else if name.contains("usb") {
        "usb"
    } else if ["xbox", "switch", "console"]
        .iter()
        .any(|part| name.contains(part))
    {
        "console"
    } else if ["pc", "computer", "laptop"]
        .iter()
        .any(|part| name.contains(part))
    {
        "computer"
    } else if ["stream", "fire", "roku"]
        .iter()
        .any(|part| name.contains(part))
    {
        "streamer"
    } else {
        "hdmi"
    }
}
/// Detail under an input title: the TV's own label (when connected and not just
/// the port), the port when a custom name replaces it, then the status.
pub fn input_status(input: &Input, renamed: bool, language: Language) -> String {
    if input.id == "airplay" {
        return language.t("airplayInputDetail").into();
    }
    let mut parts = vec![];
    if input.connected == Some(true) && input.title != input.port {
        parts.push(input.title.as_str());
    }
    if renamed {
        parts.push(input.port.as_str());
    }
    parts.push(match input.connected {
        Some(false) => language.t("noDevice"),
        None => language.t("externalInput"),
        Some(true) if input.signal == Some(false) => language.t("noSignal"),
        Some(true) => language.t("connected"),
    });
    parts.join(" · ")
}

struct EmptyState<'a> {
    rect: Rect,
    icon: &'a str,
    title: &'a str,
    hint: &'a str,
    action: Option<Target>,
}
fn empty(out: &mut Scene, state: &State, assets: &Assets, content: EmptyState<'_>) {
    let EmptyState {
        rect,
        icon,
        title,
        hint,
        action,
    } = content;
    out.shape(rect, 17.6, c(0xffffff, 0.02));
    out.border(rect, 17.6, 1., c(0xffffff, 0.05));
    let center = rect.x + rect.w / 2.;
    let top = rect.y
        + if action.is_some() {
            33.
        } else {
            (rect.h - 123.2) / 2.
        };
    out.icon(
        assets,
        icon,
        Rect::new(center - 17.6, top, 35.2, 35.2),
        c(0xd8d8d8, 0.60),
    );
    out.text(
        title,
        Rect::new(rect.x + 32., top + 52., rect.w - 64., 31.),
        text(24., 550, -0.035),
        WHITE,
        true,
    );
    out.text(
        hint,
        Rect::new(center - 352., top + 97.6, 704., 64.),
        text(16., 400, 0.).leading(1.6),
        c(0xf5f5f5, 0.72),
        true,
    );
    if let Some(target) = action {
        let label = state.preferences.language.t(if target == Target::Retry {
            "retry"
        } else {
            "exploreApps"
        });
        let width = assets.width(label, text(15.2, 600, 0.)) + 72.;
        let button = Rect::new(center - width / 2., top + 144.8, width, 46.8);
        out.shape(button, 11.2, c(0xededed, 1.));
        if state.focus == target {
            out.border(button.inset(-7.), 18.2, 2., WHITE);
        }
        out.text(
            label,
            Rect::new(button.x + 20.8, button.y + 14.4, button.w - 72., 18.4),
            text(15.2, 600, 0.),
            c(0x242424, 1.),
            false,
        );
        out.icon(
            assets,
            "arrow",
            Rect::new(button.x + button.w - 38.4, button.y + 14.6, 17.6, 17.6),
            c(0x242424, 1.),
        );
        out.control(target, button, true);
    }
}

fn options(out: &mut Scene, state: &State, assets: &Assets, id: &str, anchor: Option<Rect>) {
    let language = state.preferences.language;
    let shortcut = matches!(
        &state.overlay,
        Some(Overlay::Options {
            restore: Target::Tile(_),
            ..
        })
    );
    let background = matches!(state.overlay, Some(Overlay::BackgroundOptions { .. }));
    let app = state.inventory.apps.iter().find(|app| app.id == id);
    let favorite = state
        .preferences
        .favorites(&state.inventory.apps)
        .iter()
        .any(|item| item == id);
    let deletable = if background {
        state.can_delete_background(id) && state.deleting_wallpaper.is_none()
    } else {
        crate::platform::can_uninstall(id)
    };
    let skippable = background
        && id == crate::aerials::ID
        && state.preferences.background == crate::aerials::ID;
    let height = if shortcut {
        176.
    } else if background {
        if deletable || skippable { 176. } else { 99.2 }
    } else if deletable {
        329.6
    } else {
        252.8
    };
    let card = anchor.unwrap_or(Rect::new(460., 380., 256., 153.6));
    let x = if card.x + card.w + 32. + 520. <= 1836. {
        card.x + card.w + 32.
    } else {
        (card.x - 552.).max(84.)
    };
    let panel = Rect::new(x, card.y.clamp(164., 974. - height), 520., height);
    // Lift the selected card out of the dimmed grid without moving its hit region.
    if background {
        background_card(out, assets, state, id, card);
        out.border(card, 16., 1., c(0xffffff, 0.086));
    } else if let Some(app) = app.filter(|_| !shortcut) {
        let favorite_color = app_face(out, assets, app, card, language, 19.2);
        if favorite {
            favorite_badge(out, assets, card, favorite_color);
        }
    }
    let panel_start = out.draw.len();
    out.modal = Some(panel);
    out.shadow(panel, 27.2, 120., 25., 0.47);
    out.draw.push(Draw::RoundedGradient(
        panel,
        27.2,
        c(0x34383b, 0.96),
        c(0x24282d, 0.988),
    ));
    out.border(panel, 27.2, 1., c(0xffffff, 0.12));
    let hidden = state.preferences.hidden.iter().any(|item| item == id);
    let actions = if shortcut {
        vec![
            (Target::TileTest(id.into()), language.t("testShortcut")),
            (Target::TileRemove(id.into()), language.t("removeTile")),
        ]
    } else if background {
        let mut actions = vec![(Target::UseBackground, language.t("useBackground"))];
        if deletable {
            actions.push((Target::DeleteBackground, language.t("deleteBackground")));
        }
        if skippable {
            actions.push((Target::NextAerial, language.t("nextAerial")));
        }
        actions
    } else {
        vec![
            (Target::OpenApp, language.t("open")),
            (
                Target::Option(0),
                if favorite {
                    language.t("removeFavorite")
                } else {
                    language.t("addFavorite")
                },
            ),
            (
                Target::Option(1),
                language.t(if hidden { "showApp" } else { "hideApp" }),
            ),
            (Target::Option(2), language.t("deleteApp")),
        ]
        .into_iter()
        .take(if deletable { 4 } else { 3 })
        .collect()
    };
    for (row, (target, label)) in actions.into_iter().enumerate() {
        let rect = Rect::new(
            panel.x + 16.,
            panel.y + 16. + row as f32 * 76.8,
            panel.w - 32.,
            67.2,
        );
        let focused = state.focus == target;
        let destructive = matches!(
            target,
            Target::Option(2) | Target::DeleteBackground | Target::TileRemove(_)
        );
        let label_style = text(20.8, 600, -0.02);
        let label_height = label_style.size * label_style.line_height;
        let label_color = if destructive || !focused {
            WHITE
        } else {
            c(0x242424, 1.)
        };
        out.shape(
            rect,
            12.,
            if destructive {
                c(0xd63b35, if focused { 1. } else { 0.25 })
            } else {
                c(0xffffff, if focused { 0.96 } else { 0.10 })
            },
        );
        out.text(
            label,
            Rect::new(
                rect.x + 24.,
                rect.y + (rect.h - label_height) / 2.,
                rect.w - 88.,
                label_height,
            ),
            label_style,
            label_color,
            false,
        );
        out.icon(
            assets,
            match &target {
                Target::OpenApp | Target::TileTest(_) => "arrow",
                Target::UseBackground => "check",
                Target::NextAerial => "right",
                Target::Option(0) => {
                    if favorite {
                        "heart-filled"
                    } else {
                        "heart"
                    }
                }
                Target::Option(1) => {
                    if hidden {
                        "eye"
                    } else {
                        "eye-off"
                    }
                }
                _ => "circle-minus",
            },
            Rect::new(rect.x + rect.w - 45.6, rect.y + 22.8, 21.6, 21.6),
            label_color,
        );
        out.control(target, rect, false);
    }
    if let Some(opened) = state.panel_started {
        out.fade(
            panel_start,
            (opened.elapsed().as_secs_f32() / 0.12).min(1.),
            1.,
        );
    }
}

fn delete_confirmation(out: &mut Scene, state: &State, assets: &Assets, id: &str) {
    let language = state.preferences.language;
    let background = matches!(state.overlay, Some(Overlay::BackgroundDelete { .. }));
    let deleting = if background {
        state.deleting_wallpaper.as_deref() == Some(id)
    } else {
        state.uninstalling.as_deref() == Some(id)
    };
    let warning_key = if background {
        "deleteBackgroundWarning"
    } else {
        "deleteAppWarning"
    };
    let deleting_key = if background {
        "deletingBackground"
    } else {
        "deletingApp"
    };
    let delete_key = if background {
        "deleteBackground"
    } else {
        "deleteApp"
    };
    let title = state
        .inventory
        .apps
        .iter()
        .find(|app| app.id == id)
        .map_or(id, |app| app.title.as_str());
    let body_style = text(20.8, 600, -0.02).leading(1.5);
    let body = language.format(
        if deleting { deleting_key } else { warning_key },
        &[("app", title)],
    );
    // Status changes must not move the confirmation buttons under the pointer.
    let warning = language.format(warning_key, &[("app", title)]);
    let body_height = assets
        .lines(&warning, body_style, 568.)
        .max(assets.lines(language.t(deleting_key), body_style, 568.))
        .max(1) as f32
        * 31.2;
    let height = 244. + body_height;
    let panel = Rect::new(640., (1080. - height) / 2., 640., height);
    out.modal = Some(panel);
    out.shadow(panel, 28., 100., 24., 0.47);
    out.draw.push(Draw::RoundedGradient(
        panel,
        28.,
        c(0x34383b, 0.98),
        c(0x24282d, 0.99),
    ));
    out.border(panel, 28., 1., c(0xffffff, 0.12));
    out.text(
        body,
        Rect::new(panel.x + 36., panel.y + 36., 568., body_height),
        body_style,
        c(0xf5f5f5, 0.75),
        true,
    );
    for (index, (target, label)) in [
        (Target::ConfirmDelete, delete_key),
        (Target::CancelDelete, "cancel"),
    ]
    .into_iter()
    .enumerate()
    {
        let rect = Rect::new(
            panel.x + 28.,
            panel.y + panel.h - 184. + index as f32 * 80.,
            panel.w - 56.,
            68.,
        );
        let focused = state.focus == target;
        let disabled = target == Target::ConfirmDelete && deleting;
        let label_style = text(20.8, 600, -0.02);
        let label_height = label_style.size * label_style.line_height;
        out.shape(
            rect,
            14.,
            if index == 0 {
                c(
                    0xd63b35,
                    if disabled {
                        0.15
                    } else if focused {
                        1.
                    } else {
                        0.25
                    },
                )
            } else {
                c(0xffffff, if focused { 0.96 } else { 0.10 })
            },
        );
        out.text(
            language.t(label),
            Rect::new(
                rect.x + 20.,
                rect.y + (rect.h - label_height) / 2.,
                rect.w - 40.,
                label_height,
            ),
            label_style,
            if index == 1 && focused {
                c(0x242424, 1.)
            } else {
                WHITE
            },
            true,
        );
        if !disabled {
            out.control(target, rect, false);
        }
    }
}

fn keyboard(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let panel = Rect::new(350., 290., 1220., 530.);
    out.modal = Some(panel);
    out.shadow(panel, 27.2, 90., 20., 0.45);
    out.draw.push(Draw::RoundedGradient(
        panel,
        27.2,
        c(0x34383b, 0.98),
        c(0x24282d, 1.),
    ));
    out.border(panel, 27.2, 1., c(0xffffff, 0.12));
    out.text(
        match state.keyboard_field {
            TextField::Location => language.t("cityPostcode"),
            TextField::Name => language.t("greetingName"),
            TextField::InputName => language.t("inputName"),
            TextField::Apps => language.t("findApp"),
            TextField::Coin => language.t("cryptoFind"),
        },
        Rect::new(389., 324., 1000., 44.),
        text(37.6, 500, -0.055).leading(1.1),
        WHITE,
        false,
    );
    let close = Rect::new(1497., 328., 33.6, 33.6);
    out.icon(assets, "close", close.inset(7.6), WHITE);
    out.control(Target::CloseDialog, close, false);
    if state.focus == Target::CloseDialog {
        out.border(close, 16.8, 2., WHITE);
    }
    out.shape(Rect::new(389., 390., 1142., 56.), 28., c(0x171c20, 0.72));
    out.icon(
        assets,
        "search",
        Rect::new(409., 408., 20., 20.),
        c(0xeeeeee, 0.73),
    );
    out.text(
        format!("{}|", state.editing_text()),
        Rect::new(446., 406., 1058., 25.),
        text(20.8, 400, -0.02),
        WHITE,
        false,
    );
    for (index, key) in KEYS.iter().enumerate() {
        let label = match *key {
            "Space" => language.t("space"),
            "Delete" => language.t("delete"),
            "Clear" => language.t("clear"),
            "Done" => language.t("done"),
            _ => key,
        };
        let rect = Rect::new(
            389. + (index % 10) as f32 * 115.6,
            474. + (index / 10) as f32 * 74.,
            101.6,
            60.,
        );
        let focused = state.focus == Target::Key(index);
        out.pill(rect, focused, false);
        let mut style = text(18.4, 550, -0.02);
        let width = assets.width(label, style);
        if width > rect.w - 12. {
            style.size *= (rect.w - 12.) / width;
        }
        out.text(
            label,
            Rect::new(rect.x, rect.y + 18., rect.w, 25.),
            style,
            if focused { c(0x2c2c2c, 1.) } else { WHITE },
            true,
        );
        out.control(Target::Key(index), rect, false);
    }
}

fn toast(out: &mut Scene, state: &State, assets: &Assets, notice: &str) {
    let start = out.draw.len();
    let notice = notice.trim_end_matches(['.', '。']);
    let style = text(20.8, 550, -0.02);
    let width = (assets.width(notice, style).ceil() + 42.).clamp(42., 1856.);
    let text_height =
        assets.lines(notice, style, width - 40.) as f32 * style.size * style.line_height;
    let height = 20. + text_height;
    let rect = Rect::new(960. - width / 2., 1080. - 51.2 - height, width, height);
    out.shadow(rect, 16., 18., 4., 0.06);
    out.shape(rect, 16., c(0x171c20, 0.72));
    out.border(rect, 16., 1., c(0xffffff, 0.07));
    out.text(
        notice,
        Rect::new(rect.x + 20., rect.y + 10., width - 40., text_height),
        style,
        WHITE,
        true,
    );
    if state.focus == Target::DismissNotice {
        out.border(rect, 16., 2., WHITE);
    }
    if state.overlay.is_none() {
        out.control(Target::DismissNotice, rect, false);
    }
    if !state.preferences.reduced_motion
        && let Some((_, opened)) = &state.notice
    {
        out.fade(start, (opened.elapsed().as_secs_f32() / 0.12).min(1.), 1.);
    }
}

/// A corner-less, borderless fill of the whole logical screen.
fn full_screen_fill(draw: &Draw) -> bool {
    let whole = |r: &Rect| r.x <= 0. && r.y <= 0. && r.x + r.w >= 1920. && r.y + r.h >= 1080.;
    match draw {
        Draw::Shape(rect, radius, border, _) => whole(rect) && *radius <= 0. && *border <= 0.,
        Draw::RoundedGradient(rect, radius, ..) => whole(rect) && *radius <= 0.,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Preferences, demo_inventory};

    fn assets() -> Assets {
        Assets {
            root: PathBuf::new(),
            version: "test".into(),
            metrics: BTreeMap::from([
                ("display-500".into(), BTreeMap::from([('W', 1.)])),
                ("500".into(), BTreeMap::from([('W', 1.2)])),
                (
                    "fallback-500".into(),
                    BTreeMap::from([('界', 1.), ('한', 1.)]),
                ),
            ]),
            artwork: crate::app_artwork::Service::new(false),
        }
    }

    #[test]
    fn settings_breadcrumbs_use_equal_gaps_and_only_highlight_the_leaf() {
        let assets = assets();
        let mut state = State::new(Preferences::default());
        state.open(2);
        state.section = 2;
        state.subpage = Some(Subpage::DateFormat);
        for language in Language::ALL {
            state.preferences.language = language;
            for focused in [false, true] {
                state.focus = if focused {
                    Target::Sidebar(2)
                } else {
                    Target::Setting(0)
                };
                let mut scene = Scene::new();
                settings_sidebar(&mut scene, &state, &assets);
                let parts: Vec<_> = scene
                    .draw
                    .iter()
                    .filter_map(|draw| match draw {
                        Draw::Text(label, rect, style, color, _)
                            if (rect.y - 356.3).abs() < 0.001 =>
                        {
                            Some((label, rect, style, color))
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(parts.len(), 3);
                assert_eq!(parts[0].0, language.t("widgets"));
                assert_eq!(parts[1].0, language.t("date"));
                assert_eq!(parts[2].0, language.t("dateFormat"));
                assert_eq!(parts[0].3, parts[1].3);
                assert!(parts[2].3[3] > parts[1].3[3]);
                let chevrons: Vec<_> = scene
                    .draw
                    .iter()
                    .filter_map(|draw| match draw {
                        Draw::TintedImage(_, rect, _, _, color)
                            if (rect.y - 359.8).abs() < 0.001 =>
                        {
                            assert_eq!(color, parts[0].3);
                            Some(rect)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(chevrons.len(), 2);
                for (index, chevron) in chevrons.iter().enumerate() {
                    let (label, rect, style, _) = parts[index];
                    let left_gap = chevron.x - rect.x - assets.width(label, *style);
                    let right_gap = parts[index + 1].1.x - chevron.x - chevron.w;
                    assert!((left_gap - 6.).abs() < 0.001);
                    assert!((right_gap - 6.).abs() < 0.001);
                }
            }
        }
    }

    #[test]
    fn upload_button_padding_tracks_the_label_in_every_language() {
        let assets = assets();
        let mut state = State::new(Preferences::default());
        state.open(2);
        state.section = 1;
        for language in Language::ALL {
            state.preferences.language = language;
            let label = language.t("uploadWallpaper");
            let label_width = assets.width(label, text(20.8, 550, -0.02));
            let mut scene = Scene::new();
            settings(&mut scene, &state, &assets);
            let button = scene
                .controls
                .iter()
                .find(|control| control.target == Target::UploadWallpaper)
                .unwrap()
                .rect;
            let title = scene
                .draw
                .iter()
                .find_map(|draw| match draw {
                    Draw::Text(value, rect, ..) if value == label => Some(rect),
                    _ => None,
                })
                .unwrap();
            let right_padding = button.x + button.w - title.x - label_width;
            assert!((24.2..25.6).contains(&right_padding));
        }
    }

    #[test]
    fn auto_rotation_keeps_a_compact_gap_and_right_aligned_state() {
        let assets = assets();
        let mut state = State::new(Preferences::default());
        state.open(2);
        state.section = 1;
        let label = state.preferences.language.t("wallpaperCycle");
        let label_width = assets.width(label, text(20.8, 550, -0.02));
        for enabled in [false, true] {
            state.preferences.wallpaper_cycle = enabled;
            let mut scene = Scene::new();
            settings(&mut scene, &state, &assets);
            let button = scene
                .controls
                .iter()
                .find(|control| control.target == Target::WallpaperCycle)
                .unwrap()
                .rect;
            let title = scene
                .draw
                .iter()
                .find_map(|draw| match draw {
                    Draw::Text(value, rect, ..) if value == label => Some(rect),
                    _ => None,
                })
                .unwrap();
            let state_label =
                state
                    .preferences
                    .language
                    .t(if enabled { "stateOn" } else { "stateOff" });
            let (value, style) = scene
                .draw
                .iter()
                .find_map(|draw| match draw {
                    Draw::Text(label, rect, style, ..) if label == state_label => {
                        Some((rect, style))
                    }
                    _ => None,
                })
                .unwrap();
            assert!(style.align_right);
            assert!((button.x + button.w - value.x - value.w - 24.2).abs() < 0.001);
            assert!((24. ..25.).contains(&(value.x - title.x - label_width)));
            assert!(button.w < 320.);
        }
    }

    #[test]
    fn short_greetings_keep_their_original_style_and_geometry() {
        let assets = assets();
        for name in ["", "Ana"] {
            let state = State::new(Preferences {
                greeting_name: name.into(),
                show_weather: false,
                ..Preferences::default()
            });
            let mut scene = Scene::new();
            home(&mut scene, &state, &assets, OffsetDateTime::UNIX_EPOCH);
            let greeting = state.preferences.language.greeting(0, name);
            let (rect, style) = scene
                .draw
                .iter()
                .find_map(|draw| match draw {
                    Draw::Text(value, rect, style, ..) if value == &greeting => Some((rect, style)),
                    _ => None,
                })
                .unwrap();
            // Date and greeting, centred in the header's 150–400.
            let block = 22. * 1.2 + 14. + 80. + 16.;
            let top = 150. + (250. - block) / 2.;
            assert_eq!((rect.x, rect.w, rect.h), (80., 1760., 80.));
            assert!((rect.y - (top + 22. * 1.2 + 14.)).abs() < 0.001, "{rect:?}");
            assert_eq!(style.size, 76.);
            assert_eq!(style.weight, 600);
            assert_eq!(style.tracking, -0.04);
        }
    }

    #[test]
    fn long_localized_greetings_fit_on_one_line_without_changing_the_name() {
        let assets = assets();
        let base = text(90.4, 500, -0.065).leading(1.05);
        for language in Language::ALL {
            for hour in [2, 9, 15, 20] {
                for name in [
                    "W".repeat(40),
                    "界".repeat(40),
                    "한".repeat(40),
                    "Alexandra Catherine Elizabeth Montgomery".into(),
                ] {
                    let greeting = language.greeting(hour, &name);
                    let fitted = assets.fit_text(&greeting, base, 1500.);
                    assert!(fitted.size > 0. && fitted.size < base.size);
                    assert!(assets.width(&greeting, fitted) < 1500.);
                    assert_eq!(assets.lines(&greeting, fitted, 1500.), 1);
                    assert!(greeting.contains(&name));
                }
            }
        }
    }

    #[test]
    fn fitting_remeasures_when_shrinking_changes_the_optical_font() {
        let assets = assets();
        let greeting = "W".repeat(100);
        let fitted = assets.fit_text(&greeting, text(90.4, 500, -0.065), 1500.);
        assert!(fitted.size < 32.);
        assert!(assets.width(&greeting, fitted) < 1500.);
        assert_eq!(assets.lines(&greeting, fitted, 1500.), 1);
    }

    #[test]
    fn shrinking_a_greeting_preserves_weather_favorites_and_control_positions() {
        let assets = assets();
        for show_weather in [false, true] {
            let mut state = State::new(Preferences {
                greeting_name: "Ana".into(),
                show_weather,
                ..Preferences::default()
            });
            state.set_inventory(demo_inventory(), None);
            let mut original = Scene::new();
            home(&mut original, &state, &assets, OffsetDateTime::UNIX_EPOCH);
            state.preferences.greeting_name = "W".repeat(40);
            let mut fitted = Scene::new();
            home(&mut fitted, &state, &assets, OffsetDateTime::UNIX_EPOCH);
            for (before, after) in original.draw.iter().zip(&fitted.draw) {
                if let (
                    Draw::Text(old, rect, old_style, ..),
                    Draw::Text(new, fitted_rect, style, ..),
                ) = (before, after)
                {
                    if old == new {
                        assert_eq!(
                            (rect.x, rect.y, rect.w, rect.h),
                            (fitted_rect.x, fitted_rect.y, fitted_rect.w, fitted_rect.h)
                        );
                        assert_eq!(old_style.size, style.size);
                    } else {
                        assert_eq!(
                            new,
                            &state
                                .preferences
                                .language
                                .greeting(0, &state.preferences.greeting_name)
                        );
                        assert!(
                            (rect.y + rect.h / 2. - fitted_rect.y - fitted_rect.h / 2.).abs()
                                < 0.001
                        );
                        assert!(style.size * style.line_height <= fitted_rect.h);
                    }
                }
            }
            assert_eq!(original.controls.len(), fitted.controls.len());
            for (before, after) in original.controls.iter().zip(&fitted.controls) {
                assert_eq!(before.target, after.target);
                assert_eq!(
                    (before.rect.x, before.rect.y, before.rect.w, before.rect.h),
                    (after.rect.x, after.rect.y, after.rect.w, after.rect.h)
                );
            }
        }
    }

    #[test]
    fn hidden_home_widgets_collapse_without_moving_favorites_or_inputs() {
        let assets = assets();
        let mut state = State::new(Preferences::default());
        state.set_inventory(demo_inventory(), None);
        let mut original = Scene::new();
        home(&mut original, &state, &assets, OffsetDateTime::UNIX_EPOCH);
        let weather_y = |scene: &Scene| {
            scene
                .controls
                .iter()
                .find(|control| control.target == Target::WeatherConfigure)
                .unwrap()
                .rect
                .y
        };
        let original_y = weather_y(&original);
        for show_date in [false, true] {
            for show_greeting in [false, true] {
                state.preferences.show_date = show_date;
                state.preferences.show_greeting = show_greeting;
                let mut rendered = Scene::new();
                home(&mut rendered, &state, &assets, OffsetDateTime::UNIX_EPOCH);
                let removed = if show_date { 0. } else { 22. * 1.2 + 14. }
                    + if show_greeting { 0. } else { 80. + 16. };
                // The header keeps its height and centres what is left in it.
                assert!((weather_y(&rendered) - (original_y - removed / 2.)).abs() < 0.001);
                for control in original
                    .controls
                    .iter()
                    .filter(|control| matches!(control.target, Target::App(_) | Target::Input(_)))
                {
                    let other = rendered
                        .controls
                        .iter()
                        .find(|other| other.target == control.target)
                        .unwrap();
                    assert_eq!(
                        (control.rect.x, control.rect.y),
                        (other.rect.x, other.rect.y)
                    );
                }
                if show_date && !show_greeting {
                    let date_y = rendered
                        .draw
                        .iter()
                        .find_map(|draw| match draw {
                            Draw::Text(label, rect, ..)
                                if label
                                    == &state
                                        .preferences
                                        .language
                                        .date(OffsetDateTime::UNIX_EPOCH) =>
                            {
                                Some(rect.y)
                            }
                            _ => None,
                        })
                        .unwrap();
                    // Weather's control includes padding around the summary itself.
                    assert!((weather_y(&rendered) - date_y).abs() < 60.);
                }
            }
        }
    }
}
