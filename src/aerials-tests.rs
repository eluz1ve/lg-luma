use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "luma-aerials-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn still(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbImage::from_fn(width, height, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
    });
    crate::wallpapers::jpeg(&image, 90).unwrap()
}

#[test]
fn every_place_has_a_safe_id_and_a_name_in_every_language() {
    let pictures = &catalog().pictures;
    assert!(pictures.len() >= 80, "{}", pictures.len());
    let mut ids = std::collections::HashSet::new();
    for picture in pictures {
        assert!(valid_id(&picture.id), "{}", picture.id);
        assert!(ids.insert(picture.id.as_str()), "{} twice", picture.id);
        for language in Language::ALL {
            assert!(
                !picture.name(language).is_empty(),
                "{} {language:?}",
                picture.id
            );
        }
    }
    let east_asia = pictures
        .iter()
        .find(|picture| picture.id == "GMT026_363A_103NC_E1027_KOREA_JAPAN_NIGHT")
        .unwrap();
    assert_eq!(east_asia.name(Language::Ru), "Восточная Азия");
    assert_eq!(
        catalog().snapshot(&east_asia.id),
        "https://sylvan.apple.com/Aerials/jjGqDwCNrOsVJ4of6uSVr/snapshots/GMT026_363A_103NC_E1027_KOREA_JAPAN_NIGHT.jpg"
    );
}

#[test]
fn ids_that_could_leave_the_folder_are_refused() {
    for id in ["", ".", "..", "../x", "a/b", "a\\b", ".partial", "x.jpg"] {
        assert!(!valid_id(id), "{id:?}");
    }
}

#[test]
fn a_round_shows_every_place_once_and_the_next_round_does_not_repeat_the_last() {
    let count = 7;
    let mut rotation = Rotation::start(1_700_000_000);
    let mut shown = vec![rotation.current(count)];
    for _ in 0..count * 5 {
        let next = rotation.next(count);
        rotation = rotation.advanced(count, 0);
        assert_eq!(rotation.current(count), next);
        assert_ne!(shown.last(), Some(&next), "the same place twice in a row");
        shown.push(next);
    }
    for round in shown.chunks(count).take(5) {
        let mut sorted = round.to_vec();
        sorted.sort();
        assert_eq!(sorted, (0..count).collect::<Vec<_>>());
    }
    // The order is not the catalog's.
    assert_ne!(shown[..count], (0..count).collect::<Vec<_>>());
}

#[test]
fn the_order_follows_its_seed_and_a_new_rotation_starts_at_that_time() {
    let a = Rotation::start(100);
    assert_eq!(a.changed, 100);
    assert_eq!(a.index, 0);
    assert_eq!(Rotation::start(100), a);
    let b = Rotation::start(101);
    let count = catalog().pictures.len();
    let firsts: Vec<_> = (0..6)
        .map(|n| Rotation::start(100 + n).current(count))
        .collect();
    assert!(
        firsts.windows(2).any(|pair| pair[0] != pair[1]),
        "{firsts:?}"
    );
    assert_ne!(a.seed, b.seed);
    assert_eq!(a.advanced(count, 900).changed, 900);
}

#[test]
fn a_place_changes_after_fifteen_minutes() {
    let rotation = Rotation::start(1000);
    assert!(!rotation.due(1000 + INTERVAL - 1));
    assert!(rotation.due(1000 + INTERVAL));
    // A clock that went back waits for the interval again rather than changing at once.
    assert!(!rotation.due(10));
}

#[test]
fn a_downloaded_still_is_kept_shaded_plain_and_as_a_thumbnail() {
    let fixture = Fixture::new();
    let id = &catalog().pictures[0].id;
    let fetched = std::cell::Cell::new(0);
    let fetch = |url: &str| {
        assert_eq!(url, catalog().snapshot(id));
        fetched.set(fetched.get() + 1);
        Ok(still(1920, 1080))
    };
    let kept = ensure(&fixture.0, id, fetch).unwrap();
    assert_eq!(kept.id, *id);
    assert!(kept.has_original);
    for path in [kept.image(), kept.unshaded_image(), kept.thumbnail()] {
        assert!(Path::new(&path).is_file(), "{path}");
    }
    let thumbnail = image::open(kept.thumbnail()).unwrap();
    assert_eq!((thumbnail.width(), thumbnail.height()), (480, 270));
    // The plain still is Apple's file as it came; the shaded one is darker at the bottom.
    assert_eq!(fs::read(kept.unshaded_image()).unwrap(), still(1920, 1080));
    // Already on the TV: not fetched again.
    ensure(&fixture.0, id, fetch).unwrap();
    assert_eq!(fetched.get(), 1);
}

#[test]
fn a_still_that_is_not_full_size_or_not_an_image_is_refused_and_leaves_nothing() {
    let fixture = Fixture::new();
    let id = &catalog().pictures[0].id;
    assert!(ensure(&fixture.0, id, |_| Ok(still(900, 580))).is_err());
    assert!(ensure(&fixture.0, id, |_| Ok(b"<html>".to_vec())).is_err());
    assert!(ensure(&fixture.0, id, |_| Err("offline".into())).is_err());
    assert!(ensure(&fixture.0, "../escape", |_| Ok(still(1920, 1080))).is_err());
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 0);
}

#[test]
fn stills_no_longer_wanted_are_removed() {
    let fixture = Fixture::new();
    let ids: Vec<String> = catalog().pictures[..3]
        .iter()
        .map(|picture| picture.id.clone())
        .collect();
    for id in &ids {
        ensure(&fixture.0, id, |_| Ok(still(1920, 1080))).unwrap();
    }
    fs::create_dir(fixture.0.join(format!(".{}.partial", ids[0]))).unwrap();
    prune(&fixture.0, &ids[1..]);
    let mut left: Vec<_> = fs::read_dir(&fixture.0)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    left.sort();
    let mut expected = ids[1..].to_vec();
    expected.sort();
    assert_eq!(left, expected);
}

#[test]
#[ignore = "reaches Apple's servers"]
fn apple_serves_a_full_size_still_to_luma_trusting_only_apples_root() {
    let bytes = get(&agent(), &catalog().snapshot(&catalog().pictures[0].id)).unwrap();
    let image = crate::wallpapers::decode(&bytes).unwrap();
    assert_eq!((image.width(), image.height()), (1920, 1080));
    // The same request trusting the usual roots is refused.
    let usual = ureq::Agent::config_builder().build().new_agent();
    assert!(get(&usual, &catalog().snapshot(&catalog().pictures[0].id)).is_err());
}
