//! On-TV frame timing, for measuring where no debugger reaches: while a `perf` file sits beside
//! the preferences, each second that drew frames appends one line to `perf.log` there.
use crate::diagnostics::Frame;
use std::{
    fs::File,
    io::Write,
    path::Path,
    time::{Duration, Instant},
};

/// The log stops growing here.
const LIMIT: u64 = 4 * 1024 * 1024;
/// A longer pause between frames is idle time, not a slow frame.
const IDLE: Duration = Duration::from_millis(250);

struct Sample {
    gap: f64,
    compose: f64,
    render: f64,
    swap: f64,
    image: f64,
    redrawn: f64,
}

pub struct PerfLog {
    file: File,
    written: u64,
    started: Instant,
    window: Instant,
    label: String,
    last: Option<Instant>,
    samples: Vec<Sample>,
}

impl PerfLog {
    pub fn open(directory: &Path) -> Option<Self> {
        if !directory.join("perf").exists() {
            return None;
        }
        let mut options = std::fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o664);
        let file = options.open(directory.join("perf.log")).ok()?;
        let written = file.metadata().map_or(0, |metadata| metadata.len());
        let now = Instant::now();
        Some(Self {
            file,
            written,
            started: now,
            window: now,
            label: String::new(),
            last: None,
            samples: Vec::new(),
        })
    }

    /// One presented frame; a new second or screen writes out the frames before it.
    pub fn record(&mut self, now: Instant, label: &str, frame: &Frame) {
        if now.duration_since(self.window) >= Duration::from_secs(1) || label != self.label {
            self.flush(now);
            self.label = label.to_owned();
        }
        let gap = self
            .last
            .map(|last| now.duration_since(last))
            .filter(|gap| *gap < IDLE)
            .map(|gap| gap.as_secs_f64() * 1000.);
        self.last = Some(now);
        if let Some(gap) = gap {
            self.samples.push(Sample {
                gap,
                compose: frame.compose_ms,
                render: frame.render_ms,
                swap: frame.swap_ms,
                image: frame.image_ms,
                redrawn: frame.redrawn,
            });
        }
    }

    fn flush(&mut self, now: Instant) {
        self.window = now;
        if self.samples.is_empty() || self.written > LIMIT {
            self.samples.clear();
            return;
        }
        let line = line(
            now.duration_since(self.started).as_secs_f64(),
            &self.label,
            &self.samples,
        );
        self.samples.clear();
        if self.file.write_all(line.as_bytes()).is_ok() {
            self.written += line.len() as u64;
        }
    }
}

fn line(seconds: f64, label: &str, samples: &[Sample]) -> String {
    let stat = |value: fn(&Sample) -> f64| {
        let mean = samples.iter().map(value).sum::<f64>() / samples.len() as f64;
        let peak = samples.iter().map(value).fold(0., f64::max);
        format!("{mean:.1}/{peak:.1}")
    };
    format!(
        "t={seconds:.1} {label} n={} gap {} compose {} render {} swap {} image {:.1} redrawn {:.2}\n",
        samples.len(),
        stat(|s| s.gap),
        stat(|s| s.compose),
        stat(|s| s.render),
        stat(|s| s.swap),
        samples.iter().map(|s| s.image).fold(0., f64::max),
        samples.iter().map(|s| s.redrawn).sum::<f64>() / samples.len() as f64,
    )
}

#[cfg(test)]
#[path = "perf-log-tests.rs"]
mod tests;
