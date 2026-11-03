//! The CUB worker: sign-in polls and account reads off the SDL thread, each request tried on
//! cub.red's mirrors in turn from the one that answered last; posters on their own worker.
use crate::app_artwork::Artwork;
use crate::cub::{self, Account, CLIENT_ID, Failure, Grant, HOSTS, Login, Profile, SCOPE, Tokens};
use serde_json::Value;
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

static LOG: OnceLock<PathBuf> = OnceLock::new();

/// Diagnostics: CUB's answers (statuses and error codes, never tokens) are appended to `path`.
pub fn log_to(path: PathBuf) {
    let _ = LOG.set(path);
}

pub fn log(line: impl AsRef<str>) {
    let Some(path) = LOG.get() else {
        return;
    };
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        let _ = writeln!(file, "{now} {}", line.as_ref());
    }
}

/// A month of releases or a page of history is well under this.
const LIMIT: usize = 2 * 1024 * 1024;
const POSTER_LIMIT: usize = 512 * 1024;
/// Today's lines show posters small; this keeps them sharp when focused.
const POSTER_WIDTH: u32 = 160;

#[derive(Clone)]
pub struct Client {
    agent: ureq::Agent,
    /// The mirror that answered last.
    host: Arc<AtomicUsize>,
}

impl Client {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .max_redirects(0)
            .proxy(None)
            // OAuth answers "authorization_pending" and the like with 400.
            .http_status_as_error(false)
            .build()
            .new_agent();
        Self {
            agent,
            host: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Runs `request` against each mirror until one answers, and reads its JSON.
    fn call(
        &self,
        path: &str,
        request: impl Fn(&str) -> Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> Result<(u16, Value), String> {
        let first = self.host.load(Ordering::Relaxed);
        for offset in 0..HOSTS.len() {
            let index = (first + offset) % HOSTS.len();
            let mut response = match request(HOSTS[index]) {
                Ok(response) => response,
                Err(error) => {
                    log(format!("{} {path}: {error}", HOSTS[index]));
                    continue;
                }
            };
            self.host.store(index, Ordering::Relaxed);
            let status = response.status().as_u16();
            let mut bytes = Vec::new();
            response
                .body_mut()
                .as_reader()
                .take(LIMIT as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "The CUB reply could not be read")?;
            if bytes.len() > LIMIT {
                return Err("The CUB reply is too large".into());
            }
            let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            log(format!(
                "{} {path}: HTTP {status} error={} code={} bytes={}",
                HOSTS[index],
                value.get("error").unwrap_or(&Value::Null),
                value.get("code").unwrap_or(&Value::Null),
                bytes.len()
            ));
            return Ok((status, value));
        }
        Err("CUB is unavailable.".into())
    }

    fn post(&self, path: &str, form: &[(&str, &str)]) -> Result<(u16, Value), String> {
        self.call(path, |host| {
            self.agent
                .post(format!("https://{host}/api/{path}"))
                .header("User-Agent", "LumaNative")
                .header("Accept", "application/json")
                .send_form(form.iter().copied())
        })
    }

    fn get(
        &self,
        path: &str,
        query: &[(&str, &str)],
        tokens: &Tokens,
        profile: Option<&str>,
    ) -> Result<Value, Failure> {
        let (status, value) = self
            .call(path, |host| {
                let mut request = self
                    .agent
                    .get(format!("https://{host}/api/{path}"))
                    .header("User-Agent", "LumaNative")
                    .header("Accept", "application/json")
                    .header("Authorization", &format!("Bearer {}", tokens.access));
                if let Some(profile) = profile {
                    request = request.header("profile", profile);
                }
                for (key, value) in query {
                    request = request.query(key, value);
                }
                request.call()
            })
            .map_err(Failure::Unavailable)?;
        // A disconnected app is code 700; an unknown token is 401.
        if status == 401 || value.get("code").and_then(Value::as_i64) == Some(700) {
            return Err(Failure::SignedOut);
        }
        if !(200..300).contains(&status) {
            return Err(Failure::Unavailable(format!("CUB answered {status}")));
        }
        Ok(value)
    }

    pub fn sign_in(&self, now: i64) -> Result<Login, String> {
        let (_, reply) = self.post(
            "oauth/device/code",
            &[("client_id", CLIENT_ID), ("scope", SCOPE)],
        )?;
        cub::parse_login(&reply, now).ok_or_else(|| "CUB gave no sign-in code".into())
    }

    pub fn poll(&self, device_code: &str, now: i64) -> Grant {
        match self.post(
            "oauth/token",
            &[
                ("client_id", CLIENT_ID),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", device_code),
            ],
        ) {
            Ok((_, reply)) => cub::parse_grant(&reply, now),
            Err(error) => Grant::Failed(error),
        }
    }

    fn refresh(&self, tokens: &Tokens, now: i64) -> Result<Tokens, Failure> {
        let (_, reply) = self
            .post(
                "oauth/token",
                &[
                    ("client_id", CLIENT_ID),
                    ("grant_type", "refresh_token"),
                    ("refresh_token", &tokens.refresh),
                ],
            )
            .map_err(Failure::Unavailable)?;
        match cub::parse_grant(&reply, now) {
            Grant::Tokens(tokens) => Ok(tokens),
            Grant::Revoked | Grant::Denied => Err(Failure::SignedOut),
            _ => Err(Failure::Unavailable("CUB did not renew the sign-in".into())),
        }
    }

    /// The profile, its releases over `range` and its history; refreshed tokens come back
    /// with the result, to be kept even when a later step fails.
    pub fn read(
        &self,
        tokens: &Tokens,
        profile: Option<&Profile>,
        range: (time::Date, time::Date),
        now: i64,
    ) -> (Option<Tokens>, Result<Account, Failure>) {
        let refreshed = if cub::needs_refresh(tokens, now) {
            match self.refresh(tokens, now) {
                Ok(tokens) => Some(tokens),
                Err(failure) => return (None, Err(failure)),
            }
        } else {
            None
        };
        let tokens = refreshed.as_ref().unwrap_or(tokens);
        let result = (|| {
            let profile = match profile {
                Some(profile) => profile.clone(),
                None => cub::parse_profile(&self.get("profiles/all", &[], tokens, None)?)
                    .ok_or_else(|| Failure::Unavailable("CUB has no profile".into()))?,
            };
            let (from, to) = (range.0.to_string(), range.1.to_string());
            let calendar = self.get(
                "calendar/list",
                &[("from", &from), ("to", &to)],
                tokens,
                Some(&profile.id),
            )?;
            let history = self.get(
                "history/list",
                &[("group", "card"), ("page", "1")],
                tokens,
                Some(&profile.id),
            )?;
            Ok(Account {
                releases: cub::parse_calendar(&calendar),
                watched: cub::parse_history(&history),
                profile,
            })
        })();
        (refreshed, result)
    }

    /// A card's poster from TMDB (or its mirror), decoded and scaled on this worker.
    pub fn poster(&self, path: &str) -> Option<Arc<Artwork>> {
        cub::poster_urls(path).into_iter().find_map(|url| {
            let mut response = self
                .agent
                .get(&url)
                .header("User-Agent", "LumaNative")
                .call()
                .ok()?;
            if !response.status().is_success() {
                return None;
            }
            let mut bytes = Vec::new();
            response
                .body_mut()
                .as_reader()
                .take(POSTER_LIMIT as u64 + 1)
                .read_to_end(&mut bytes)
                .ok()?;
            if bytes.len() > POSTER_LIMIT {
                return None;
            }
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
                key: format!("cub-poster:{path}"),
                pixels,
                backdrop: [[0; 3]; 2],
            }))
        })
    }
}

pub enum Command {
    SignIn,
    Poll {
        device_code: String,
    },
    Read {
        id: u64,
        tokens: Tokens,
        profile: Option<Profile>,
        range: (time::Date, time::Date),
    },
}

pub enum Response {
    Login(Result<Login, String>),
    Grant {
        device_code: String,
        grant: Grant,
    },
    Account {
        id: u64,
        refreshed: Option<Tokens>,
        result: Result<Account, Failure>,
    },
    Poster {
        path: String,
        image: Option<Arc<Artwork>>,
    },
}

pub struct Service {
    sender: Option<mpsc::Sender<Command>>,
    posters: Option<mpsc::Sender<String>>,
    pub receiver: mpsc::Receiver<Response>,
}

impl Service {
    pub fn new(client: Client) -> Self {
        let (sender, jobs) = mpsc::channel::<Command>();
        let (responses, receiver) = mpsc::channel();
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
            for job in jobs {
                let now = time::OffsetDateTime::now_utc().unix_timestamp();
                let response = match job {
                    Command::SignIn => Response::Login(client.sign_in(now)),
                    Command::Poll { device_code } => Response::Grant {
                        grant: client.poll(&device_code, now),
                        device_code,
                    },
                    Command::Read {
                        id,
                        tokens,
                        profile,
                        range,
                    } => {
                        let (refreshed, result) =
                            client.read(&tokens, profile.as_ref(), range, now);
                        Response::Account {
                            id,
                            refreshed,
                            result,
                        }
                    }
                };
                if responses.send(response).is_err() {
                    break;
                }
            }
        });
        Self {
            sender: Some(sender),
            posters: Some(posters),
            receiver,
        }
    }

    pub fn send(&self, command: Command) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(command);
        }
    }

    pub fn poster(&self, path: String) {
        if let Some(posters) = &self.posters {
            let _ = posters.send(path);
        }
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.sender.take();
        self.posters.take();
    }
}
