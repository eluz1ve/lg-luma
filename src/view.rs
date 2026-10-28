//! Screen geometry and styling mirror the web UI's 1920×1080 / 16px-rem layout.
use crate::{
    design,
    home_grid::BlockKind,
    i18n::Language,
    model::{App, BACKGROUNDS, CATEGORIES, Input, InputMode, UiSize},
    renderer::{Color, Draw, TextStyle, WHITE},
    state::{
        APPS_COLUMNS, BACKGROUND_SECTION, Control, HOME_TILE_PITCH, HOME_TILE_WIDTH, KEYS,
        Overlay, Page, Rect, SYSTEM_SECTION, State, Subpage, Target, TextField, WIDGETS_SECTION,
    },
    weather::{self, Unit},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use time::OffsetDateTime;

#[path = "widget-view.rs"]
mod widget_view;

const MAIN_TOP: f32 = 108.8;
const FACE_HEIGHT: f32 = 154.4;
const GRID_PITCH: f32 = 296.4;
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
        let version = String::from_utf8(load("version.txt")?).map_err(|e| e.to_string())?;
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

    fn background(&self, state: &State, id: &str) -> String {
        if let Some(item) = state.wallpapers.iter().find(|item| item.id == id) {
            return item.image();
        }
        let id = if BACKGROUNDS.iter().any(|(known, _)| *known == id) {
            id
        } else {
            "dunes"
        };
        self.path(format!("wallpapers/{id}.jpg"))
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
    /// Focus rings; Home uses its theme's accent.
    pub accent: Color,
}
impl Scene {
    fn new() -> Self {
        Self {
            draw: Vec::new(),
            controls: Vec::new(),
            modal: None,
            accent: c(0xf0f0f0, 1.),
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
    fn control(&mut self, target: Target, rect: Rect, scrollable: bool) {
        self.controls.push(Control {
            target,
            rect,
            scrollable,
        });
    }
    fn ring(&mut self, rect: Rect, radius: f32) {
        self.border(rect.inset(-6.9), radius + 6.9, 3.7, c(0xffffff, 0.10));
        self.border(rect.inset(-3.2), radius + 3.2, 3.2, self.accent);
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
                | Draw::AppImage(_, _, color)
                | Draw::Picture(_, _, _, color) => apply(color),
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

fn c(rgb: u32, alpha: f32) -> Color {
    [
        ((rgb >> 16) & 255) as f32 / 255.,
        ((rgb >> 8) & 255) as f32 / 255.,
        (rgb & 255) as f32 / 255.,
        alpha,
    ]
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
    if state.page == Page::Home {
        out.accent = c(
            crate::themes::theme(&state.preferences.home_theme).accent,
            1.,
        );
    }
    if let Some((previous, start)) = &state.previous_background {
        out.image(
            assets.background(state, previous),
            Rect::new(0., 0., 1920., 1080.),
            0.,
            false,
        );
        let alpha = (start.elapsed().as_secs_f32() / state.background_fade).min(1.);
        out.draw.push(Draw::TintedImage(
            assets.background(state, &state.preferences.background),
            Rect::new(0., 0., 1920., 1080.),
            0.,
            false,
            [1., 1., 1., alpha],
        ));
    } else {
        out.image(
            assets.background(state, &state.preferences.background),
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
    out.draw.push(Draw::Clip(Some(viewport)));
    if state.page == Page::Apps {
        out.draw
            .push(Draw::EdgeFade(Some([MAIN_TOP, 152.8, 1080., 1104.])));
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
    footer(&mut out, state, assets);
    if let Some(
        overlay @ (Overlay::Options { .. }
        | Overlay::Delete { .. }
        | Overlay::BackgroundOptions { .. }
        | Overlay::BackgroundDelete { .. }
        | Overlay::Keyboard
        | Overlay::WallpaperUpload
        | Overlay::TileEditor
        | Overlay::Block { .. }
        | Overlay::AddBlock { .. }
        | Overlay::Poster { .. }
        | Overlay::PhoneKey { .. }
        | Overlay::WidgetDialog { .. }
        | Overlay::Store { .. }
        | Overlay::StoreEntry { .. }),
    ) = &state.overlay
    {
        let language = state.preferences.language;
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
                    let names = if state.page == Page::Home {
                        state.preferences.show_home_names
                    } else {
                        state.preferences.show_app_names
                    };
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
            Overlay::Block { kind, restore } => {
                let rows: Vec<String> = state
                    .block_actions(*kind, matches!(restore, Target::App(_)))
                    .into_iter()
                    .map(|action| match (action, restore) {
                        (crate::state::BlockAction::Reorder, Target::App(id)) => {
                            let title = state
                                .inventory
                                .apps
                                .iter()
                                .find(|app| &app.id == id)
                                .map_or(id.as_str(), |app| app.title.as_str());
                            language.format("blockReorderApp", &[("app", title)])
                        }
                        (action, _) => language.t(action.key()).to_owned(),
                    })
                    .collect();
                menu(
                    &mut out,
                    state,
                    assets,
                    &state.block_name(*kind),
                    &rows,
                    Target::BlockAction,
                );
            }
            Overlay::AddBlock { .. } => {
                let rows: Vec<String> = state
                    .add_choices()
                    .into_iter()
                    .map(|choice| match choice {
                        crate::state::AddChoice::Kind(kind) => state.block_name(kind),
                        crate::state::AddChoice::Widget(id) => state
                            .widgets
                            .store
                            .manifest(&id)
                            .map_or(id, |m| m.name.get(language.code()).to_owned()),
                        crate::state::AddChoice::Store => language.t("widgetStore").to_owned(),
                    })
                    .collect();
                menu(
                    &mut out,
                    state,
                    assets,
                    language.t("addWidget"),
                    &rows,
                    Target::AddKind,
                );
            }
            Overlay::PhoneKey { .. } => phone_key_dialog(&mut out, state, assets),
            Overlay::WidgetDialog { slot, id, .. } => {
                widget_view::dialog(&mut out, state, assets, *slot, id, now)
            }
            Overlay::Store { .. } => widget_view::store(&mut out, state, assets),
            Overlay::StoreEntry { id, .. } => widget_view::store_entry(&mut out, state, assets, id),
            Overlay::Poster { index, .. } => {
                if let Some(premiere) = state.feeds.premieres.get(*index) {
                    let mut rows: Vec<String> = state
                        .poster_apps()
                        .iter()
                        .map(|id| {
                            let title = state
                                .inventory
                                .apps
                                .iter()
                                .find(|app| &app.id == id)
                                .map_or(id.as_str(), |app| app.title.as_str());
                            language.format("posterOpenIn", &[("app", title)])
                        })
                        .collect();
                    rows.push(language.t("close").to_owned());
                    let title = format!(
                        "{} · {}",
                        premiere.title,
                        language.format(
                            "seasonPremiere",
                            &[
                                ("season", &premiere.season.to_string()),
                                ("date", &short_date(&premiere.date, now, language)),
                            ],
                        )
                    );
                    menu(&mut out, state, assets, &title, &rows, Target::PosterApp);
                }
            }
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
        Rect::new(84., 76.8 - 12. * k, 300. * k, 24. * k),
        text(20. * k, 500, -0.03),
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
    let rect = Rect::new(1786.4, 52., 49.6, 49.6);
    let focused = state.focus == Target::TvSettings;
    out.pill(rect, focused, false);
    out.icon(
        assets,
        "settings",
        Rect::new(1799.6, 65.2, 23.2, 23.2),
        if focused {
            c(0x2c2c2c, 1.)
        } else {
            c(0xeeeeee, 0.73)
        },
    );
    out.control(Target::TvSettings, rect, false);
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
    let viewport = if state.page == Page::Home {
        state.favorites_viewport()
    } else {
        state.content_viewport()
    };
    if rect.y + rect.h + label_height < MAIN_TOP
        || rect.y > 1054.
        || rect.x + rect.w + 40. < viewport.x
        || rect.x - 40. > viewport.x + viewport.w
    {
        return;
    }
    let start = out.draw.len();
    let focused = state.focus == Target::App(app.id.clone());
    out.shadow(
        rect,
        19.2,
        if focused { 34. } else { 22. },
        if focused { 14. } else { 9. },
        if focused { 0.4 } else { 0.165 },
    );
    app_face(out, assets, app, rect, state.preferences.language, 19.2);
    if pinned {
        out.icon(
            assets,
            "heart-filled",
            Rect::new(rect.x + rect.w - 27.2, rect.y + 11.2, 16., 16.),
            c(0xffffff, 0.9),
        );
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
    if focused {
        out.ring(rect, 19.2);
    }
    if state.page == Page::Apps && state.filter == 5 {
        out.fade(start, 0.45, 1.);
    }
    if state.pressed.as_ref() == Some(&Target::App(app.id.clone())) {
        out.fade(start, 1., 0.85);
    }
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
                17.6 * caption_scale(state),
                if focused { 550 } else { 450 },
                -0.02,
            )
            .leading(1.4),
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

/// Shared card artwork for Home, Apps and Recents. Curated vectors always take
/// precedence over installed artwork, with the same relative logo proportions.
fn app_face(
    out: &mut Scene,
    assets: &Assets,
    app: &App,
    rect: Rect,
    language: Language,
    radius: f32,
) {
    let logo = crate::branding::for_key(&app.brand);
    let artwork = logo
        .is_none()
        .then(|| {
            app.icon
                .as_ref()
                .and_then(|source| assets.artwork.get(source))
        })
        .flatten();
    if let Some([top, bottom]) = artwork.as_ref().map(|image| image.backdrop) {
        let color = |rgb: [u8; 3]| rgb.map(|c| c as f32 / 255.);
        let [top, bottom] = [color(top), color(bottom)];
        if top == bottom {
            out.shape(rect, radius, [top[0], top[1], top[2], 1.]);
        } else {
            out.draw.push(Draw::RoundedGradient(
                rect,
                radius,
                [top[0], top[1], top[2], 1.],
                [bottom[0], bottom[1], bottom[2], 1.],
            ));
        }
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
    } else if let Some(image) = artwork {
        out.draw.push(Draw::AppImage(
            image,
            Rect::new(
                rect.x + rect.w * 0.16,
                rect.y + rect.h * 0.16,
                rect.w * 0.68,
                rect.h * 0.68,
            ),
            [1.; 4],
        ));
    } else {
        brand_mark(out, assets, app, rect, language);
    }
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
        17.6 * caption_scale(state),
        if state.focus == Target::App(app.id.clone()) {
            550
        } else {
            450
        },
        -0.02,
    )
    .leading(1.4);
    (assets.lines(&title, style, width - 8.) as f32 * style.size * style.line_height)
        .max(49.28 * caption_scale(state))
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

fn home(out: &mut Scene, state: &State, assets: &Assets, now: OffsetDateTime) {
    let s = state.scroll;
    let blocks = state.home_blocks();
    let theme = crate::themes::theme(&state.preferences.home_theme);
    let editing = state.home_edit.as_ref();
    if editing.is_some() {
        // The grid's cells, so a moved or resized block can be lined up.
        for row in 0..crate::home_grid::bottom(&blocks) + 1 {
            for column in 0..crate::home_grid::COLUMNS {
                let cell = crate::home_grid::rect(&crate::home_grid::Block {
                    kind: BlockKind::Greeting,
                    x: column,
                    y: row,
                    w: 1,
                    h: 1,
                });
                // Clear of the hints at the bottom of the screen.
                if cell.y - s + cell.h <= 1010. {
                    out.shape(
                        Rect::new(cell.x, cell.y - s, cell.w, cell.h),
                        10.,
                        c(0xffffff, 0.045),
                    );
                }
            }
        }
    }
    for block in &blocks {
        let cell = crate::home_grid::rect(block);
        let rect = Rect::new(cell.x, cell.y - s, cell.w, cell.h);
        let controls = out.controls.len();
        match block.kind {
            BlockKind::Greeting => greeting_block(out, state, assets, now, rect),
            BlockKind::Weather => weather_block(out, state, assets, theme, rect),
            BlockKind::Favorites => favorites_block(out, state, assets, rect),
            BlockKind::Inputs => inputs_block(out, state, assets, theme, block, rect),
            BlockKind::Crypto => crypto_block(out, state, assets, theme, rect),
            BlockKind::Premieres => premieres_block(out, state, assets, theme, block, rect, now),
            BlockKind::Widget(slot) => {
                widget_view::row(out, state, assets, theme, slot, block, rect, now)
            }
        }
        // Blocks without controls of their own are still focusable, to edit them.
        if out.controls.len() == controls {
            let target = Target::Block(block.kind);
            if state.focus == target && !state.pointer && editing.is_none() {
                out.ring(rect, theme.radius);
            }
            out.control(target, rect, true);
        }
        if let Some(edit) = editing {
            let active = edit.kind == block.kind;
            let color = if active {
                out.accent
            } else {
                c(0xffffff, 0.35)
            };
            out.border(
                rect.inset(-4.),
                theme.radius + 4.,
                if active { 4. } else { 1.5 },
                color,
            );
            let label = format!("{} · {}×{}", state.block_name(block.kind), block.w, block.h);
            let style = text(16., 500, 0.);
            let width = assets.width(&label, style) + 24.;
            let tag = Rect::new(rect.x + rect.w - width - 8., rect.y + 8., width, 30.);
            out.shape(tag, 9., c(0x080b12, if active { 0.85 } else { 0.6 }));
            out.text(
                label,
                Rect::new(tag.x + 12., tag.y + 6., width, 20.),
                style,
                c(0xffffff, if active { 0.95 } else { 0.7 }),
                false,
            );
        }
    }
    let missing = BlockKind::ALL
        .iter()
        .any(|kind| !blocks.iter().any(|b| b.kind == *kind));
    if missing && editing.is_none() {
        let cell = crate::home_grid::rect(&crate::home_grid::Block {
            kind: BlockKind::Greeting,
            x: 0,
            y: crate::home_grid::bottom(&blocks),
            w: 4,
            h: 1,
        });
        let rect = Rect::new(cell.x, cell.y - s, cell.w, cell.h);
        let focused = state.focus == Target::AddBlock && !state.pointer;
        out.shape(
            rect,
            theme.radius,
            c(0xffffff, if focused { 0.10 } else { 0.03 }),
        );
        out.border(rect, theme.radius, 1.5, c(0xffffff, 0.30));
        out.text(
            format!("+  {}", state.preferences.language.t("addWidget")),
            Rect::new(rect.x, rect.y + 37., rect.w, 26.),
            text(21., 500, -0.02),
            c(0xf0f0f0, 0.85),
            true,
        );
        if focused {
            out.ring(rect, theme.radius);
        }
        out.control(Target::AddBlock, rect, true);
    }
}

/// The date and the greeting, centred in the block; the greeting shrinks to fit its width.
fn greeting_block(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    now: OffsetDateTime,
    rect: Rect,
) {
    let language = state.preferences.language;
    let greeting = language.greeting(now.hour(), &state.preferences.greeting_name);
    let compact = rect.h < 150.;
    let mut style = text(
        if compact {
            46.
        } else if rect.h < 300. {
            90.4
        } else {
            124.
        },
        500,
        -0.065,
    )
    .leading(1.05);
    let width = assets.width(&greeting, style);
    if width > rect.w {
        style.size *= rect.w / width;
    }
    let date_style = text(if compact { 12.8 } else { 14.4 }, 600, 0.12);
    let gap = if compact { 6. } else { 20.8 };
    let height = 17. + gap + style.size * 1.05;
    // Taller greetings sit at the bottom of their block, close to what follows.
    let top = if compact {
        rect.y + (rect.h - height) / 2.
    } else {
        rect.y + rect.h - height - 8.
    };
    out.text(
        language.date(now),
        Rect::new(rect.x, top, rect.w, 17.),
        date_style,
        c(0xf5f5f5, 0.72),
        false,
    );
    out.text(
        &greeting,
        Rect::new(
            rect.x,
            top + 17. + gap,
            rect.w + 24.,
            style.size * 1.05 + 4.,
        ),
        style,
        WHITE,
        false,
    );
}

/// The card behind a glance widget (weather, crypto) in themes with cards, in the theme's
/// surface; see docs/design.md.
fn widget_card(out: &mut Scene, theme: &crate::themes::Theme, rect: Rect) {
    out.shape(rect, design::RADIUS, c(theme.surface, theme.surface_alpha));
    out.border(rect, design::RADIUS, 1., c(0xffffff, 0.07));
}

/// A row's header (docs/design.md): its title at the block's top-left and, at its right, meta
/// such as how fresh the data is or which cards show.
fn row_header(out: &mut Scene, assets: &Assets, rect: Rect, title: &str, meta: &[String]) {
    let mut right = rect.x + rect.w;
    for part in meta.iter().rev() {
        let style = design::meta();
        let w = assets.width(part, style) + 4.;
        out.text(
            part,
            Rect::new(right - w, rect.y + 7., w, 20.),
            style,
            design::ink(0.55),
            false,
        );
        right -= w + 20.;
    }
    let style = design::section();
    let w = right - rect.x - 12.;
    out.text(
        fit(assets, title, style, w),
        Rect::new(rect.x, rect.y, w, 31.),
        style,
        WHITE,
        false,
    );
}

/// Every card in a row (docs/design.md): a soft shadow, the theme's surface with a highlight
/// at the top and a hairline edge. Focus brightens it and rings it; `edge` colours the hairline
/// for a state such as live.
fn card(
    out: &mut Scene,
    theme: &crate::themes::Theme,
    rect: Rect,
    focused: bool,
    edge: Option<Color>,
) {
    let radius = design::RADIUS;
    out.shadow(
        rect,
        radius,
        if focused { 34. } else { 22. },
        if focused { 14. } else { 9. },
        if focused { 0.4 } else { 0.16 },
    );
    out.shape(
        rect,
        radius,
        c(theme.surface, (theme.surface_alpha + 0.08).min(0.92)),
    );
    out.draw.push(Draw::RoundedGradient(
        rect,
        radius,
        c(0xffffff, if focused { 0.16 } else { 0.07 }),
        c(0xffffff, if focused { 0.07 } else { 0.015 }),
    ));
    out.border(
        rect,
        radius,
        1.,
        edge.unwrap_or(c(0xffffff, if focused { 0.24 } else { 0.08 })),
    );
    if focused {
        out.ring(rect, radius);
    }
}

/// A row with nothing to show yet: one ghost card with the message, where its cards go.
fn ghost(out: &mut Scene, assets: &Assets, rect: Rect, message: &str) {
    out.shape(rect, design::RADIUS, c(0xffffff, 0.04));
    out.border(rect, design::RADIUS, 1., c(0xffffff, 0.1));
    let style = design::body().leading(1.4);
    out.text(
        message,
        Rect::new(
            rect.x + 28.,
            rect.y
                + (rect.h
                    - style.size * 1.4 * assets.lines(message, style, rect.w - 56.).min(2) as f32)
                    / 2.,
            rect.w - 56.,
            style.size * 2.8,
        ),
        style,
        design::ink(0.62),
        false,
    );
}

/// Draws a row's cards (docs/design.md): the whole ones in view and the ones cut off at either
/// edge, each clipped to itself, under the edge fades, with a control for each visible card.
/// Returns the first card in view.
fn track_cards(
    out: &mut Scene,
    state: &State,
    track: design::Track,
    kind: BlockKind,
    count: usize,
    mut draw: impl FnMut(&mut Scene, usize, Rect) -> Target,
) -> usize {
    let first = track.first(state.row_scroll.get(&kind).copied().unwrap_or(0), count);
    // Like the favourites, a row reaches past the content's margins to the screen's edge.
    let viewport = state.content_viewport();
    let clip = overlap(
        Rect::new(
            track.clip_left,
            track.y - 16.,
            track.clip_right - track.clip_left,
            track.height + 32.,
        ),
        Rect::new(0., viewport.y, 1920., viewport.h),
    );
    let (left, right) = (first > 0, count > first + track.columns);
    if left || right {
        let end = track.x + track.columns as f32 * track.pitch - design::GAP;
        out.draw.push(Draw::EdgeFadeX(Some([
            if left { track.clip_left } else { 0. },
            if left { track.x } else { 0. },
            if right { end } else { 4000. },
            if right { track.clip_right } else { 4001. },
        ])));
    }
    for index in first.saturating_sub(1)..count.min(first + track.columns + 1) {
        let card = track.card(index, first);
        let shown = overlap(card, clip);
        if shown.w <= 0. {
            continue;
        }
        out.draw
            .push(Draw::Clip(Some(overlap(card.inset(-10.), clip))));
        let target = draw(out, index, card);
        if shown.w > 40. {
            out.control(target, shown, true);
        }
    }
    if left || right {
        out.draw.push(Draw::EdgeFadeX(None));
    }
    out.draw.push(Draw::Clip(Some(state.content_viewport())));
    first
}

/// The part of `a` inside `b`, empty when they do not meet.
fn overlap(a: Rect, b: Rect) -> Rect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    Rect::new(
        x,
        y,
        ((a.x + a.w).min(b.x + b.w) - x).max(0.),
        ((a.y + a.h).min(b.y + b.h) - y).max(0.),
    )
}

/// "3–5 / 12" (or "3 / 12" for one whole card) once a row has more cards than show.
fn row_position(track: &design::Track, first: usize, count: usize) -> Option<String> {
    (count > track.columns).then(|| {
        let last = (first + track.columns).min(count);
        if last == first + 1 {
            format!("{last} / {count}")
        } else {
            format!("{}–{last} / {count}", first + 1)
        }
    })
}

fn weather_block(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    theme: &crate::themes::Theme,
    rect: Rect,
) {
    if !state.preferences.show_weather {
        return;
    }
    let cards = state.preferences.weather_card.resolve(theme.cards);
    if cards {
        widget_card(out, theme, rect);
    }
    let pad = if cards { 24. } else { 0. };
    let summary = weather_summary_layout(state, assets, rect.w - 2. * pad);
    // Tall enough to fit the hourly strip below the summary: top-aligned instead of centred.
    let tall = rect.h > 150.;
    let y = if cards && !tall {
        rect.y + (rect.h - summary.height) / 2.
    } else {
        rect.y + 4.
    };
    let summary_height = summary.height;
    weather_summary(out, state, assets, summary, rect.x + pad, y);
    if tall {
        let hours = state
            .preferences
            .weather_location
            .as_ref()
            .map_or(&[][..], |place| state.weather.hourly(place));
        if !hours.is_empty() {
            let strip_y = y + summary_height + 16.;
            hourly_strip(
                out,
                state,
                assets,
                hours,
                Rect::new(rect.x + pad, strip_y, rect.w - 2. * pad, rect.y + rect.h - strip_y),
            );
        }
    }
}

/// Upcoming hours as a row of compact tiles: time, icon and temperature.
fn hourly_strip(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    hours: &[weather::Hourly],
    rect: Rect,
) {
    let language = state.preferences.language;
    let tile_w = 92.;
    let gap = 8.;
    let count = ((rect.w + gap) / (tile_w + gap)).floor().max(1.) as usize;
    for (index, hour) in hours.iter().take(count).enumerate() {
        let x = rect.x + index as f32 * (tile_w + gap);
        let condition = weather::condition(weather::Reading {
            temperature: hour.temperature,
            code: hour.code,
            is_day: true,
        });
        out.text(
            hour_label(&hour.time, language, state.preferences.hour_24),
            Rect::new(x, rect.y, tile_w, 20.),
            text(14.4, 450, 0.).leading(1.2),
            c(0xf5f5f5, 0.58),
            true,
        );
        out.icon(
            assets,
            &format!("weather-{}", condition.icon),
            Rect::new(x + tile_w / 2. - 12., rect.y + 26., 24., 24.),
            c(0xdedede, 0.83),
        );
        out.text(
            weather::format_temperature(hour.temperature, state.preferences.temperature_unit),
            Rect::new(x, rect.y + 56., tile_w, 24.),
            text(17.6, 500, -0.01).leading(1.2),
            c(0xf5f5f5, 0.87),
            true,
        );
    }
}

/// The hour and minute of an Open-Meteo `YYYY-MM-DDTHH:MM` local timestamp, in the user's clock
/// format; the date is irrelevant since only the time of day is shown.
fn hour_label(time: &str, language: Language, hour_24: bool) -> String {
    if time.len() < 16 {
        return String::new();
    }
    let (Ok(hour), Ok(minute)) = (time[11..13].parse::<i64>(), time[14..16].parse::<i64>()) else {
        return String::new();
    };
    let at = OffsetDateTime::UNIX_EPOCH
        + time::Duration::hours(hour)
        + time::Duration::minutes(minute);
    language.clock(at, hour_24, false)
}

fn favorites_block(out: &mut Scene, state: &State, assets: &Assets, rect: Rect) {
    let language = state.preferences.language;
    let apps = state.apps();
    let names = state.preferences.show_home_names;
    let strip = state.favorites_strip();
    if state.loading && state.inventory.apps.is_empty() {
        let whole = ((strip.right - strip.left + HOME_TILE_PITCH - HOME_TILE_WIDTH)
            / HOME_TILE_PITCH)
            .round() as usize;
        for index in 0..whole {
            out.shape(
                Rect::new(
                    rect.x + index as f32 * HOME_TILE_PITCH,
                    rect.y + 66.,
                    HOME_TILE_WIDTH,
                    FACE_HEIGHT,
                ),
                19.2,
                c(0xffffff, 0.06),
            );
        }
        return;
    }
    if apps.is_empty() {
        let error = state.error.is_some() && state.inventory.apps.is_empty();
        empty(
            out,
            state,
            assets,
            EmptyState {
                rect,
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
        return;
    }
    out.text(
        language.t("favorites"),
        Rect::new(rect.x, rect.y, 1000., 31.),
        text(23.2, 550, -0.035).leading(1.3),
        WHITE,
        false,
    );
    if state.moving.is_some() {
        out.text(
            language.t("moveApps"),
            Rect::new(rect.x + rect.w - 150., rect.y + 6.5, 150., 24.),
            text(14.4, 400, 0.),
            c(0xdedede, 1.),
            true,
        );
    }
    // Like Apple TV's rows, the favourites run to the block's edge (the screen edge when the
    // block reaches it): the next tile shows cut off, so it is clear the row continues.
    out.draw.push(Draw::Clip(Some(state.favorites_viewport())));
    let left = if state.home_scroll > 0. {
        [strip.clip_left, strip.left]
    } else {
        [0., 0.]
    };
    out.draw.push(Draw::EdgeFadeX(Some([
        left[0],
        left[1],
        strip.right,
        strip.clip_right,
    ])));
    for (index, app) in apps.iter().enumerate() {
        tile(
            out,
            state,
            assets,
            app,
            Rect::new(
                rect.x + index as f32 * HOME_TILE_PITCH - state.home_scroll,
                rect.y + 66.,
                HOME_TILE_WIDTH,
                FACE_HEIGHT,
            ),
            names,
            false,
        );
    }
    out.draw.push(Draw::EdgeFadeX(None));
    out.draw.push(Draw::Clip(Some(state.content_viewport())));
}

/// Input cards in rows, as many across as whole cards fit the block.
/// The inputs as a row of wide cards; a one-cell block is a compact row without a header.
fn inputs_block(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    theme: &crate::themes::Theme,
    block: &crate::home_grid::Block,
    rect: Rect,
) {
    let inputs = state.home_inputs();
    let track = design::track(block, rect, design::WIDE_W);
    if track.header {
        row_header(
            out,
            assets,
            rect,
            state.preferences.language.t("inputsHeading"),
            &[],
        );
    }
    track_cards(
        out,
        state,
        track,
        BlockKind::Inputs,
        inputs.len(),
        |out, index, card| {
            input_card(out, state, assets, theme, inputs[index], card);
            Target::Input(inputs[index].id.clone())
        },
    );
}

/// A price with the language's separators: `80 378 $`, `$80,378`, `2,31 $`.
fn price(value: f64, language: Language) -> String {
    let english = matches!(language, Language::En | Language::Zh);
    let decimals = if value >= 1000. {
        0
    } else if value >= 1. {
        2
    } else {
        4
    };
    let text = format!("{value:.decimals$}");
    let (whole, fraction) = text.split_once('.').unwrap_or((&text, ""));
    let group = match language {
        Language::En | Language::Zh => ',',
        Language::Ru => ' ',
        _ => '.',
    };
    let mut grouped = String::new();
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index).is_multiple_of(3) {
            grouped.push(group);
        }
        grouped.push(digit);
    }
    if !fraction.is_empty() {
        grouped.push(if english { '.' } else { ',' });
        grouped.push_str(fraction);
    }
    if english {
        format!("${grouped}")
    } else {
        format!("{grouped} $")
    }
}

/// `Today`, `Tomorrow`, or the day and month in the language's order.
fn short_date(date: &str, now: OffsetDateTime, language: Language) -> String {
    let parse = |text: &str| -> Option<time::Date> {
        let mut parts = text.split('-').map(|part| part.parse::<i32>().ok());
        let (year, month, day) = (parts.next()??, parts.next()??, parts.next()??);
        time::Date::from_calendar_date(year, time::Month::try_from(month as u8).ok()?, day as u8)
            .ok()
    };
    let Some(day) = parse(date) else {
        return date.to_owned();
    };
    match (day - now.date()).whole_days() {
        0 => language.t("today").to_owned(),
        1 => language.t("tomorrow").to_owned(),
        _ => match language {
            Language::En => format!("{}/{}", day.month() as u8, day.day()),
            Language::Zh => format!("{}月{}日", day.month() as u8, day.day()),
            _ => format!("{:02}.{:02}", day.day(), day.month() as u8),
        },
    }
}

/// Coins side by side, as many as fit; a second row of cells adds each coin's 24 h range.
fn crypto_block(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    theme: &crate::themes::Theme,
    rect: Rect,
) {
    let language = state.preferences.language;
    // A glance widget (docs/design.md): on a card by the theme's own choice, or its override.
    let pad = if state.preferences.crypto_card.resolve(theme.cards) {
        widget_card(out, theme, rect);
        24.
    } else {
        0.
    };
    let line_style = text(23.2, 400, -0.025);
    let quotes = &state.feeds.quotes;
    if quotes.is_empty() {
        out.text(
            language.t(if state.feeds.crypto_failed {
                "cryptoUnavailable"
            } else {
                "cryptoLoading"
            }),
            Rect::new(
                rect.x + pad,
                rect.y + rect.h / 2. - 15.,
                rect.w - 2. * pad,
                30.,
            ),
            line_style,
            design::ink(0.58),
            false,
        );
        return;
    }
    let tall = rect.h > 150.;
    let columns = (((rect.w - 2. * pad + 24.) / 272.).floor() as usize).max(1);
    let rows = if tall { 2 } else { 1 };
    let width = (rect.w - 2. * pad) / columns as f32;
    let row_h = (rect.h - 32.) / rows as f32;
    for (index, quote) in quotes.iter().take(columns * rows).enumerate() {
        let x = rect.x + pad + (index % columns) as f32 * width;
        let y = rect.y + 16. + (index / columns) as f32 * row_h;
        let line = y + (row_h - if tall { 56. } else { 30. }) / 2.;
        let coin = line_style.weight(450);
        out.text(
            quote.coin,
            Rect::new(x, line, 80., 30.),
            coin,
            design::ink(0.72),
            false,
        );
        let value = price(quote.last, language);
        let value_style = line_style.weight(550);
        let coin_width = assets.width(quote.coin, coin) + 10.;
        out.text(
            &value,
            Rect::new(x + coin_width, line, width, 30.),
            value_style,
            design::ink(0.91),
            false,
        );
        let change = format!(
            "{}{:.1}%",
            if quote.change >= 0. { "+" } else { "−" },
            quote.change.abs()
        );
        out.text(
            change,
            Rect::new(
                x + coin_width + assets.width(&value, value_style) + 10.,
                line + 4.,
                100.,
                24.,
            ),
            text(18.4, 550, 0.),
            design::color(
                if quote.change >= 0. {
                    design::UP
                } else {
                    design::DOWN
                },
                1.,
            ),
            false,
        );
        if tall {
            out.text(
                format!(
                    "{} – {}",
                    price(quote.low, language),
                    price(quote.high, language)
                ),
                Rect::new(x, line + 34., width, 22.),
                design::meta(),
                design::ink(0.5),
                false,
            );
        }
    }
}

/// Season premieres as a row of wide cards: the poster at the left, the title large beside it,
/// then the season and its date.
fn premieres_block(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    theme: &crate::themes::Theme,
    block: &crate::home_grid::Block,
    rect: Rect,
    now: OffsetDateTime,
) {
    let language = state.preferences.language;
    let premieres = &state.feeds.premieres;
    let track = design::track(block, rect, design::WIDE_W);
    let first = track.first(
        state
            .row_scroll
            .get(&BlockKind::Premieres)
            .copied()
            .unwrap_or(0),
        premieres.len(),
    );
    if track.header {
        let meta: Vec<String> = row_position(&track, first, premieres.len())
            .into_iter()
            .collect();
        row_header(out, assets, rect, language.t("premieresWidget"), &meta);
    }
    if premieres.is_empty() {
        ghost(
            out,
            assets,
            Rect::new(track.x, track.y, rect.w, track.height),
            language.t(if state.feeds.premieres_failed {
                "premieresUnavailable"
            } else {
                "premieresLoading"
            }),
        );
        return;
    }
    track_cards(
        out,
        state,
        track,
        BlockKind::Premieres,
        premieres.len(),
        |out, index, rect| {
            let premiere = &premieres[index];
            let focused = state.focus == Target::Poster(index) && !state.pointer;
            card(out, theme, rect, focused, None);
            let poster = Rect::new(rect.x, rect.y, (rect.h * 2. / 3.).round(), rect.h);
            match premiere
                .poster
                .as_ref()
                .and_then(|url| state.feeds.images.get(url))
            {
                Some(image) => out.draw.push(Draw::Picture(
                    image.clone(),
                    poster,
                    design::RADIUS,
                    [1.; 4],
                )),
                None => {
                    out.draw.push(Draw::RoundedGradient(
                        poster,
                        design::RADIUS,
                        alpha(name_tint(&premiere.title), 0.45),
                        alpha(name_tint(&premiere.title), 0.15),
                    ));
                    out.icon(assets, "tv", poster.inset(poster.w * 0.3), design::ink(0.5));
                }
            }
            let tall = rect.h > 200.;
            let x = poster.x + poster.w + design::PAD;
            let w = rect.x + rect.w - design::PAD - x;
            let title = design::card_title(tall).leading(1.18);
            let lines = assets
                .lines(&premiere.title, title, w)
                .min(if tall { 3 } else { 2 }) as f32;
            out.text(
                &premiere.title,
                Rect::new(
                    x,
                    rect.y + design::PAD - 2.,
                    w,
                    lines * title.size * 1.18 + 2.,
                ),
                title,
                WHITE,
                false,
            );
            let info = language.format(
                "seasonPremiere",
                &[
                    ("season", &premiere.season.to_string()),
                    ("date", &short_date(&premiere.date, now, language)),
                ],
            );
            let style = design::body();
            out.text(
                fit(assets, &info, style, w),
                Rect::new(x, rect.y + rect.h - design::PAD - 24., w, 24.),
                style,
                design::ink(0.7),
                false,
            );
            Target::Poster(index)
        },
    );
}

fn input_card(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    theme: &crate::themes::Theme,
    input: &Input,
    rect: Rect,
) {
    let language = state.preferences.language;
    let focused = state.focus == Target::Input(input.id.clone()) && !state.pointer;
    card(out, theme, rect, focused, None);
    let art = Rect::new(rect.x + 21., rect.y + (rect.h - 56.) / 2., 56., 56.);
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
    let top = (rect.h - (28.8 * title_k + 5.6 + 20. * k)) / 2.;
    out.text(
        state.preferences.input_title(input),
        Rect::new(rect.x + 93., rect.y + top, rect.w - 114., 28.8 * title_k),
        text(24. * title_k, 500, -0.03),
        WHITE,
        false,
    );
    out.text(
        input_status(input, name.is_some(), language),
        Rect::new(
            rect.x + 93.,
            rect.y + top + 28.8 * title_k + 5.6,
            rect.w - 114.,
            20. * k,
        ),
        text(15.2 * k, 450, 0.).leading(1.5),
        design::ink(0.55),
        false,
    );
}

struct WeatherSummary {
    height: f32,
    width: f32,
    icon: String,
    parts: Vec<(String, TextStyle, Color, Rect)>,
}
type WeatherPart = (String, TextStyle, Color, f32, f32);
fn weather_summary_layout(state: &State, assets: &Assets, width: f32) -> WeatherSummary {
    let line = width - 37.6;
    let language = state.preferences.language;
    let base = text(23.2, 400, -0.025).leading(1.5);
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
                    text(15.08, 400, 0.).leading(1.5),
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
        let measured_width = assets.width(&label, style);
        let width = measured_width.min(line);
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
        if used > 0. && used + 11.2 + width > line {
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
        let height = row.iter().map(|part| part.4).fold(34.8, f32::max);
        let mut x = 37.6;
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
        width: right,
        icon,
        parts: positioned,
    }
}
fn weather_summary(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    summary: WeatherSummary,
    x: f32,
    y: f32,
) {
    let label_focus = summary.parts.first().map(|(_, style, _, rect)| {
        // The label's layout line box includes leading above and below the
        // text. Keep the focus ring snug to the label rather than the whole
        // weather row, which also contains the location icon.
        let leading = style.size * (style.line_height - 1.).max(0.);
        Rect::new(
            x + rect.x,
            y + rect.y + leading / 2.,
            rect.w,
            (rect.h - leading).max(style.size),
        )
    });
    out.icon(
        assets,
        &summary.icon,
        Rect::new(x, y + 5.8, 26.4, 26.4),
        c(0xdedede, 0.83),
    );
    for (label, style, color, rect) in summary.parts {
        out.text(
            label,
            Rect::new(x + rect.x, rect.y + y, rect.w, rect.h),
            style,
            color,
            false,
        );
    }
    if state.preferences.weather_location.is_none() {
        let rect = Rect::new(x, y, summary.width, summary.height);
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
    let s = state.scroll;
    let x = category_chips(out, state, assets, 84., 192.8 - s);
    if state.filter != 0 {
        let k = state.preferences.ui_scale.scale();
        let rect = Rect::new(x, 192.8 - 16. * k - s, 32. * k, 32. * k);
        out.shape(rect, 16. * k, c(0x23282c, 0.5));
        out.icon(assets, "close", rect.inset(8. * k), c(0xeeeeee, 0.73));
        if state.focus == Target::ClearFilter {
            out.border(rect.inset(-2.), 16. * k + 2., 2., WHITE);
        }
        out.control(Target::ClearFilter, rect, true);
    }
    search_field(out, state, assets, 1836., 192.8 - s);
    library_grid(out, state, assets);
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
                rect: Rect::new(84., 259.2 - s, 1752., 304.),
                icon: "grid",
                title,
                hint: &hint,
                action: state.error.as_ref().map(|_| Target::Retry),
            },
        );
    }
    let mut y = 270.4;
    let names = state.preferences.show_app_names;
    for row in apps.chunks(APPS_COLUMNS) {
        let row_height = FACE_HEIGHT
            + if names {
                18.4 + row
                    .iter()
                    .map(|app| caption_height(state, assets, app, 270.))
                    .fold(49.28 * caption_scale(state), f32::max)
            } else {
                0.
            };
        for (column, app) in row.iter().enumerate() {
            tile(
                out,
                state,
                assets,
                app,
                Rect::new(84. + column as f32 * GRID_PITCH, y - s, 270., FACE_HEIGHT),
                names,
                favorites.contains(&app.id),
            );
        }
        y += row_height + 38.4;
    }
}

/// The category filter chips from `x`, centred on `y`; returns where the row ends.
fn category_chips(out: &mut Scene, state: &State, assets: &Assets, mut x: f32, y: f32) -> f32 {
    let language = state.preferences.language;
    let k = state.preferences.ui_scale.scale();
    let style = text(16. * k, 400, 0.);
    let h = 42.8 * k;
    for (index, label) in CATEGORIES.into_iter().enumerate() {
        let label = language.category(label);
        let width = assets.width(label, style) + 35.6 * k;
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
        let rect = Rect::new(x, y - h / 2., assets.width(&label, style) + 35.6 * k, h);
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
fn search_field(out: &mut Scene, state: &State, assets: &Assets, right: f32, y: f32) {
    let language = state.preferences.language;
    let k = state.preferences.ui_scale.scale();
    let search = Rect::new(right - 304. * k, y - 28. * k, 304. * k, 56. * k);
    let focused = matches!(state.focus, Target::Search | Target::ClearSearch);
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
        Rect::new(search.x + 19.4 * k, search.y + 18.4 * k, 19.2 * k, 19.2 * k),
        c(0xeeeeee, 0.73),
    );
    out.text(
        if state.query.is_empty() {
            language.t("findApp")
        } else {
            &state.query
        },
        Rect::new(
            search.x + 49.8 * k,
            search.y + 17.8 * k,
            if state.query.is_empty() {
                234. * k
            } else {
                202. * k
            },
            22. * k,
        ),
        text(16.8 * k, 400, -0.02),
        c(0xeeeeee, if state.query.is_empty() { 0.73 } else { 1. }),
        false,
    );
    out.control(Target::Search, search, true);
    if !state.query.is_empty() {
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
        out.control(Target::ClearSearch, clear, true);
    }
}

fn filter(out: &mut Scene, state: &State, target: Target, rect: Rect, label: &str, selected: bool) {
    let k = rect.h / 42.8;
    if selected {
        out.shape(rect, 32., c(0xffffff, 0.11));
        out.border(rect, 32., 1., c(0xffffff, 0.07));
    }
    if state.focus == target && !state.pointer {
        out.border(rect.inset(-7.), 39., 2., c(0xf0f0f0, 0.95));
    }
    out.text(
        label,
        Rect::new(rect.x, rect.y + 11.4 * k, rect.w, 20. * k),
        text(16. * k, 400, 0.),
        c(
            if selected { 0xf5f5f5 } else { 0xf0f0f0 },
            if selected { 1. } else { 0.66 },
        ),
        true,
    );
    out.control(target, rect, true);
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
        out.shape(row.rect, radius, c(0x171c20, 0.72));
        out.border(row.rect, radius, 1., c(0xffffff, 0.07));
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
    let compact_value_width = (row.target == Target::WallpaperCycle)
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
    let scrollable = !matches!(row.target, Target::UploadWallpaper | Target::WallpaperCycle);
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
    row(
        out,
        state,
        assets,
        Row {
            target: Target::WeatherCard,
            rect: Rect::new(640., y + 84., 1120., 72.),
            title: language.t("cardBackground"),
            icon: Some("grid"),
            value: Some(language.t(state.preferences.weather_card.label_key())),
            description: None,
            selected: false,
            chevron: false,
        },
    );
}

/// The crypto widget's own settings: whether it sits on a card.
fn crypto_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let s = state.scroll;
    row(
        out,
        state,
        assets,
        Row {
            target: Target::CryptoCard,
            rect: Rect::new(640., 164.8 - s, 1120., 72.),
            title: language.t("cardBackground"),
            icon: Some("grid"),
            value: Some(language.t(state.preferences.crypto_card.label_key())),
            description: None,
            selected: false,
            chevron: false,
        },
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
    let language = state.preferences.language;
    let s = state.scroll;
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
                Some(Subpage::ClockFormat) => Some(format!(
                    "{} › {}",
                    language.t("clock"),
                    language.t("clockFormat")
                )),
                Some(Subpage::TimeFormat) => Some(format!(
                    "{} › {}",
                    language.t("clock"),
                    language.t("timeFormat")
                )),
                Some(Subpage::WeatherLocation | Subpage::Temperature) => Some(format!(
                    "{} › {}",
                    language.t("weather"),
                    language.t(if state.subpage == Some(Subpage::WeatherLocation) {
                        "location"
                    } else {
                        "temperature"
                    })
                )),
                Some(Subpage::Input(id)) => Some(format!(
                    "{} › {}",
                    language.t("inputs"),
                    state
                        .inventory
                        .inputs
                        .iter()
                        .find(|input| &input.id == id)
                        .map_or(language.t("inputs"), |input| state
                            .preferences
                            .input_title(input))
                )),
                _ => None,
            };
            let mut widget_title: Option<String> = None;
            let sub = match &state.subpage {
                Some(Subpage::AppNames) => language.t("appNames"),
                Some(Subpage::Greeting) => language.t("greeting"),
                Some(Subpage::Weather) => language.t("weather"),
                Some(Subpage::Shortcuts) => language.t("shortcuts"),
                Some(Subpage::Inputs) => language.t("inputs"),
                Some(Subpage::Crypto) => language.t("cryptoWidget"),
                Some(Subpage::Clock) => language.t("clock"),
                Some(Subpage::ClockFormat | Subpage::TimeFormat) => nested_path.as_deref().unwrap(),
                Some(Subpage::WeatherLocation | Subpage::Temperature | Subpage::Input(_)) => {
                    nested_path.as_deref().unwrap()
                }
                Some(Subpage::Language) => language.t("language"),
                Some(Subpage::Appearance) => language.t("uiScale"),
                Some(Subpage::Theme) => language.t("homeTheme"),
                Some(Subpage::Widget(slot)) => widget_title
                    .get_or_insert(state.block_name(crate::home_grid::BlockKind::Widget(*slot))),
                Some(Subpage::WidgetOptions(slot, setting)) => widget_title.get_or_insert(
                    state
                        .slot_manifest(*slot)
                        .and_then(|m| {
                            m.settings
                                .get(*setting)
                                .map(|s| s.label.get(language.code()).to_owned())
                        })
                        .unwrap_or_default(),
                ),
                Some(Subpage::HomeTakeover) => language.t("homeTakeover"),
                Some(Subpage::TextRendering) => language.t("textRendering"),
                None => "",
            };
            let parent_width = assets.width(label, text(20.8, 550, -0.02)) + 2.;
            out.text(
                label,
                Rect::new(rect.x + 24.2, rect.y + 23.5, parent_width, 25.),
                text(20.8, 550, -0.02),
                [color[0], color[1], color[2], color[3] * 0.7],
                false,
            );
            out.icon(
                assets,
                "right",
                Rect::new(rect.x + 24.2 + parent_width + 5., rect.y + 27., 17., 17.),
                [color[0], color[1], color[2], color[3] * 0.6],
            );
            let sub_width = rect.w - 24.2 - parent_width - 28. - 24.2;
            let mut sub_style = text(20.8, 550, -0.02);
            if nested_path.is_some() {
                sub_style.size *= (sub_width / assets.width(sub, sub_style)).min(1.);
            }
            out.text(
                sub,
                Rect::new(
                    rect.x + 24.2 + parent_width + 28.,
                    // A name shrunk to fit stays centred on the parent's line.
                    rect.y + 23.5 + (20.8 - sub_style.size) * sub_style.line_height / 2.,
                    sub_width,
                    25.,
                ),
                sub_style,
                color,
                false,
            );
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
    if let Some(subpage) = &state.subpage {
        let start = out.draw.len();
        if matches!(
            subpage,
            Subpage::WeatherLocation
                | Subpage::Weather
                | Subpage::Shortcuts
                | Subpage::Inputs
                | Subpage::Greeting
                | Subpage::AppNames
                | Subpage::Clock
                | Subpage::Crypto
                | Subpage::Widget(_)
        ) {
            match subpage {
                Subpage::WeatherLocation => location_settings(out, state, assets),
                Subpage::Weather => weather_settings(out, state, assets),
                Subpage::Shortcuts => shortcut_settings(out, state, assets),
                Subpage::Inputs => inputs_settings(out, state, assets),
                Subpage::Greeting => greeting_settings(out, state, assets),
                Subpage::AppNames => app_name_settings(out, state, assets),
                Subpage::Clock => clock_settings(out, state, assets),
                Subpage::Crypto => crypto_settings(out, state, assets),
                Subpage::Widget(slot) => widget_view::settings(out, state, assets, *slot),
                _ => unreachable!(),
            }
            if let Some(opened) = state.panel_started {
                out.fade(start, (opened.elapsed().as_secs_f32() / 0.15).min(1.), 1.);
            }
            return;
        }
        let widget_options: Vec<(String, bool)> = match subpage {
            Subpage::WidgetOptions(slot, setting) => state.widget_options(*slot, *setting),
            _ => vec![],
        };
        let options: Vec<(&str, Option<&str>, bool)> = match subpage {
            Subpage::WidgetOptions(..) => widget_options
                .iter()
                .map(|(label, chosen)| (label.as_str(), None, *chosen))
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
                        Some(language.t("inputWhenConnected")),
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
            Subpage::Weather | Subpage::Shortcuts | Subpage::Inputs | Subpage::Crypto => {
                unreachable!()
            }
            Subpage::Theme => crate::themes::THEMES
                .iter()
                .map(|theme| {
                    (
                        language.t(theme.name_key),
                        Some(language.t(theme.hint_key)),
                        theme.id == state.preferences.home_theme,
                    )
                })
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
            Subpage::WeatherLocation
            | Subpage::Greeting
            | Subpage::AppNames
            | Subpage::Clock
            | Subpage::Widget(_) => unreachable!(),
            Subpage::Language => Language::ALL
                .into_iter()
                .map(|option| (option.name(), None, option == language))
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
        if *subpage == Subpage::Theme {
            out.text(
                language.t("homeThemeReplaces"),
                Rect::new(
                    660.,
                    164.8 + crate::themes::THEMES.len() as f32 * 102.7 + 10. - s,
                    1080.,
                    24.,
                ),
                text(16., 400, 0.),
                c(0xf5f5f5, 0.55),
                false,
            );
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
                (4, "language", "language", Some(language.name()), true),
                (
                    7,
                    "uiScale",
                    "grid",
                    Some(language.t(state.preferences.ui_scale.keys().0)),
                    true,
                ),
                (
                    9,
                    "homeTheme",
                    "sparkles",
                    Some(language.t(crate::themes::theme(&state.preferences.home_theme).name_key)),
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
        BACKGROUND_SECTION => {
            let upload_label = language.t("uploadWallpaper");
            let upload_width = (assets.width(upload_label, text(20.8, 550, -0.02)) + 88.)
                .ceil()
                .min(520.);
            let cycle_label = language.t("wallpaperCycle");
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
                    rect: Rect::new(
                        1760. - upload_width - 20. - cycle_width,
                        164.8,
                        cycle_width,
                        64.,
                    ),
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
                    target: Target::UploadWallpaper,
                    rect: Rect::new(1760. - upload_width, 164.8, upload_width, 64.),
                    title: upload_label,
                    icon: Some("upload"),
                    value: None,
                    description: None,
                    selected: false,
                    chevron: false,
                },
            );
            out.draw
                .push(Draw::Clip(Some(state.backgrounds_viewport())));
            for index in 0..state.background_count() {
                let id = state.background_id(index).unwrap();
                let custom = state.wallpapers.iter().find(|item| item.id == id);
                let rect = Rect::new(
                    640. + (index % 3) as f32 * 380.,
                    252.8 + (index / 3) as f32 * 226.5 - s,
                    360.,
                    202.5,
                );
                out.shadow(rect, 16., 15., 3., 0.13);
                out.image(
                    custom.map_or_else(
                        || assets.path(format!("thumbnails/{id}.jpg")),
                        |item| item.thumbnail(),
                    ),
                    rect,
                    16.,
                    false,
                );
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
            out.draw.push(Draw::Clip(Some(state.content_viewport())));
        }
        WIDGETS_SECTION => {
            for (index, (key, icon)) in [
                ("greeting", "type"),
                ("weather", "weather-sun"),
                ("shortcuts", "sparkles"),
                ("inputs", "input-hdmi"),
                ("cryptoWidget", "generic"),
            ]
            .into_iter()
            .enumerate()
            {
                row(
                    out,
                    state,
                    assets,
                    Row {
                        target: Target::Widget(index),
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
            let version_rect = Rect::new(640., 164.8 - s, 1120., 72.);
            settings_surface(out, version_rect);
            out.text(
                format!("Luma v{}", assets.version.trim()),
                Rect::new(664.2, version_rect.y + (72. - 29.12) / 2., 1031.8, 29.12),
                text(20.8, 550, -0.02).leading(1.4),
                c(0xeeeeee, 0.5),
                false,
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
                    10,
                    language.t("textRendering"),
                    "type",
                    Some(language.t(state.preferences.text_rendering.label_key())),
                    true,
                ),
                (5, language.t("quitApp"), "power", None, false),
                (
                    8,
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
                        rect: Rect::new(640., 248.8 + position as f32 * 84. - s, 1120., 72.),
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
            rect: Rect::new(640., 164.8 - state.scroll, 1120., 90.7),
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
            rect: Rect::new(640., 267.5 - state.scroll, 1120., 72.),
            title: language.t("clearName"),
            icon: Some("refresh"),
            value: None,
            description: None,
            selected: false,
            chevron: false,
        },
    );
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
            state.preferences.show_app_names,
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
                value: Some(language.t(if enabled { "stateOn" } else { "stateOff" })),
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

fn settings_surface(out: &mut Scene, rect: Rect) {
    out.shape(rect, 48., c(0x171c20, 0.3));
    out.border(rect, 48., 1., c(0xffffff, 0.045));
}

fn shortcut_settings(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let tiles = &state.preferences.custom_tiles;
    for (index, tile) in tiles.iter().enumerate() {
        let description = format!("{} · {}", tile.method.label(), shorten(&tile.url));
        let y = 248.8 + index as f32 * 102.7 - state.scroll;
        let rect = Rect::new(640., y, 1036., 90.7);
        settings_surface(out, rect);
        out.icon(
            assets,
            tile.icon(),
            Rect::new(664.2, y + 33.35, 24., 24.),
            c(0xeeeeee, 0.5),
        );
        out.text(
            &tile.title,
            Rect::new(711., y + 18., 940., 29.12),
            text(20.8, 550, -0.02).leading(1.4),
            c(0xeeeeee, 0.5),
            false,
        );
        out.text(
            description,
            Rect::new(711., y + 51., 940., 24.),
            text(16.8, 400, 0.),
            c(0xeeeeee, 0.5),
            false,
        );
        let remove = Rect::new(1688., y + 13.35, 72., 64.);
        let target = Target::TileRemove(tile.id.clone());
        let focused = state.focus == target;
        out.pill(remove, focused, false);
        out.icon(
            assets,
            "close",
            Rect::new(remove.x + 24., remove.y + 20., 24., 24.),
            if focused {
                c(0x2c2c2c, 1.)
            } else {
                c(0xeeeeee, 0.6)
            },
        );
        out.control(target, remove, true);
    }
    if tiles.is_empty() {
        out.text(
            language.t("noTiles"),
            Rect::new(664., 254. - state.scroll, 1000., 52.),
            text(16.8, 400, 0.).leading(1.5),
            c(0xf5f5f5, 0.72),
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
        Some(true) => language.t("connected"),
    });
    parts.join(" · ")
}

fn keycap(out: &mut Scene, label: &str, x: f32, y: f32, w: f32, small: bool, k: f32) {
    let h = if small { 16. } else { 23.2 } * k;
    out.border(
        Rect::new(x, y, w, h),
        4.8,
        1.,
        c(0xffffff, if small { 0.125 } else { 0.20 }),
    );
    out.text(
        label,
        Rect::new(x, y + if small { 2. } else { 4.2 } * k, w, h),
        text(if small { 8. } else { 11.52 } * k, 500, 0.),
        c(0xf3f3f3, 0.64),
        true,
    );
}

fn footer(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    // Hints are centred on y 1042; larger hints are also less faint.
    let k = state.preferences.ui_scale.scale();
    let style = text(14.4 * k, 400, 0.005);
    let color = c(0xeeeeee, if k > 1. { 0.6 } else { 0.37 });
    let (cap_y, text_y) = (1041.6 - 11.6 * k, 1042. - 9. * k);
    if let Some(edit) = &state.home_edit {
        // Editing a Home block: arrows move or resize it, OK keeps, Back restores.
        let mut x = 84.;
        for (key, width, label) in [
            (
                "← → ↑ ↓",
                70.,
                language.t(if edit.resize {
                    "blockResize"
                } else {
                    "blockMove"
                }),
            ),
            ("OK", 27., language.t("done")),
            ("Back", 39., language.t("cancel")),
        ] {
            keycap(out, key, x, cap_y, width * k, false, k);
            let label_x = x + (width + 8.) * k;
            out.text(
                label,
                Rect::new(label_x, text_y, 300. * k, 20. * k),
                style,
                color,
                false,
            );
            x = label_x + assets.width(label, style) + 25.6 * k;
        }
        return;
    }
    if state.page == Page::Home && state.moving.is_none() {
        return;
    }
    if state.moving.is_some() {
        keycap(out, "← →", 84., cap_y, 42. * k, false, k);
        let label_x = 84. + 50. * k;
        out.text(
            language.t("move"),
            Rect::new(label_x, text_y, 100. * k, 20. * k),
            style,
            color,
            false,
        );
        let second_x = label_x + assets.width(language.t("move"), style) + 25.6 * k;
        keycap(out, "OK/Back", second_x, cap_y, 62. * k, false, k);
        out.text(
            language.t("done"),
            Rect::new(second_x + 70. * k, text_y, 150. * k, 20. * k),
            style,
            color,
            false,
        );
    } else {
        let label = language.t(if state.page == Page::Settings {
            "select"
        } else {
            "open"
        });
        let label_x = 84. + 35. * k;
        let second_x = label_x + assets.width(label, style) + 25.6 * k;
        keycap(out, "OK", 84., cap_y, 27. * k, false, k);
        out.text(
            label,
            Rect::new(label_x, text_y, 100. * k, 20. * k),
            style,
            color,
            false,
        );
        if state.page == Page::Settings {
            keycap(out, "Back", second_x, cap_y, 39. * k, false, k);
            out.text(
                language.t("homeLabel"),
                Rect::new(second_x + 47. * k, text_y, 180. * k, 20. * k),
                style,
                color,
                false,
            );
            out.text(
                format!("v{}", assets.version.trim()),
                Rect::new(1775., 1037., 61., 16.),
                text(12.48, 400, 0.),
                c(0xf5f5f5, 0.56),
                false,
            );
        } else {
            out.icon(
                assets,
                "more",
                Rect::new(second_x, 1042. - 9.6 * k, 19.2 * k, 19.2 * k),
                c(0xeeeeee, color[3] * 0.7),
            );
            out.text(
                language.t("options"),
                Rect::new(second_x + 27.2 * k, text_y, 400. * k, 20. * k),
                style,
                color,
                false,
            );
        }
    }
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
    let height = if background {
        if deletable { 176. } else { 99.2 }
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
        let path = state
            .wallpapers
            .iter()
            .find(|item| item.id == id)
            .map_or_else(
                || assets.path(format!("thumbnails/{id}.jpg")),
                |item| item.thumbnail(),
            );
        out.image(path, card, 16., false);
        out.border(card, 16., 1., c(0xffffff, 0.086));
    } else if let Some(app) = app {
        app_face(out, assets, app, card, language, 19.2);
        if favorite {
            out.icon(
                assets,
                "heart-filled",
                Rect::new(card.x + card.w - 27.2, card.y + 11.2, 16., 16.),
                c(0xffffff, 0.9),
            );
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
    let actions = if background {
        let mut actions = vec![(Target::UseBackground, language.t("useBackground"))];
        if deletable {
            actions.push((Target::DeleteBackground, language.t("deleteBackground")));
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
        let destructive = matches!(target, Target::Option(2) | Target::DeleteBackground);
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
                Target::OpenApp => "arrow",
                Target::UseBackground => "check",
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
    let line_y = panel.y + panel.h - 53.2;
    out.shape(Rect::new(717.8, line_y, 484.4, 1.), 0., c(0xffffff, 0.04));
    let shortcut = text(10.4, 400, 0.);
    let select_width = assets.width(language.t("selectClose"), shortcut);
    let close_width = assets.width(language.t("close"), shortcut);
    let start = 960. - (23. + 6.4 + select_width + 15. + 29. + 6.4 + close_width) / 2.;
    let back = start + 29.4 + select_width + 15.;
    keycap(out, "OK", start, line_y + 15., 23., true, 1.);
    out.text(
        language.t("selectClose"),
        Rect::new(start + 29.4, line_y + 18., select_width + 1., 13.),
        text(10.4, 400, 0.),
        c(0xe0e0e0, 0.35),
        false,
    );
    keycap(out, "Back", back, line_y + 15., 29., true, 1.);
    out.text(
        language.t("close"),
        Rect::new(back + 35.4, line_y + 18., close_width + 1., 13.),
        text(10.4, 400, 0.),
        c(0xe0e0e0, 0.35),
        false,
    );
}

fn keyboard(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let keyboard_title = state.widget_editing_label();
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
            TextField::Widget => keyboard_title.as_str(),
            TextField::Apps => language.t("findApp"),
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
    let chips = panel.y + 56. + 32. * k;
    category_chips(out, state, assets, panel.x + 32., chips);
    search_field(out, state, assets, panel.x + panel.w - 32., chips);
    let favorites = state.preferences.favorites(&state.inventory.apps);
    let apps = favorites
        .iter()
        .filter(|id| !state.preferences.hidden.contains(id))
        .filter_map(|id| state.inventory.apps.iter().find(|app| &app.id == id))
        .take(3);
    for (index, app) in apps.enumerate() {
        let rect = Rect::new(
            panel.x + 32. + index as f32 * HOME_TILE_PITCH,
            chips + 32. * k + 28.,
            HOME_TILE_WIDTH,
            FACE_HEIGHT,
        );
        tile(out, state, assets, app, rect, true, false);
    }
    out.controls.truncate(controls);
}

/// A modal list of actions: a title and rows that activate `target(index)`.
fn menu(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    title: &str,
    rows: &[String],
    target: fn(usize) -> Target,
) {
    let panel = Rect::new(
        680.,
        540. - (120. + rows.len() as f32 * 76.8) / 2.,
        560.,
        120. + rows.len() as f32 * 76.8,
    );
    out.modal = Some(panel);
    out.shadow(panel, 27.2, 120., 25., 0.47);
    out.draw.push(Draw::RoundedGradient(
        panel,
        27.2,
        c(0x34383b, 0.96),
        c(0x24282d, 0.988),
    ));
    out.border(panel, 27.2, 1., c(0xffffff, 0.12));
    out.text(
        title,
        Rect::new(717.8, panel.y + 34., 484.4, 42.),
        text(32., 500, -0.04).leading(1.1),
        WHITE,
        false,
    );
    for (index, label) in rows.iter().enumerate() {
        let rect = Rect::new(717.8, panel.y + 96. + index as f32 * 76.8, 484.4, 67.2);
        let focused = state.focus == target(index);
        out.shape(rect, 12., c(0xffffff, if focused { 0.07 } else { 0.016 }));
        out.border(
            rect,
            12.,
            if focused { 2. } else { 1. },
            c(
                if focused { 0xf0f0f0 } else { 0xffffff },
                if focused { 0.95 } else { 0.05 },
            ),
        );
        out.text(
            label,
            Rect::new(rect.x + 22., rect.y + 22., rect.w - 44., 25.),
            text(17.6, 500, -0.015),
            WHITE,
            false,
        );
        out.control(target(index), rect, false);
    }
    let _ = assets;
}

/// The local time of an RFC 3339 instant, with the day when it is not today.
fn match_time(begin_at: &str, now: OffsetDateTime, language: Language) -> String {
    OffsetDateTime::parse(begin_at, &time::format_description::well_known::Rfc3339)
        .map_or(String::new(), |start| local_time(start, now, language))
}

/// Whole minutes since an RFC 3339 instant.
fn minutes_since(begin_at: &str, now: OffsetDateTime) -> Option<i64> {
    let start =
        OffsetDateTime::parse(begin_at, &time::format_description::well_known::Rfc3339).ok()?;
    Some((now - start).whole_minutes()).filter(|minutes| *minutes >= 0)
}

/// A loaded image from the feeds' cache, by its URL.
fn feed_image<'a>(
    state: &'a State,
    url: &Option<String>,
) -> Option<&'a std::sync::Arc<crate::app_artwork::Artwork>> {
    url.as_ref().and_then(|url| state.feeds.images.get(url))
}

/// An instant in the local time of `now`, with the day when it is not today.
fn local_time(instant: OffsetDateTime, now: OffsetDateTime, language: Language) -> String {
    let local = instant.to_offset(now.offset());
    let clock = format!("{}:{:02}", local.hour(), local.minute());
    match (local.date() - now.date()).whole_days() {
        0 => clock,
        1 => format!("{} {clock}", language.t("tomorrow")),
        _ => match language {
            Language::En => format!("{}/{} {clock}", local.month() as u8, local.day()),
            Language::Zh => format!("{}月{}日 {clock}", local.month() as u8, local.day()),
            _ => format!("{:02}.{:02} {clock}", local.day(), local.month() as u8),
        },
    }
}

/// `value`, shortened with an ellipsis to fit `width`.
fn fit(assets: &Assets, value: &str, style: TextStyle, width: f32) -> String {
    if assets.width(value, style) <= width {
        return value.to_owned();
    }
    let mut chars: Vec<char> = value.chars().collect();
    while chars.pop().is_some() {
        let shorter = format!("{}…", chars.iter().collect::<String>().trim_end());
        if assets.width(&shorter, style) <= width {
            return shorter;
        }
    }
    String::new()
}

/// An image, or the initials of `name` on a disc until it arrives (or when there is none),
/// in `tint` when given.
fn emblem(
    out: &mut Scene,
    state: &State,
    image: &Option<String>,
    name: &str,
    rect: Rect,
    tint: Option<Color>,
) {
    if let Some(image) = feed_image(state, image) {
        out.draw.push(Draw::AppImage(image.clone(), rect, [1.; 4]));
        return;
    }
    match tint {
        Some(tint) => {
            out.draw.push(Draw::RoundedGradient(
                rect,
                rect.w / 2.,
                alpha(tint, 0.42),
                alpha(tint, 0.2),
            ));
            out.border(rect, rect.w / 2., 1., alpha(tint, 0.6));
        }
        None => {
            out.shape(rect, rect.w / 2., c(0xffffff, 0.08));
            out.border(rect, rect.w / 2., 1., c(0xffffff, 0.10));
        }
    }
    let initials: String = name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(2)
        .collect::<String>()
        .to_uppercase();
    let size = (rect.h * 0.36).max(10.);
    out.text(
        if initials.is_empty() {
            "?".into()
        } else {
            initials
        },
        Rect::new(
            rect.x,
            rect.y + (rect.h - size * 1.25) / 2.,
            rect.w,
            size * 1.25,
        ),
        text(size, 650, 0.02),
        c(0xf5f5f5, 0.78),
        true,
    );
}

/// A small label on a rounded fill; returns where the next one starts.
fn chip(
    out: &mut Scene,
    assets: &Assets,
    label: &str,
    x: f32,
    y: f32,
    tone: (Color, Color),
) -> f32 {
    let style = text(15.2, 550, 0.01);
    let w = assets.width(label, style) + 28.;
    let rect = Rect::new(x, y, w, 30.);
    out.shape(rect, 15., tone.0);
    out.text(
        label,
        Rect::new(rect.x, rect.y + 6., w, 19.),
        style,
        tone.1,
        true,
    );
    x + w + 10.
}

/// One of a few calm colours, always the same for the same name.
fn name_tint(name: &str) -> Color {
    const TINTS: [u32; 8] = [
        0x5b7cfa, 0xe5484d, 0x2fb67c, 0xf5a524, 0x9b6cf6, 0x17b3c7, 0xe0609f, 0x8a94a6,
    ];
    let hash = name.bytes().fold(0u32, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(byte as u32)
    });
    c(TINTS[hash as usize % TINTS.len()], 1.)
}

fn alpha(color: Color, alpha: f32) -> Color {
    [color[0], color[1], color[2], alpha]
}

/// Sending the key from a phone: the QR code, numbered steps and the address to type instead.
fn phone_key_dialog(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let panel = Rect::new(360., 230., 1200., 620.);
    out.modal = Some(panel);
    out.shadow(panel, 27.2, 120., 25., 0.47);
    out.draw.push(Draw::RoundedGradient(
        panel,
        27.2,
        c(0x34383b, 0.97),
        c(0x24282d, 0.99),
    ));
    out.border(panel, 27.2, 1., c(0xffffff, 0.12));
    let close = Rect::new(panel.x + panel.w - 72., panel.y + 36., 33.6, 33.6);
    out.shape(close, 16.8, c(0xffffff, 0.035));
    out.border(close, 16.8, 1., c(0xffffff, 0.07));
    out.icon(assets, "close", close.inset(7.6), WHITE);
    if state.focus == Target::CloseDialog {
        out.border(close.inset(1.), 15.8, 2., WHITE);
    }
    out.control(Target::CloseDialog, close, false);
    out.text(
        language.t("phoneKeyTitle"),
        Rect::new(panel.x + 48., panel.y + 40., panel.w - 160., 44.),
        text(32., 500, -0.04),
        WHITE,
        false,
    );
    let code = Rect::new(panel.x + 48., panel.y + 120., 400., 400.);
    out.shape(code, 18., c(0xffffff, 1.));
    let x = code.x + code.w + 48.;
    let Some((url, qr)) = &state.phone_key else {
        out.text(
            language.t("phoneKeyStarting"),
            Rect::new(x, code.y + 10., panel.x + panel.w - x - 48., 28.),
            text(20., 450, 0.),
            c(0xf5f5f5, 0.7),
            false,
        );
        return;
    };
    out.draw
        .push(Draw::AppImage(qr.clone(), code.inset(14.), [1.; 4]));
    let width = panel.x + panel.w - x - 48.;
    let step = text(20., 450, -0.01).leading(1.35);
    let mut y = code.y;
    for (number, key) in [
        "phoneKeyStep1",
        "phoneKeyStep2",
        "phoneKeyStep3",
        "phoneKeyStep4",
    ]
    .into_iter()
    .enumerate()
    {
        let line = format!("{}. {}", number + 1, language.t(key));
        let height = assets.lines(&line, step, width) as f32 * step.size * step.line_height;
        out.text(
            &line,
            Rect::new(x, y, width, height + 4.),
            step,
            WHITE,
            false,
        );
        y += height + 18.;
    }
    out.text(
        language.t("phoneKeyAddress"),
        Rect::new(x, y + 12., width, 22.),
        text(16., 450, 0.),
        c(0xf5f5f5, 0.6),
        false,
    );
    out.text(
        url,
        Rect::new(x, y + 40., width, 28.),
        text(22., 600, 0.),
        WHITE,
        false,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Preferences;

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

}
