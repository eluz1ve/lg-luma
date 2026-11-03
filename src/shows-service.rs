//! The TV Shows worker: CUB's feed of series off the SDL thread, newest request wins, and posters
//! one at a time on their own worker.
use crate::app_artwork::Artwork;
use crate::shows::{Command, PAGES, Show, parse_feed, poster_urls};
use serde_json::Value;
use std::{
    io::Read,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

/// A page of twenty series is about fifty kilobytes.
const LIMIT: usize = 1024 * 1024;
const POSTER_LIMIT: usize = 1024 * 1024;
/// Posters are drawn about 256 px wide; a little more keeps them sharp when the card lifts.
const POSTER_WIDTH: u32 = 320;

#[derive(Clone)]
pub struct Client {
    agent: ureq::Agent,
}
impl Client {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(8)))
            .max_redirects(0)
            .proxy(None)
            .build()
            .new_agent();
        Self { agent }
    }
    fn get(&self, url: &str, limit: usize) -> Result<Vec<u8>, String> {
        let mut response = self
            .agent
            .get(url)
            .header("User-Agent", "LumaNative")
            .call()
            .map_err(|_| "CUB is unavailable.")?;
        if !response.status().is_success() {
            return Err("CUB is unavailable.".into());
        }
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "The CUB reply could not be read")?;
        if bytes.len() > limit {
            return Err("The CUB reply is too large".into());
        }
        Ok(bytes)
    }
    /// The feed's first pages, from the first of CUB's hosts that answers.
    pub fn shows(&self, command: &Command) -> Result<Vec<Show>, String> {
        let mut error = None;
        for host in crate::cub::HOSTS {
            let pages: Result<Vec<Value>, String> = (1..=PAGES)
                .map(|page| {
                    let bytes = self.get(&feed_url(host, page), LIMIT)?;
                    serde_json::from_slice(&bytes)
                        .map_err(|_| "CUB returned unreadable data".into())
                })
                .collect();
            match pages {
                Ok(pages) => return Ok(parse_feed(&pages, command.date)),
                Err(message) => error = Some(message),
            }
        }
        Err(error.unwrap_or_else(|| "CUB is unavailable.".into()))
    }
    /// A poster from TMDB (or its mirror), decoded and scaled on this worker.
    pub fn poster(&self, path: &str) -> Option<Arc<Artwork>> {
        let bytes = poster_urls(path)
            .iter()
            .find_map(|url| self.get(url, POSTER_LIMIT).ok())?;
        let mut reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .ok()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(2048);
        limits.max_image_height = Some(2048);
        limits.max_alloc = Some(32 * 1024 * 1024);
        reader.limits(limits);
        let image = reader.decode().ok()?;
        let pixels = if image.width() > POSTER_WIDTH {
            image.thumbnail(POSTER_WIDTH, POSTER_WIDTH * 2).into_rgba8()
        } else {
            image.into_rgba8()
        };
        Some(Arc::new(Artwork {
            key: format!("show-poster:{path}"),
            pixels,
            backdrop: [[0; 3]; 2],
        }))
    }
}

/// A page of the series Lampa's viewers watch now, as Lampa itself asks for it.
fn feed_url(host: &str, page: u32) -> String {
    format!("https://tmdb.{host}/?cat=tv&sort=now_playing&page={page}&email=")
}

pub enum Response {
    Shows {
        id: u64,
        result: Result<Vec<Show>, String>,
    },
    Poster {
        path: String,
        image: Option<Arc<Artwork>>,
    },
}

pub struct Service {
    sender: Option<mpsc::Sender<Command>>,
    posters: Option<mpsc::Sender<String>>,
    generation: Arc<AtomicU64>,
    pub receiver: mpsc::Receiver<Response>,
}
impl Service {
    pub fn new(client: Client) -> Self {
        let (sender, jobs) = mpsc::channel::<Command>();
        let (responses, receiver) = mpsc::channel();
        let generation = Arc::new(AtomicU64::new(0));
        let current = generation.clone();
        let (posters, wanted) = mpsc::channel::<String>();
        let poster_client = client.clone();
        let poster_responses = responses.clone();
        thread::spawn(move || {
            for path in wanted {
                let image = poster_client.poster(&path);
                if poster_responses
                    .send(Response::Poster { path, image })
                    .is_err()
                {
                    break;
                }
            }
        });
        thread::spawn(move || {
            while let Ok(mut job) = jobs.recv() {
                // Only the newest waiting request is worth answering.
                while let Ok(newer) = jobs.try_recv() {
                    job = newer;
                }
                if current.load(Ordering::SeqCst) != job.id {
                    continue;
                }
                let result = client.shows(&job);
                if current.load(Ordering::SeqCst) == job.id
                    && responses
                        .send(Response::Shows { id: job.id, result })
                        .is_err()
                {
                    break;
                }
            }
        });
        Self {
            sender: Some(sender),
            posters: Some(posters),
            generation,
            receiver,
        }
    }
    /// Requests older than `id` are dropped before or after they run.
    pub fn generation(&self, id: u64) {
        self.generation.store(id, Ordering::SeqCst);
    }
    pub fn poster(&self, path: String) {
        if let Some(posters) = &self.posters {
            let _ = posters.send(path);
        }
    }
    pub fn send(&self, command: Command) {
        self.generation(command.id);
        if let Some(sender) = &self.sender {
            let _ = sender.send(command);
        }
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.generation.store(u64::MAX, Ordering::SeqCst);
        self.sender.take();
        self.posters.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_feed_is_asked_for_as_lampa_asks_for_it() {
        assert_eq!(
            feed_url("cub.red", 2),
            "https://tmdb.cub.red/?cat=tv&sort=now_playing&page=2&email="
        );
    }
}
