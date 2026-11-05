//! Line-art animals for card backs, court cards and game icons.
//!
//! Each drawing is kept as a small subset of SVG path data (absolute `M`, `L`,
//! `C` and `Z`) plus circles and ellipses, in the coordinates it was designed
//! in. It is flattened into polylines once and then stroked with round joins.

use macroquad::prelude::*;
use std::f32::consts::TAU;
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Animal {
    Fox,
    Heron,
    Owl,
    Whale,
    Hare,
    Spider,
    Swallow,
}

const ALL: [Animal; 7] = [
    Animal::Fox,
    Animal::Heron,
    Animal::Owl,
    Animal::Whale,
    Animal::Hare,
    Animal::Spider,
    Animal::Swallow,
];

struct Source {
    paths: &'static [&'static str],
    /// (cx, cy, r): stroked circles.
    rings: &'static [(f32, f32, f32)],
    /// (cx, cy, rx, ry): stroked ellipses.
    ellipses: &'static [(f32, f32, f32, f32)],
    /// (cx, cy, r): filled dots (eyes, noses).
    dots: &'static [(f32, f32, f32)],
    /// Tight frame around the drawing, for icons and court cards.
    crop: Rect,
    /// Frame that centres the drawing on a 100×140 card back.
    back: Rect,
}

const fn back_view(dx: f32, dy: f32) -> Rect {
    Rect {
        x: -dx,
        y: -dy,
        w: 100.0,
        h: 140.0,
    }
}

const fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, w, h }
}

fn source(animal: Animal) -> Source {
    match animal {
        Animal::Fox => Source {
            paths: &[
                "M30 38 L41 56 L59 56 L70 38 L72 66 L50 88 L28 66 Z",
                "M28 66 L44 72 L50 86 L56 72 L72 66",
                "M34 46 L39 55 M66 46 L61 55",
            ],
            rings: &[],
            ellipses: &[],
            dots: &[(42.0, 63.0, 2.0), (58.0, 63.0, 2.0), (50.0, 86.0, 2.4)],
            crop: rect(20.0, 30.0, 60.0, 66.0),
            back: back_view(0.0, 7.0),
        },
        Animal::Heron => Source {
            paths: &[
                "M42 70 C48 60 66 60 76 70 C68 76 52 78 42 70 Z",
                "M76 70 L86 75",
                "M46 66 C38 58 52 50 46 42 C43 37 46 33 51 33",
                "M55 34 L68 37",
                "M58 75 L56 100 M64 75 L66 100 M44 100 L80 100",
            ],
            rings: &[(52.0, 34.0, 3.0)],
            ellipses: &[],
            dots: &[],
            crop: rect(30.0, 28.0, 60.0, 76.0),
            back: back_view(-6.0, 4.0),
        },
        Animal::Owl => Source {
            paths: &[
                "M34 54 L33 41 L43 48",
                "M66 54 L67 41 L57 48",
                "M48 67 L50 72 L52 67 Z",
                "M43 80 L46 83 L49 80 M51 80 L54 83 L57 80 M47 86 L50 89 L53 86",
                "M44 95 L44 100 M56 95 L56 100 M34 100 L66 100",
            ],
            rings: &[(42.0, 61.0, 7.0), (58.0, 61.0, 7.0)],
            ellipses: &[(50.0, 70.0, 21.0, 25.0)],
            dots: &[(42.0, 61.0, 2.2), (58.0, 61.0, 2.2)],
            crop: rect(20.0, 36.0, 60.0, 68.0),
            back: back_view(0.0, -2.0),
        },
        Animal::Whale => Source {
            paths: &[
                "M20 74 C20 60 40 54 58 59 C68 62 74 66 79 62 L84 52 C86 58 88 62 92 64 \
                 C86 66 82 68 80 70 C72 80 58 86 42 86 C28 86 20 82 20 74 Z",
                "M27 78 C38 83 54 83 66 77",
                "M44 84 L48 92 L54 84",
                "M38 53 C38 47 35 44 32 42 M38 53 C38 47 41 44 44 42 M38 53 L38 44",
            ],
            rings: &[],
            ellipses: &[],
            dots: &[(30.0, 70.0, 1.8)],
            crop: rect(16.0, 38.0, 80.0, 58.0),
            back: back_view(-6.0, 2.0),
        },
        Animal::Hare => Source {
            paths: &[
                "M38 98 C32 82 38 68 52 68 C66 68 74 80 72 92 C71 97 66 99 58 99 Z",
                "M41 52 C36 40 36 30 40 28 C44 29 45 40 45 51",
                "M47 51 C47 39 50 31 54 31 C57 33 53 43 50 52",
                "M34 99 L78 99",
            ],
            rings: &[(45.0, 60.0, 9.0), (73.0, 88.0, 3.0)],
            ellipses: &[],
            dots: &[(42.0, 59.0, 1.8)],
            crop: rect(28.0, 24.0, 56.0, 80.0),
            back: back_view(-4.0, 5.0),
        },
        Animal::Spider => Source {
            paths: &[
                "M9.3 12 L5 9 L3.5 4.5 M9 13.8 L4 13 L2 10 M9.3 15.6 L4.5 17.5 L3 21 \
                 M10.4 16.8 L8 20.5 L7.5 22.5 M14.7 12 L19 9 L20.5 4.5 M15 13.8 L20 13 L22 10 \
                 M14.7 15.6 L19.5 17.5 L21 21 M13.6 16.8 L16 20.5 L16.5 22.5",
            ],
            rings: &[(12.0, 14.0, 3.2), (12.0, 8.6, 1.9)],
            ellipses: &[],
            dots: &[],
            crop: rect(0.0, 0.0, 24.0, 24.0),
            back: rect(0.0, 0.0, 24.0, 24.0),
        },
        Animal::Swallow => Source {
            paths: &[
                "M2 9 C6 7.5 9 8.5 11 10.5 L21.5 5 L15.5 12.5 C13.5 15.5 9.5 16.5 6.5 14.5 \
                 L2.5 19 L4 13.5 Z",
            ],
            rings: &[],
            ellipses: &[],
            dots: &[(5.5, 10.0, 0.6)],
            crop: rect(0.0, 0.0, 24.0, 24.0),
            back: rect(0.0, 0.0, 24.0, 24.0),
        },
    }
}

/// A drawing flattened into polylines, in its design coordinates.
struct Flat {
    strokes: Vec<Vec<Vec2>>,
    dots: Vec<(Vec2, f32)>,
    crop: Rect,
    back: Rect,
}

fn flatten(src: &Source) -> Flat {
    let mut strokes = Vec::new();
    for path in src.paths {
        parse_path(path, &mut strokes);
    }
    let ellipse = |c: Vec2, rx: f32, ry: f32| -> Vec<Vec2> {
        const STEPS: usize = 40;
        (0..=STEPS)
            .map(|i| {
                let a = i as f32 / STEPS as f32 * TAU;
                c + vec2(a.cos() * rx, a.sin() * ry)
            })
            .collect()
    };
    for &(x, y, r) in src.rings {
        strokes.push(ellipse(vec2(x, y), r, r));
    }
    for &(x, y, rx, ry) in src.ellipses {
        strokes.push(ellipse(vec2(x, y), rx, ry));
    }
    Flat {
        strokes,
        dots: src.dots.iter().map(|&(x, y, r)| (vec2(x, y), r)).collect(),
        crop: src.crop,
        back: src.back,
    }
}

/// Parses absolute M/L/C/Z path data into polylines, one per subpath.
fn parse_path(d: &str, out: &mut Vec<Vec<Vec2>>) {
    const CURVE_STEPS: usize = 12;
    // Split letters from numbers so "M30 38" tokenises as "M", "30", "38".
    let spaced: String = d
        .chars()
        .flat_map(|c| {
            if c.is_ascii_alphabetic() {
                vec![' ', c, ' ']
            } else {
                vec![c]
            }
        })
        .collect();
    let mut tokens = spaced.split_whitespace();
    fn num<'a>(tokens: &mut impl Iterator<Item = &'a str>) -> f32 {
        tokens
            .next()
            .and_then(|t| t.parse().ok())
            .expect("art path: number expected")
    }
    let mut current: Vec<Vec2> = Vec::new();
    while let Some(cmd) = tokens.next() {
        match cmd {
            "M" => {
                if current.len() > 1 {
                    out.push(std::mem::take(&mut current));
                }
                current.clear();
                let p = vec2(num(&mut tokens), num(&mut tokens));
                current.push(p);
            }
            "L" => {
                let p = vec2(num(&mut tokens), num(&mut tokens));
                current.push(p);
            }
            "C" => {
                let p0 = *current.last().expect("art path: C needs a start point");
                let c1 = vec2(num(&mut tokens), num(&mut tokens));
                let c2 = vec2(num(&mut tokens), num(&mut tokens));
                let p3 = vec2(num(&mut tokens), num(&mut tokens));
                for i in 1..=CURVE_STEPS {
                    let t = i as f32 / CURVE_STEPS as f32;
                    let u = 1.0 - t;
                    current.push(
                        p0 * (u * u * u)
                            + c1 * (3.0 * u * u * t)
                            + c2 * (3.0 * u * t * t)
                            + p3 * (t * t * t),
                    );
                }
            }
            "Z" => {
                if let Some(&first) = current.first() {
                    current.push(first);
                }
            }
            other => panic!("art path: unsupported command {other:?}"),
        }
    }
    if current.len() > 1 {
        out.push(current);
    }
}

fn flat(animal: Animal) -> &'static Flat {
    static CACHE: OnceLock<Vec<Flat>> = OnceLock::new();
    let all = CACHE.get_or_init(|| ALL.iter().map(|&a| flatten(&source(a))).collect());
    &all[ALL
        .iter()
        .position(|&a| a == animal)
        .expect("animal is listed")]
}

/// Draws the animal tightly fitted into `dst`, keeping its proportions.
pub fn draw_icon(animal: Animal, dst: Rect, stroke: f32, color: Color) {
    let f = flat(animal);
    draw(f, f.crop, dst, stroke, color);
}

/// Draws the animal placed as on a card back filling `dst`.
pub fn draw_back(animal: Animal, dst: Rect, stroke: f32, color: Color) {
    let f = flat(animal);
    draw(f, f.back, dst, stroke, color);
}

/// Maps `view` (design coordinates) into `dst`, centred, scaled to fit.
fn draw(f: &Flat, view: Rect, dst: Rect, stroke: f32, color: Color) {
    let scale = (dst.w / view.w).min(dst.h / view.h);
    let offset = vec2(
        dst.x + (dst.w - view.w * scale) / 2.0 - view.x * scale,
        dst.y + (dst.h - view.h * scale) / 2.0 - view.y * scale,
    );
    let map = |p: Vec2| offset + p * scale;
    let half = stroke / 2.0;
    for line in &f.strokes {
        let mut prev: Option<Vec2> = None;
        for &p in line {
            let q = map(p);
            if let Some(a) = prev {
                draw_line(a.x, a.y, q.x, q.y, stroke, color);
            }
            // Round joins and caps.
            draw_poly(q.x, q.y, 10, half, 0.0, color);
            prev = Some(q);
        }
    }
    for &(c, r) in &f.dots {
        let q = map(c);
        draw_poly(q.x, q.y, 14, r * scale, 0.0, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_drawing_parses() {
        for a in ALL {
            let f = flatten(&source(a));
            assert!(!f.strokes.is_empty(), "{a:?}");
            assert!(f.strokes.iter().all(|s| s.len() >= 2), "{a:?}");
        }
    }

    #[test]
    fn subpaths_split_on_move() {
        let mut out = Vec::new();
        parse_path("M0 0 L1 1 M2 2 L3 3", &mut out);
        assert_eq!(out.len(), 2);
    }
}
