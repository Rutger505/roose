use std::path::Path;

use tiny_skia::{
    FillRule, FilterQuality, LineCap, Paint, PathBuilder, Pixmap, PixmapPaint, Rect, Stroke,
    Transform,
};

const UNITS: f32 = 80.0;
const WHITE: [u8; 3] = [250, 250, 247];
const OUTLINE: [u8; 3] = [120, 120, 125];
const ORANGE: [u8; 3] = [245, 150, 30];

/// Where the beak tip sits, as a fraction of the sprite size, when facing right.
pub const BEAK: (f32, f32) = (0.97, 0.22);

pub enum Source {
    Builtin,
    Png(Pixmap),
}

impl Source {
    pub fn load(path: Option<&Path>) -> Result<Self, String> {
        match path {
            None => Ok(Self::Builtin),
            Some(path) => Pixmap::load_png(path)
                .map(Self::Png)
                .map_err(|e| format!("could not load sprite {}: {e}", path.display())),
        }
    }

    pub fn aspect(&self) -> f32 {
        match self {
            Self::Builtin => 1.0,
            Self::Png(p) => p.height() as f32 / p.width() as f32,
        }
    }

    /// Renders the goose facing right into a `width`x`height` pixel image.
    pub fn render(&self, width: u32, height: u32) -> Pixmap {
        let mut pixmap = Pixmap::new(width, height).expect("sprite size must be non-zero");
        match self {
            Self::Builtin => {
                let transform = Transform::from_scale(width as f32 / UNITS, height as f32 / UNITS);
                draw_goose(&mut pixmap, transform);
            }
            Self::Png(source) => {
                let transform = Transform::from_scale(
                    width as f32 / source.width() as f32,
                    height as f32 / source.height() as f32,
                );
                let paint = PixmapPaint {
                    quality: FilterQuality::Bicubic,
                    ..Default::default()
                };
                pixmap.draw_pixmap(0, 0, source.as_ref(), &paint, transform, None);
            }
        }
        pixmap
    }
}

/// Converts tiny-skia's premultiplied RGBA into Wayland's little-endian ARGB8888, optionally mirrored.
pub fn write_argb(pixmap: &Pixmap, mirrored: bool, out: &mut [u8]) {
    let width = pixmap.width() as usize;
    for (src_row, dst_row) in pixmap
        .data()
        .chunks_exact(width * 4)
        .zip(out.chunks_exact_mut(width * 4))
    {
        for (x, px) in src_row.as_chunks::<4>().0.iter().enumerate() {
            let dst_x = if mirrored { width - 1 - x } else { x };
            dst_row[dst_x * 4..dst_x * 4 + 4].copy_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
    }
}

fn paint(rgb: [u8; 3]) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color_rgba8(rgb[0], rgb[1], rgb[2], 255);
    paint.anti_alias = true;
    paint
}

fn oval(x: f32, y: f32, w: f32, h: f32) -> tiny_skia::Path {
    PathBuilder::from_oval(Rect::from_xywh(x, y, w, h).unwrap()).unwrap()
}

fn draw_goose(pixmap: &mut Pixmap, t: Transform) {
    let thick = |width| Stroke {
        width,
        line_cap: LineCap::Round,
        ..Default::default()
    };

    let mut legs = PathBuilder::new();
    legs.move_to(35.0, 58.0);
    legs.line_to(33.0, 74.0);
    legs.move_to(45.0, 58.0);
    legs.line_to(47.0, 74.0);
    let legs = legs.finish().unwrap();
    pixmap.stroke_path(&legs, &paint(ORANGE), &thick(3.5), t, None);
    for foot_x in [28.0, 42.0] {
        pixmap.fill_path(
            &oval(foot_x, 72.0, 11.0, 4.0),
            &paint(ORANGE),
            FillRule::Winding,
            t,
            None,
        );
    }

    let mut neck = PathBuilder::new();
    neck.move_to(52.0, 46.0);
    neck.quad_to(62.0, 34.0, 60.0, 17.0);
    let neck = neck.finish().unwrap();

    let mut tail = PathBuilder::new();
    tail.move_to(18.0, 40.0);
    tail.line_to(3.0, 35.0);
    tail.line_to(14.0, 54.0);
    tail.close();
    let tail = tail.finish().unwrap();

    let body = oval(10.0, 34.0, 52.0, 30.0);
    let head = oval(52.0, 7.0, 18.0, 17.0);

    let outline = paint(OUTLINE);
    for shape in [&tail, &body, &head] {
        pixmap.stroke_path(shape, &outline, &thick(3.0), t, None);
    }
    pixmap.stroke_path(&neck, &outline, &thick(13.0), t, None);

    let white = paint(WHITE);
    for shape in [&tail, &body, &head] {
        pixmap.fill_path(shape, &white, FillRule::Winding, t, None);
    }
    pixmap.stroke_path(&neck, &white, &thick(10.0), t, None);

    let mut wing = PathBuilder::new();
    wing.move_to(20.0, 44.0);
    wing.quad_to(34.0, 38.0, 48.0, 48.0);
    wing.quad_to(34.0, 56.0, 22.0, 50.0);
    let wing = wing.finish().unwrap();
    pixmap.stroke_path(&wing, &paint([200, 200, 205]), &thick(2.0), t, None);

    let mut beak = PathBuilder::new();
    beak.move_to(67.0, 12.0);
    beak.line_to(78.5, 17.5);
    beak.line_to(67.0, 21.0);
    beak.close();
    let beak = beak.finish().unwrap();
    pixmap.fill_path(&beak, &paint(ORANGE), FillRule::Winding, t, None);

    pixmap.fill_path(
        &oval(61.0, 11.0, 4.0, 4.0),
        &paint([20, 20, 20]),
        FillRule::Winding,
        t,
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrored_argb_swaps_channels_and_columns() {
        let mut pixmap = Pixmap::new(2, 1).unwrap();
        pixmap.data_mut().copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let mut out = [0; 8];
        write_argb(&pixmap, true, &mut out);
        assert_eq!(out, [7, 6, 5, 8, 3, 2, 1, 4]);
    }

    #[test]
    fn builtin_goose_is_not_empty() {
        let pixmap = Source::Builtin.render(80, 80);
        assert!(pixmap.pixels().iter().filter(|p| p.alpha() > 0).count() > 1000);
    }
}
