use super::*;
use image::{Rgba, RgbaImage};

fn luminance(image: &RgbaImage, x: u32, y: u32) -> f32 {
    let Rgba([r, g, b, _]) = *image.get_pixel(x, y);
    (r as f32 + g as f32 + b as f32) / 3.
}

#[test]
fn frost_makes_one_small_soft_copy() {
    let wallpaper = RgbaImage::from_fn(1920, 1080, |x, _| {
        if x < 960 {
            Rgba([0, 0, 0, 255])
        } else {
            Rgba([255, 255, 255, 255])
        }
    });
    let copy = frost(&wallpaper);
    assert_eq!(copy.dimensions(), (480, 270));
    assert!(luminance(&copy, 10, 135) < 8.);
    assert!(luminance(&copy, 470, 135) > 247.);
    assert!((luminance(&copy, 240, 135) - 128.).abs() <= 24.);
    let row: Vec<f32> = (200..=280).map(|x| luminance(&copy, x, 135)).collect();
    assert!(row.windows(2).all(|pair| pair[1] >= pair[0]), "{row:?}");
    let band = row.iter().filter(|&&v| v > 25.5 && v < 229.5).count();
    assert!(band > 12, "the edge is soft over {band} px");
}

#[test]
fn frost_takes_any_wallpaper_size() {
    for (width, height) in [(3840, 2160), (1001, 563)] {
        let wallpaper = RgbaImage::from_pixel(width, height, Rgba([40, 90, 160, 255]));
        let copy = frost(&wallpaper);
        assert_eq!(copy.dimensions(), (480, 270));
        for pixel in copy.pixels() {
            for (channel, expected) in pixel.0.iter().zip([40u8, 90, 160, 255]) {
                assert!(channel.abs_diff(expected) <= 1, "{pixel:?}");
            }
        }
    }
}
