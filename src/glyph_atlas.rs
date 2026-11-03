//! Text as bitmaps: each glyph is rasterized once on the CPU at the size it is drawn, into one
//! shared texture, and drawn as a textured quad. As outline triangles a run cost the TV's GPU
//! thousands of tiny triangles each frame it was redrawn; as quads it costs two per glyph.
use owned_ttf_parser::OutlineBuilder;

/// One glyph's coverage: `left` and `top` place its first pixel from the pen on the baseline.
#[derive(Debug)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub left: i32,
    pub top: i32,
    pub pixels: Vec<u8>,
}

/// A glyph outline flattened into closed polylines, in physical pixels from the pen, y down.
pub struct Contours {
    scale: f32,
    contours: Vec<Vec<[f32; 2]>>,
}

impl Contours {
    /// `scale` is physical pixels per font unit.
    pub fn new(scale: f32) -> Self {
        Self {
            scale,
            contours: Vec::new(),
        }
    }

    fn point(&self, x: f32, y: f32) -> [f32; 2] {
        [x * self.scale, -y * self.scale]
    }

    fn last(&self) -> [f32; 2] {
        self.contours
            .last()
            .and_then(|contour| contour.last())
            .copied()
            .unwrap_or_default()
    }

    fn push(&mut self, point: [f32; 2]) {
        if let Some(contour) = self.contours.last_mut()
            && contour.last() != Some(&point)
        {
            contour.push(point);
        }
    }

    /// Coverage of the outline grown by `embolden` pixels along its normals; `None` when it
    /// has no area (a space).
    pub fn rasterize(&self, embolden: f32) -> Option<Bitmap> {
        // Contours close themselves; a last point repeating the first adds nothing.
        let closed: Vec<Vec<[f32; 2]>> = self
            .contours
            .iter()
            .map(|contour| match contour.as_slice() {
                [first, .., last] if first == last => contour[..contour.len() - 1].to_vec(),
                _ => contour.clone(),
            })
            .filter(|contour| contour.len() > 2)
            .collect();
        let contours = if embolden > 0. {
            grow(&closed, embolden)
        } else {
            closed
        };
        let points = contours.iter().flatten();
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &[x, y] in points {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        if x0 >= x1 || y0 >= y1 {
            return None;
        }
        // A pixel of margin each side: edges on a pixel boundary stay inside the bitmap.
        let (left, top) = (x0.floor() as i32 - 1, y0.floor() as i32 - 1);
        let width = (x1.ceil() as i32 + 1 - left) as usize;
        let height = (y1.ceil() as i32 + 1 - top) as usize;
        let mut raster = Rasterizer::new(width, height);
        for contour in &contours {
            for (index, &a) in contour.iter().enumerate() {
                let b = contour[(index + 1) % contour.len()];
                let at = |[x, y]: [f32; 2]| [x - left as f32, y - top as f32];
                raster.line(at(a), at(b));
            }
        }
        Some(Bitmap {
            width: width as u32,
            height: height as u32,
            left,
            top,
            pixels: raster.coverage(),
        })
    }
}

impl OutlineBuilder for Contours {
    fn move_to(&mut self, x: f32, y: f32) {
        let point = self.point(x, y);
        self.contours.push(vec![point]);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let point = self.point(x, y);
        self.push(point);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (p0, p1, p2) = (self.last(), self.point(x1, y1), self.point(x, y));
        // Segments from how far the curve bends (as in font-rs): about a tenth of a pixel off.
        let bend = [p0[0] - 2. * p1[0] + p2[0], p0[1] - 2. * p1[1] + p2[1]];
        let steps = segments(bend[0] * bend[0] + bend[1] * bend[1]);
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let u = 1. - t;
            self.push([
                u * u * p0[0] + 2. * u * t * p1[0] + t * t * p2[0],
                u * u * p0[1] + 2. * u * t * p1[1] + t * t * p2[1],
            ]);
        }
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let p0 = self.last();
        let (p1, p2, p3) = (self.point(x1, y1), self.point(x2, y2), self.point(x, y));
        let bend = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| {
            let d = [a[0] - 2. * b[0] + c[0], a[1] - 2. * b[1] + c[1]];
            d[0] * d[0] + d[1] * d[1]
        };
        let steps = segments(bend(p0, p1, p2).max(bend(p1, p2, p3)) * 2.25);
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let u = 1. - t;
            let (a, b, c, d) = (u * u * u, 3. * u * u * t, 3. * u * t * t, t * t * t);
            self.push([
                a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0],
                a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1],
            ]);
        }
    }
    fn close(&mut self) {}
}

fn segments(bend_squared: f32) -> usize {
    if bend_squared < 0.333 {
        1
    } else {
        (1. + (3. * bend_squared).sqrt().sqrt()).floor().min(64.) as usize
    }
}

/// Moves every vertex `by` pixels away from the filled side, so stems thicken by twice that.
/// Contours wind one way around ink and the other around counters; the largest contour is
/// always ink, which tells which side of travel is outward.
fn grow(contours: &[Vec<[f32; 2]>], by: f32) -> Vec<Vec<[f32; 2]>> {
    let area = |contour: &Vec<[f32; 2]>| {
        (0..contour.len())
            .map(|i| {
                let ([ax, ay], [bx, by]) = (contour[i], contour[(i + 1) % contour.len()]);
                ax * by - bx * ay
            })
            .sum::<f32>()
    };
    let Some(largest) = contours
        .iter()
        .map(area)
        .max_by(|a, b| a.abs().total_cmp(&b.abs()))
    else {
        return Vec::new();
    };
    let side = largest.signum();
    contours
        .iter()
        .map(|contour| {
            let n = contour.len();
            (0..n)
                .map(|i| {
                    let (a, p, b) = (contour[(i + n - 1) % n], contour[i], contour[(i + 1) % n]);
                    let normal = |from: [f32; 2], to: [f32; 2]| {
                        let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
                        let length = (dx * dx + dy * dy).sqrt().max(1e-6);
                        [-dy / length * side, dx / length * side]
                    };
                    let (incoming, outgoing) = (normal(a, p), normal(p, b));
                    let mut mean = [incoming[0] + outgoing[0], incoming[1] + outgoing[1]];
                    let length = (mean[0] * mean[0] + mean[1] * mean[1]).sqrt();
                    if length < 1e-3 {
                        mean = outgoing;
                    } else {
                        mean = [mean[0] / length, mean[1] / length];
                    }
                    // A miter, limited so sharp corners do not spike.
                    let cos = (mean[0] * outgoing[0] + mean[1] * outgoing[1]).max(0.5);
                    [p[0] - mean[0] * by / cos, p[1] - mean[1] * by / cos]
                })
                .collect()
        })
        .collect()
}

/// Exact-area coverage of filled outlines, as in font-rs: each edge adds the signed area it
/// sweeps to the pixels it crosses, and a running sum along the rows gives the coverage.
struct Rasterizer {
    width: usize,
    height: usize,
    area: Vec<f32>,
}

impl Rasterizer {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            area: vec![0.; width * height + 2],
        }
    }

    fn line(&mut self, p0: [f32; 2], p1: [f32; 2]) {
        if (p0[1] - p1[1]).abs() <= f32::EPSILON {
            return;
        }
        let (direction, p0, p1) = if p0[1] < p1[1] {
            (1., p0, p1)
        } else {
            (-1., p1, p0)
        };
        let slope = (p1[0] - p0[0]) / (p1[1] - p0[1]);
        let mut x = p0[0];
        let first = p0[1].max(0.) as usize;
        if p0[1] < 0. {
            x -= p0[1] * slope;
        }
        for y in first..self.height.min(p1[1].ceil() as usize) {
            let row = y * self.width;
            let dy = ((y + 1) as f32).min(p1[1]) - (y as f32).max(p0[1]);
            let next = x + slope * dy;
            let d = dy * direction;
            let (x0, x1) = if x < next { (x, next) } else { (next, x) };
            let x0_floor = x0.floor();
            let x0i = x0_floor as usize;
            let x1_ceil = x1.ceil();
            let x1i = x1_ceil as usize;
            if x1i <= x0i + 1 {
                let middle = 0.5 * (x + next) - x0_floor;
                self.area[row + x0i] += d - d * middle;
                self.area[row + x0i + 1] += d * middle;
            } else {
                let s = (x1 - x0).recip();
                let x0f = x0 - x0_floor;
                let a0 = 0.5 * s * (1. - x0f) * (1. - x0f);
                let x1f = x1 - x1_ceil + 1.;
                let am = 0.5 * s * x1f * x1f;
                self.area[row + x0i] += d * a0;
                if x1i == x0i + 2 {
                    self.area[row + x0i + 1] += d * (1. - a0 - am);
                } else {
                    let a1 = s * (1.5 - x0f);
                    self.area[row + x0i + 1] += d * (a1 - a0);
                    for xi in x0i + 2..x1i - 1 {
                        self.area[row + xi] += d * s;
                    }
                    let a2 = a1 + (x1i - x0i - 3) as f32 * s;
                    self.area[row + x1i - 1] += d * (1. - a2 - am);
                }
                self.area[row + x1i] += d * am;
            }
            x = next;
        }
    }

    fn coverage(&self) -> Vec<u8> {
        let mut sum = 0.;
        self.area[..self.width * self.height]
            .iter()
            .map(|area| {
                sum += area;
                (sum.abs().min(1.) * 255.).round() as u8
            })
            .collect()
    }
}

/// A text shadow made once per glyph: the glyph's coverage laid down at each tap with `alpha`,
/// as drawing the glyph faint at every tap would, in one bitmap. `taps` are pixel offsets.
pub fn shadow(glyph: &Bitmap, taps: &[[f32; 2]], alpha: f32) -> Bitmap {
    let reach = taps
        .iter()
        .map(|[x, y]| x.abs().max(y.abs()))
        .fold(0., f32::max);
    let margin = reach.ceil() as u32 + 1;
    let (width, height) = (glyph.width + 2 * margin, glyph.height + 2 * margin);
    let sample = |x: f32, y: f32| {
        // Bilinear, with texel (i, j) at (i, j); outside the glyph is empty.
        let (fx, fy) = (x.floor(), y.floor());
        let (tx, ty) = (x - fx, y - fy);
        let texel = |i: i32, j: i32| {
            if i < 0 || j < 0 || i >= glyph.width as i32 || j >= glyph.height as i32 {
                0.
            } else {
                f32::from(glyph.pixels[j as usize * glyph.width as usize + i as usize]) / 255.
            }
        };
        let (i, j) = (fx as i32, fy as i32);
        let top = texel(i, j) * (1. - tx) + texel(i + 1, j) * tx;
        let bottom = texel(i, j + 1) * (1. - tx) + texel(i + 1, j + 1) * tx;
        top * (1. - ty) + bottom * ty
    };
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let (gx, gy) = (x as f32 - margin as f32, y as f32 - margin as f32);
            let clear: f32 = taps
                .iter()
                .map(|[dx, dy]| 1. - alpha * sample(gx - dx, gy - dy))
                .product();
            pixels.push(((1. - clear) * 255.).round() as u8);
        }
    }
    Bitmap {
        width,
        height,
        left: glyph.left - margin as i32,
        top: glyph.top - margin as i32,
        pixels,
    }
}

/// Rows of glyphs in a square texture, each row as tall as the first glyph that opened it.
pub struct Shelves {
    size: u32,
    rows: Vec<[u32; 3]>,
    bottom: u32,
}

impl Shelves {
    pub fn new(size: u32) -> Self {
        Self {
            size,
            rows: Vec::new(),
            bottom: 0,
        }
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    /// Pixels in use, rows to the texture's right edge.
    pub fn used(&self) -> u32 {
        self.bottom * self.size
    }

    /// Where a `width` × `height` glyph goes, with a pixel between neighbours; `None` once
    /// the texture is full.
    pub fn allocate(&mut self, width: u32, height: u32) -> Option<[u32; 2]> {
        let (width, height) = (width + 1, height + 1);
        if width > self.size {
            return None;
        }
        let fits = |&&[_, row_height, x]: &&[u32; 3]| {
            row_height >= height && row_height <= height + height / 4 + 2 && x + width <= self.size
        };
        if let Some(index) = self.rows.iter().position(|row| fits(&row)) {
            let row = &mut self.rows[index];
            let place = [row[2], row[0]];
            row[2] += width;
            return Some(place);
        }
        if self.bottom + height > self.size {
            return None;
        }
        let place = [0, self.bottom];
        self.rows.push([self.bottom, height, width]);
        self.bottom += height;
        Some(place)
    }

    pub fn clear(&mut self) {
        self.rows.clear();
        self.bottom = 0;
    }
}

#[cfg(test)]
#[path = "glyph-atlas-tests.rs"]
mod tests;
