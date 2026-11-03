use super::*;

/// An outline of rectangles given in pixels (x, y down), one unit per pixel.
fn rectangles(rects: &[[f32; 4]], clockwise: &[bool]) -> Contours {
    let mut contours = Contours::new(1.);
    for (&[x0, y0, x1, y1], &clockwise) in rects.iter().zip(clockwise) {
        let mut corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
        if !clockwise {
            corners.reverse();
        }
        // Font units point up.
        contours.move_to(corners[0][0], -corners[0][1]);
        for &[x, y] in &corners[1..] {
            contours.line_to(x, -y);
        }
        contours.close();
    }
    contours
}

fn at(bitmap: &Bitmap, x: i32, y: i32) -> u8 {
    let (i, j) = (x - bitmap.left, y - bitmap.top);
    if i < 0 || j < 0 || i >= bitmap.width as i32 || j >= bitmap.height as i32 {
        return 0;
    }
    bitmap.pixels[(j as u32 * bitmap.width + i as u32) as usize]
}

#[test]
fn a_rectangle_on_the_pixel_grid_covers_exactly_its_pixels() {
    let bitmap = rectangles(&[[1., 1., 5., 4.]], &[true])
        .rasterize(0.)
        .unwrap();
    assert_eq!((bitmap.left, bitmap.top), (0, 0), "a pixel of margin");
    for y in -1..6 {
        for x in -1..7 {
            let inside = (1..5).contains(&x) && (1..4).contains(&y);
            assert_eq!(at(&bitmap, x, y), if inside { 255 } else { 0 }, "{x},{y}");
        }
    }
    // Either winding fills the same.
    let other = rectangles(&[[1., 1., 5., 4.]], &[false])
        .rasterize(0.)
        .unwrap();
    assert_eq!(bitmap.pixels, other.pixels);
}

#[test]
fn edges_between_pixels_cover_them_by_area() {
    let bitmap = rectangles(&[[1.5, 1., 4.25, 3.5]], &[true])
        .rasterize(0.)
        .unwrap();
    assert_eq!(at(&bitmap, 1, 1), 128, "half of the left column");
    assert_eq!(at(&bitmap, 2, 1), 255);
    assert_eq!(at(&bitmap, 4, 1), 64, "a quarter of the right column");
    assert_eq!(at(&bitmap, 2, 3), 128, "half of the bottom row");
    assert_eq!(at(&bitmap, 1, 3), 64, "a corner: half by half");
}

#[test]
fn a_counter_wound_the_other_way_stays_empty() {
    let ring = rectangles(&[[0., 0., 9., 9.], [3., 3., 6., 6.]], &[true, false])
        .rasterize(0.)
        .unwrap();
    assert_eq!(at(&ring, 1, 1), 255);
    assert_eq!(at(&ring, 4, 4), 0, "the hole");
    // Emboldening grows ink into the hole too.
    let bold = rectangles(&[[0., 0., 9., 9.], [3., 3., 6., 6.]], &[true, false])
        .rasterize(0.5)
        .unwrap();
    assert_eq!(at(&bold, 3, 4), 128, "half a pixel into the hole");
    assert_eq!(at(&bold, -1, 4), 128, "half a pixel out");
    assert_eq!(at(&bold, 4, 4), 0);
}

#[test]
fn curves_flatten_close_to_the_true_curve() {
    // Four quadratic arcs through a square's corners enclose 10r²/3: each adds two thirds of
    // its control triangle to the inner square.
    let mut contours = Contours::new(1.);
    let r = 20.;
    contours.move_to(r, 0.);
    contours.quad_to(r, r, 0., r);
    contours.quad_to(-r, r, -r, 0.);
    contours.quad_to(-r, -r, 0., -r);
    contours.quad_to(r, -r, r, 0.);
    contours.close();
    let bitmap = contours.rasterize(0.).unwrap();
    let area: f32 = bitmap.pixels.iter().map(|&p| f32::from(p) / 255.).sum();
    // Chords cut inside the curve by about a tenth of a pixel.
    let deficit = 1. - area / (10. * r * r / 3.);
    assert!(deficit > 0. && deficit < 0.012, "{area}");
    assert_eq!(at(&bitmap, 0, 0), 255);
    assert_eq!(at(&bitmap, 0, -25), 0);
}

#[test]
fn a_shadow_lays_the_glyph_down_at_each_tap() {
    let glyph = rectangles(&[[0., 0., 1., 1.]], &[true])
        .rasterize(0.)
        .unwrap();
    let shadow = shadow(&glyph, &[[0., 1.], [1., 1.]], 0.5);
    assert_eq!(at(&shadow, 0, 0), 0, "nothing at the glyph's own place");
    assert_eq!(at(&shadow, 0, 1), 128, "one tap below");
    assert_eq!(at(&shadow, 1, 1), 128, "the other, below and right");
    // A tap halfway between pixels shares the glyph between them.
    let half = shadow_of_half(&glyph);
    assert_eq!(at(&half, 0, 2), 64);
    assert_eq!(at(&half, 0, 3), 64);
}

fn shadow_of_half(glyph: &Bitmap) -> Bitmap {
    shadow(glyph, &[[0., 2.5]], 0.5)
}

#[test]
fn overlapping_taps_darken_as_layers_do() {
    let glyph = rectangles(&[[0., 0., 1., 1.]], &[true])
        .rasterize(0.)
        .unwrap();
    let shadow = shadow(&glyph, &[[0., 1.], [0., 1.]], 0.5);
    // Two layers of half: 1 - 0.5 * 0.5.
    assert_eq!(at(&shadow, 0, 1), 191);
}

#[test]
fn shelves_pack_rows_and_say_when_full() {
    let mut shelves = Shelves::new(64);
    assert_eq!(shelves.allocate(20, 10), Some([0, 0]));
    assert_eq!(
        shelves.allocate(20, 9),
        Some([21, 0]),
        "same row, a pixel apart"
    );
    assert_eq!(
        shelves.allocate(20, 30),
        Some([0, 11]),
        "too tall: a new row"
    );
    assert_eq!(shelves.allocate(30, 10), Some([0, 42]), "first row is full");
    assert_eq!(shelves.allocate(10, 30), Some([21, 11]));
    assert_eq!(shelves.allocate(64, 1), None, "no room for the gap");
    assert_eq!(shelves.allocate(10, 20), None, "a new row would not fit");
    assert_eq!(shelves.used(), 53 * 64);
    shelves.clear();
    assert_eq!(shelves.allocate(10, 30), Some([0, 0]));
}

#[test]
fn a_real_glyph_has_ink_and_a_counter() {
    // Only where the native assets were generated.
    let Ok(bytes) = std::fs::read(".build/assets/font-400.ttf") else {
        return;
    };
    let font = crate::font::Font::from_bytes(bytes).unwrap();
    let size = 40.;
    let mut contours = Contours::new(size / font.units_per_em());
    assert!(font.outline('o', &mut contours));
    let bitmap = contours.rasterize(0.).unwrap();
    assert!(
        bitmap.top < -15 && bitmap.top > -30,
        "x-height: {}",
        bitmap.top
    );
    let middle = |dx: i32| {
        at(
            &bitmap,
            bitmap.left + bitmap.width as i32 / 2 + dx,
            bitmap.top + bitmap.height as i32 / 2,
        )
    };
    assert_eq!(middle(0), 0, "the counter");
    let ink = bitmap.pixels.iter().filter(|&&p| p == 255).count();
    assert!(ink > 50, "{ink}");
    let mut space = Contours::new(size / font.units_per_em());
    font.outline(' ', &mut space);
    assert!(space.rasterize(0.).is_none());
}
