//! Dynamic wallpapers: a set of frames that follow the sun, like macOS's Dynamic Desktop. A pack
//! (`.lumadyn`, made from a Dynamic Desktop picture by `tools/lumadyn.swift`) is uploaded like any
//! wallpaper; Luma shows the frame whose sun position, or time of day, is nearest the real one at
//! the weather location and fades to the next as the day moves on.
use serde::{Deserialize, Serialize};

/// Frames a pack may hold; macOS's have up to 16.
pub const MAX_FRAMES: usize = 32;
/// How long a change of frame fades, in seconds: slow enough to pass for daylight changing.
pub const FADE: f32 = 6.;
const MAGIC: &[u8] = b"LUMADYN1\n";

/// Where a frame belongs in the day: the sun's altitude and azimuth in degrees, or a time of day
/// from 0 (midnight) to 1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct Frame {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub altitude: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub azimuth: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<f32>,
}

#[derive(Deserialize)]
struct Header {
    frames: Vec<HeaderFrame>,
}

#[derive(Deserialize)]
struct HeaderFrame {
    #[serde(flatten)]
    frame: Frame,
    length: usize,
}

/// Splits a pack into its frames' positions and image bytes: `LUMADYN1`, a line of JSON
/// (`{"frames":[{"altitude":…,"azimuth":…,"length":…},…]}`), then the images back to back.
pub fn parse(bytes: &[u8]) -> Result<Vec<(Frame, &[u8])>, String> {
    let invalid = || "Not a Luma dynamic wallpaper".to_owned();
    let rest = bytes.strip_prefix(MAGIC).ok_or_else(invalid)?;
    let end = rest
        .iter()
        .take(64 * 1024)
        .position(|b| *b == b'\n')
        .ok_or_else(invalid)?;
    let header: Header = serde_json::from_slice(&rest[..end]).map_err(|_| invalid())?;
    if header.frames.is_empty() || header.frames.len() > MAX_FRAMES {
        return Err(format!("A dynamic wallpaper has 1 to {MAX_FRAMES} frames"));
    }
    let mut images = &rest[end + 1..];
    let mut frames = Vec::new();
    for item in header.frames {
        let valid = match (item.frame.altitude, item.frame.azimuth, item.frame.time) {
            (Some(altitude), Some(azimuth), _) => {
                (-90. ..=90.).contains(&altitude) && (0. ..=360.).contains(&azimuth)
            }
            (None, None, Some(time)) => (0. ..=1.).contains(&time),
            _ => false,
        };
        if !valid || item.length == 0 || item.length > images.len() {
            return Err(invalid());
        }
        let (image, next) = images.split_at(item.length);
        frames.push((item.frame, image));
        images = next;
    }
    if !images.is_empty() {
        return Err(invalid());
    }
    Ok(frames)
}

/// The sun's altitude and azimuth (degrees, azimuth clockwise from north) at a place and time,
/// from NOAA's approximations: within a degree, plenty to pick a wallpaper frame.
pub fn sun(latitude: f64, longitude: f64, unix: i64) -> (f32, f32) {
    let days = unix as f64 / 86400. - 10957.5; // since 2000-01-01 12:00 UTC
    let mean_longitude = (280.460 + 0.985_647_4 * days).rem_euclid(360.);
    let anomaly = (357.528 + 0.985_600_3 * days).rem_euclid(360.).to_radians();
    let ecliptic =
        (mean_longitude + 1.915 * anomaly.sin() + 0.020 * (2. * anomaly).sin()).to_radians();
    let obliquity = (23.439 - 0.000_000_4 * days).to_radians();
    let declination = (obliquity.sin() * ecliptic.sin()).asin();
    let ascension = (obliquity.cos() * ecliptic.sin()).atan2(ecliptic.cos());
    let sidereal = (280.460_618_37 + 360.985_647_366_29 * days + longitude).rem_euclid(360.);
    let hour = (sidereal.to_radians() - ascension).rem_euclid(std::f64::consts::TAU);
    let latitude = latitude.to_radians();
    let altitude = (latitude.sin() * declination.sin()
        + latitude.cos() * declination.cos() * hour.cos())
    .asin();
    let azimuth = (-hour.sin())
        .atan2(declination.tan() * latitude.cos() - latitude.sin() * hour.cos())
        .rem_euclid(std::f64::consts::TAU);
    (altitude.to_degrees() as f32, azimuth.to_degrees() as f32)
}

/// The frame for this moment: by the sun at (latitude, longitude) for frames placed by the sun,
/// else by the local time of day (`day`, 0–1).
pub fn choose(frames: &[Frame], sun: (f32, f32), day: f32) -> Option<usize> {
    let by_sun = frames
        .iter()
        .enumerate()
        .filter_map(|(index, frame)| Some((index, frame.altitude?, frame.azimuth?)))
        .map(|(index, altitude, azimuth)| {
            let turn = (azimuth - sun.1).abs() % 360.;
            // Altitude first; the azimuth tells morning from evening at the same height.
            (
                index,
                (altitude - sun.0).abs() + 0.25 * turn.min(360. - turn),
            )
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index);
    by_sun.or_else(|| {
        // The latest frame that has started today, else the last one from yesterday.
        let timed = frames
            .iter()
            .enumerate()
            .filter_map(|(index, frame)| Some((index, frame.time?)));
        timed
            .clone()
            .filter(|(_, time)| *time <= day)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .or_else(|| timed.max_by(|a, b| a.1.total_cmp(&b.1)))
            .map(|(index, _)| index)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(header: &str, images: &[&[u8]]) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(header.as_bytes());
        bytes.push(b'\n');
        for image in images {
            bytes.extend_from_slice(image);
        }
        bytes
    }

    #[test]
    fn packs_split_into_frames_and_reject_anything_malformed() {
        let bytes = pack(
            r#"{"frames":[{"altitude":10,"azimuth":90,"length":3},{"time":0.5,"length":2}]}"#,
            &[b"abc", b"de"],
        );
        let frames = parse(&bytes).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].1, b"abc");
        assert_eq!(frames[1].0.time, Some(0.5));
        for bad in [
            pack(
                r#"{"frames":[{"altitude":10,"azimuth":90,"length":4}]}"#,
                &[b"abc"],
            ),
            pack(
                r#"{"frames":[{"altitude":10,"azimuth":90,"length":2}]}"#,
                &[b"abc"],
            ),
            pack(
                r#"{"frames":[{"altitude":99,"azimuth":90,"length":3}]}"#,
                &[b"abc"],
            ),
            pack(r#"{"frames":[{"length":3}]}"#, &[b"abc"]),
            pack(r#"{"frames":[]}"#, &[]),
            b"JPEG".to_vec(),
        ] {
            assert!(parse(&bad).is_err());
        }
    }

    #[test]
    fn the_sun_is_where_it_should_be() {
        // Kyiv, 2026-06-21: high around 13:00 local (10:00 UTC), below the horizon at midnight.
        let noon = sun(50.45, 30.52, 1_782_036_000);
        assert!((noon.0 - 63.).abs() < 1.5, "{noon:?}");
        assert!((noon.1 - 180.).abs() < 12., "{noon:?}");
        let midnight = sun(50.45, 30.52, 1_782_036_000 + 12 * 3600);
        assert!(midnight.0 < -10.);
        // Morning sun is in the east, evening sun in the west.
        assert!(sun(50.45, 30.52, 1_782_036_000 - 5 * 3600).1 < 120.);
        assert!(sun(50.45, 30.52, 1_782_036_000 + 6 * 3600).1 > 240.);
    }

    #[test]
    fn frames_are_chosen_by_the_sun_or_by_the_time() {
        let solar = [
            Frame {
                altitude: Some(-30.),
                azimuth: Some(0.),
                time: None,
            },
            Frame {
                altitude: Some(5.),
                azimuth: Some(90.),
                time: None,
            },
            Frame {
                altitude: Some(60.),
                azimuth: Some(180.),
                time: None,
            },
            Frame {
                altitude: Some(5.),
                azimuth: Some(270.),
                time: None,
            },
        ];
        assert_eq!(choose(&solar, (55., 170.), 0.5), Some(2));
        assert_eq!(choose(&solar, (3., 100.), 0.3), Some(1), "morning");
        assert_eq!(choose(&solar, (3., 260.), 0.8), Some(3), "evening");
        assert_eq!(choose(&solar, (-40., 10.), 0.), Some(0));
        let timed = [
            Frame {
                time: Some(0.25),
                ..Default::default()
            },
            Frame {
                time: Some(0.75),
                ..Default::default()
            },
        ];
        assert_eq!(choose(&timed, (0., 0.), 0.5), Some(0));
        assert_eq!(choose(&timed, (0., 0.), 0.9), Some(1));
        assert_eq!(
            choose(&timed, (0., 0.), 0.1),
            Some(1),
            "before the first: yesterday's last"
        );
        assert_eq!(choose(&[], (0., 0.), 0.1), None);
    }
}
