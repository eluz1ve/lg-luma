//! Glass: chrome drawn over one blurred copy of the wallpaper, made once when the wallpaper
//! loads, never per frame.
use image::{Rgba, RgbaImage};

/// The copy's size: a quarter of 1080p each way, enough for a blur this soft.
pub const WIDTH: u32 = 480;
pub const HEIGHT: u32 = 270;
const RADIUS: usize = 8;
const PASSES: usize = 3;

/// A small, soft copy of `image` for glass to sample: box-averaged down to 480 × 270, then
/// three box-blur passes each way (close to a Gaussian, about 34 px at 1080p).
pub fn frost(image: &RgbaImage) -> RgbaImage {
    let (width, height) = (WIDTH as usize, HEIGHT as usize);
    let mut pixels = shrink(image);
    let mut scratch = vec![[0f32; 4]; pixels.len()];
    for _ in 0..PASSES {
        blur(&pixels, &mut scratch, width, height, 1, width);
        blur(&scratch, &mut pixels, height, width, width, 1);
    }
    RgbaImage::from_fn(WIDTH, HEIGHT, |x, y| {
        let pixel = pixels[y as usize * width + x as usize];
        Rgba(pixel.map(|channel| channel.round().clamp(0., 255.) as u8))
    })
}

/// Each output pixel is the mean of the source pixels its cell covers.
fn shrink(image: &RgbaImage) -> Vec<[f32; 4]> {
    let (source_width, source_height) = (image.width() as usize, image.height() as usize);
    let cell = |index: usize, source: usize, size: usize| {
        let start = index * source / size;
        let end = ((index + 1) * source / size).max(start + 1).min(source);
        start..end
    };
    let raw = image.as_raw();
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT) as usize);
    for y in 0..HEIGHT as usize {
        let rows = cell(y, source_height, HEIGHT as usize);
        for x in 0..WIDTH as usize {
            let columns = cell(x, source_width, WIDTH as usize);
            let mut sum = [0u32; 4];
            for sy in rows.clone() {
                let row = &raw[(sy * source_width + columns.start) * 4
                    ..(sy * source_width + columns.end) * 4];
                for pixel in row.chunks_exact(4) {
                    for (total, channel) in sum.iter_mut().zip(pixel) {
                        *total += *channel as u32;
                    }
                }
            }
            let count = (rows.len() * columns.len()) as f32;
            pixels.push(sum.map(|total| total as f32 / count));
        }
    }
    pixels
}

/// One box-blur pass along `lines` lines of `length` pixels: `step` apart within a line,
/// `stride` apart between lines. Edges repeat their last pixel.
fn blur(
    from: &[[f32; 4]],
    to: &mut [[f32; 4]],
    length: usize,
    lines: usize,
    step: usize,
    stride: usize,
) {
    let window = (2 * RADIUS + 1) as f32;
    for line in 0..lines {
        let at = |i: isize| from[line * stride + i.clamp(0, length as isize - 1) as usize * step];
        let mut sum = [0f32; 4];
        for i in -(RADIUS as isize)..=RADIUS as isize {
            for (total, channel) in sum.iter_mut().zip(at(i)) {
                *total += channel;
            }
        }
        for i in 0..length {
            to[line * stride + i * step] = sum.map(|total| total / window);
            let (enter, leave) = (
                at((i + RADIUS + 1) as isize),
                at(i as isize - RADIUS as isize),
            );
            for channel in 0..4 {
                sum[channel] += enter[channel] - leave[channel];
            }
        }
    }
}

#[cfg(test)]
#[path = "glass-tests.rs"]
mod tests;
