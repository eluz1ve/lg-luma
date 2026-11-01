//! Apple TV wallpapers: stills of the places in Apple TV's screensaver (assets/apple-tv.json, made
//! by tools/apple-tv-wallpapers.py from Apple's catalog), downloaded from Apple as they are shown.
//! The background moves to the next place, in a shuffled order, every fifteen minutes; only the
//! stills around the one shown stay on the TV.
use crate::{
    i18n::Language,
    wallpapers::{Wallpaper, create_directory, decode, jpeg, shade, write_new},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{OnceLock, mpsc},
    thread,
    time::Duration,
};

/// The gallery entry; its drawn key is `aerials#<place>`.
pub const ID: &str = "aerials";
pub const LABEL: &str = "Apple TV";
pub const INTERVAL: u64 = crate::wallpaper_cycle::INTERVAL;
/// Apple's stills are about 400 KB.
const LIMIT: usize = 2 * 1024 * 1024;
/// A failed download is tried again this long after.
const RETRY: Duration = Duration::from_secs(120);
/// Apple's servers chain to Apple Root CA, which the Mozilla roots ureq trusts leave out. Stills
/// are fetched trusting it alone (SHA-256 B0:B1:73:0E:CB:C7:FF:45:…:B5:A1:10:01:F0:24).
const APPLE_ROOT_CA: &[u8] = include_bytes!("../assets/apple-root-ca.der");

#[derive(Deserialize)]
pub struct Catalog {
    snapshots: String,
    pub pictures: Vec<Picture>,
}

#[derive(Deserialize)]
pub struct Picture {
    pub id: String,
    name: BTreeMap<String, String>,
}

impl Picture {
    pub fn name(&self, language: Language) -> &str {
        self.name
            .get(language.code())
            .or_else(|| self.name.get("en"))
            .map_or(self.id.as_str(), String::as_str)
    }
}

impl Catalog {
    pub fn snapshot(&self, id: &str) -> String {
        format!("{}{id}.jpg", self.snapshots)
    }
}

pub fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/apple-tv.json"))
            .expect("assets/apple-tv.json is valid")
    })
}

/// Apple's shot IDs, which name the folders stills are kept in.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

/// Where the background is in its shuffled order of places.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Rotation {
    pub seed: u64,
    pub index: usize,
    /// Unix seconds the place shown was changed.
    pub changed: u64,
}

impl Rotation {
    pub fn start(now: u64) -> Self {
        Self {
            seed: mix(now),
            index: 0,
            changed: now,
        }
    }

    /// The place shown, as an index into the catalog's pictures.
    pub fn current(&self, count: usize) -> usize {
        if count == 0 {
            return 0;
        }
        order(self.seed, count)[self.index % count]
    }

    pub fn next(&self, count: usize) -> usize {
        self.following(count).current(count)
    }

    pub fn advanced(&self, count: usize, now: u64) -> Self {
        Self {
            changed: now,
            ..self.following(count)
        }
    }

    pub fn due(&self, now: u64) -> bool {
        now.saturating_sub(self.changed) >= INTERVAL
    }

    /// The next place in this round, or the first of a new order that does not begin with the
    /// place just shown.
    fn following(&self, count: usize) -> Self {
        if self.index + 1 < count {
            return Self {
                index: self.index + 1,
                ..*self
            };
        }
        let last = self.current(count);
        let mut seed = mix(self.seed);
        while count > 1 && order(seed, count)[0] == last {
            seed = mix(seed);
        }
        Self {
            seed,
            index: 0,
            changed: self.changed,
        }
    }
}

/// splitmix64.
fn mix(value: u64) -> u64 {
    let mut z = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A Fisher–Yates shuffle of the places, by seed.
fn order(seed: u64, count: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..count).collect();
    let mut state = seed;
    for i in (1..count).rev() {
        state = mix(state);
        order.swap(i, (state % (i as u64 + 1)) as usize);
    }
    order
}

pub enum Event {
    /// A still is on the TV.
    Ready(Wallpaper),
}

pub struct Service {
    wanted: mpsc::Sender<Vec<String>>,
    pub receiver: mpsc::Receiver<Event>,
}

impl Service {
    /// Offline, it reports the stills already on the TV and fetches nothing.
    pub fn new(preferences: &Path, offline: bool) -> Result<Self, String> {
        let directory = preferences
            .parent()
            .unwrap_or(Path::new("."))
            .join("aerials");
        let (wanted, wishes) = mpsc::channel();
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name("luma-aerials".into())
            .spawn(move || worker(&directory, offline, &wishes, &sender))
            .map_err(|e| format!("Could not start Apple TV wallpaper worker: {e}"))?;
        Ok(Self { wanted, receiver })
    }

    /// Keeps these places' stills on the TV, downloading any missing, and removes the rest.
    pub fn want(&self, ids: Vec<String>) {
        let _ = self.wanted.send(ids);
    }
}

fn worker(
    directory: &Path,
    offline: bool,
    wishes: &mpsc::Receiver<Vec<String>>,
    sender: &mpsc::Sender<Event>,
) {
    let agent = agent();
    let fetch = |url: &str| {
        if offline {
            Err("offline".to_owned())
        } else {
            get(&agent, url)
        }
    };
    let mut wanted = Vec::new();
    let mut retry = false;
    loop {
        let wish = if retry {
            wishes
                .recv_timeout(RETRY)
                .map_err(|error| error == mpsc::RecvTimeoutError::Disconnected)
        } else {
            wishes.recv().map_err(|_| true)
        };
        match wish {
            Ok(ids) => wanted = ids,
            Err(true) => return,
            Err(false) => {}
        }
        while let Ok(ids) = wishes.try_recv() {
            wanted = ids;
        }
        retry = false;
        if let Err(error) = create_directory(directory) {
            eprintln!("Luma Apple TV wallpapers: {error}");
            retry = true;
            continue;
        }
        for id in &wanted {
            match ensure(directory, id, fetch) {
                Ok(still) => {
                    if sender.send(Event::Ready(still)).is_err() {
                        return;
                    }
                }
                Err(_) if offline => {}
                Err(error) => {
                    eprintln!("Luma Apple TV wallpaper {id}: {error}");
                    retry = true;
                }
            }
        }
        if !offline {
            prune(directory, &wanted);
        }
    }
}

fn agent() -> ureq::Agent {
    use ureq::tls::{Certificate, RootCerts, TlsConfig};
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .max_redirects(0)
        .proxy(None)
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::new_with_certs(&[Certificate::from_der(
                    APPLE_ROOT_CA,
                )]))
                .build(),
        )
        .build()
        .new_agent()
}

fn get(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, String> {
    let mut response = agent
        .get(url)
        .header("User-Agent", "LumaNative")
        .call()
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > LIMIT {
        return Err("The still is too large".into());
    }
    Ok(bytes)
}

fn still(directory: &Path, id: &str) -> Wallpaper {
    Wallpaper {
        id: id.to_owned(),
        directory: directory.join(id),
        has_original: true,
        frames: Vec::new(),
    }
}

/// A place's still on the TV, fetched from Apple if it is not there yet: Apple's file as the plain
/// picture, a shaded copy like every wallpaper's, and a thumbnail for the gallery.
pub(crate) fn ensure(
    directory: &Path,
    id: &str,
    fetch: impl Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<Wallpaper, String> {
    if !valid_id(id) || !catalog().pictures.iter().any(|picture| picture.id == id) {
        return Err("Not an Apple TV place".into());
    }
    let kept = still(directory, id);
    let file = |path: String| fs::symlink_metadata(path).is_ok_and(|m| m.is_file());
    if file(kept.image()) && file(kept.unshaded_image()) && file(kept.thumbnail()) {
        return Ok(kept);
    }
    let original = fetch(&catalog().snapshot(id))?;
    let mut image = decode(&original)?;
    let thumbnail =
        image::imageops::resize(&image, 480, 270, image::imageops::FilterType::Triangle);
    shade(&mut image);
    let shaded = jpeg(&image, 94)?;
    let thumbnail = jpeg(&thumbnail, 82)?;
    // Written aside and moved into place, so a still is never seen half written.
    let partial = directory.join(format!(".{id}.partial"));
    let _ = fs::remove_dir_all(&partial);
    let written = create_directory(&partial)
        .and_then(|()| write_new(&partial.join("wallpaper.jpg"), &shaded))
        .and_then(|()| write_new(&partial.join("original.jpg"), &original))
        .and_then(|()| write_new(&partial.join("thumbnail.jpg"), &thumbnail))
        .and_then(|()| {
            let _ = fs::remove_dir_all(&kept.directory);
            fs::rename(&partial, &kept.directory).map_err(|e| e.to_string())
        });
    if let Err(error) = written {
        let _ = fs::remove_dir_all(&partial);
        return Err(error);
    }
    Ok(kept)
}

/// Removes every still but these; the folder holds nothing else.
pub(crate) fn prune(directory: &Path, keep: &[String]) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        if keep.iter().any(|id| entry.file_name() == id.as_str()) {
            continue;
        }
        let path: PathBuf = entry.path();
        let _ = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(&path),
            _ => fs::remove_file(&path),
        };
    }
}

#[cfg(test)]
#[path = "aerials-tests.rs"]
mod tests;
