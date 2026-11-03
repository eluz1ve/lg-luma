//! Lazy TTF metrics and individual outlines; never preprocess an entire font.
use owned_ttf_parser::{AsFaceRef, GlyphId, OutlineBuilder, OwnedFace};

pub struct Font {
    face: OwnedFace,
}

pub struct LineMetrics {
    pub ascent: f32,
    pub descent: f32,
}

pub struct Metrics {
    pub advance_width: f32,
}

impl Font {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, String> {
        OwnedFace::from_vec(bytes, 0)
            .map(|face| Self { face })
            .map_err(|e| e.to_string())
    }

    pub fn lookup_glyph_index(&self, character: char) -> u16 {
        self.face
            .as_face_ref()
            .glyph_index(character)
            .map_or(0, |id| id.0)
    }

    pub fn horizontal_line_metrics(&self, size: f32) -> Option<LineMetrics> {
        let face = self.face.as_face_ref();
        let scale = size / f32::from(face.units_per_em());
        Some(LineMetrics {
            ascent: f32::from(face.ascender()) * scale,
            descent: f32::from(face.descender()) * scale,
        })
    }

    pub fn metrics(&self, character: char, size: f32) -> Metrics {
        let face = self.face.as_face_ref();
        let glyph = GlyphId(self.lookup_glyph_index(character));
        Metrics {
            advance_width: f32::from(face.glyph_hor_advance(glyph).unwrap_or(0)) * size
                / f32::from(face.units_per_em()),
        }
    }

    pub fn horizontal_kern(&self, left: char, right: char, size: f32) -> Option<f32> {
        let face = self.face.as_face_ref();
        let left = face.glyph_index(left)?;
        let right = face.glyph_index(right)?;
        for table in face.tables().kern?.subtables {
            if table.horizontal
                && !table.has_cross_stream
                && let Some(value) = table.glyphs_kerning(left, right)
            {
                return Some(f32::from(value) * size / f32::from(face.units_per_em()));
            }
        }
        None
    }

    pub fn units_per_em(&self) -> f32 {
        f32::from(self.face.as_face_ref().units_per_em())
    }

    /// The character's outline in font units, y up; false when it has none (a space).
    pub fn outline(&self, character: char, builder: &mut dyn OutlineBuilder) -> bool {
        let glyph = GlyphId(self.lookup_glyph_index(character));
        self.face
            .as_face_ref()
            .outline_glyph(glyph, builder)
            .is_some()
    }
}
