//! The TV Shows worker: TVmaze's schedules off the SDL thread, newest request wins, and posters
//! one at a time on their own worker.
use crate::app_artwork::Artwork;
use crate::shows::{Command, POSTER_ORIGIN, Show, parse_schedules};
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

const API: &str = "https://api.tvmaze.com";
/// A day's streaming schedule is a few hundred kilobytes.
const LIMIT: usize = 2 * 1024 * 1024;
const POSTER_LIMIT: usize = 1024 * 1024;
/// Posters are drawn about 256 px wide; a little more keeps them sharp when the card lifts.
const POSTER_WIDTH: u32 = 320;
/// Days of the streaming schedule read, up to today; tomorrow's is read too, since west of UTC
/// this evening's episodes are on tomorrow's.
const DAYS: i64 = 7;

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
    fn get(&self, url: &str, query: &[(&str, &str)], limit: usize) -> Result<Vec<u8>, String> {
        let mut request = self.agent.get(url).header("User-Agent", "LumaNative");
        for (key, value) in query {
            request = request.query(key, value);
        }
        let mut response = request.call().map_err(|_| "TVmaze is unavailable.")?;
        if !response.status().is_success() {
            return Err("TVmaze is unavailable.".into());
        }
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "The TVmaze reply could not be read")?;
        if bytes.len() > limit {
            return Err("The TVmaze reply is too large".into());
        }
        Ok(bytes)
    }
    /// The week's streaming schedule up to tomorrow, and today's TV in `country`.
    pub fn shows(&self, command: &Command) -> Result<Vec<Show>, String> {
        let mut replies = Vec::new();
        let mut error = None;
        for (url, query) in requests(command) {
            let query: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
            match self.get(&url, &query, LIMIT).and_then(|bytes| {
                serde_json::from_slice::<Value>(&bytes)
                    .map_err(|_| "TVmaze returned unreadable data".into())
            }) {
                Ok(value) => replies.push(value),
                Err(message) => error = Some(message),
            }
        }
        if replies.is_empty() {
            return Err(error.unwrap_or_else(|| "TVmaze is unavailable.".into()));
        }
        Ok(parse_schedules(&replies, &command.installed, command.until))
    }
    /// A poster from TVmaze's image CDN, decoded and scaled on this worker.
    pub fn poster(&self, url: &str) -> Option<Arc<Artwork>> {
        if !url.starts_with(POSTER_ORIGIN) {
            return None;
        }
        let bytes = self.get(url, &[], POSTER_LIMIT).ok()?;
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
            key: format!("show-poster:{url}"),
            pixels,
            backdrop: [[0; 3]; 2],
        }))
    }
}

/// What a refresh asks TVmaze for: the streaming schedule from the TV's tomorrow back over the
/// week, and the TV schedule of the TV's today in its country. Dates are the TV's own, so evening
/// shows are not lost where the UTC date is already another.
fn requests(command: &Command) -> Vec<(String, Vec<(&'static str, String)>)> {
    let mut requests: Vec<_> = (-1..DAYS)
        .map(|day| {
            let date = (command.date - time::Duration::days(day)).to_string();
            (format!("{API}/schedule/web"), vec![("date", date)])
        })
        .collect();
    requests.push((
        format!("{API}/schedule"),
        vec![
            ("country", command.country.clone()),
            ("date", command.date.to_string()),
        ],
    ));
    requests
}

pub enum Response {
    Shows {
        id: u64,
        result: Result<Vec<Show>, String>,
    },
    Poster {
        url: String,
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
            for url in wanted {
                let image = poster_client.poster(&url);
                if poster_responses
                    .send(Response::Poster { url, image })
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
    pub fn poster(&self, url: String) {
        if let Some(posters) = &self.posters {
            let _ = posters.send(url);
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
    fn the_schedules_are_asked_for_the_tvs_local_days() {
        let command = Command {
            id: 1,
            country: "US".into(),
            installed: Vec::new(),
            date: time::Date::from_calendar_date(2026, time::Month::November, 1).unwrap(),
            until: 0,
        };
        let asked = requests(&command);
        let dates = |path: &str| -> Vec<String> {
            asked
                .iter()
                .filter(|(url, _)| url.ends_with(path))
                .map(|(_, query)| query.iter().find(|(k, _)| *k == "date").unwrap().1.clone())
                .collect()
        };
        assert_eq!(dates("/schedule"), ["2026-11-01"]);
        let web = dates("/schedule/web");
        assert_eq!(web.first().map(String::as_str), Some("2026-11-02"));
        assert_eq!(web.last().map(String::as_str), Some("2026-10-26"));
    }
}
