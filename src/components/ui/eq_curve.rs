//! The equalizer's response curve.

use crate::equalizer::{curve_points, response_db, EQ_BANDS, EQ_BAND_COUNT, EQ_GAIN_LIMIT_DB};
use dioxus::prelude::*;

const WIDTH: f64 = 320.0;
const HEIGHT: f64 = 120.0;
const POINTS: usize = 64;

fn x_of(freq: f64) -> f64 {
    let (lo, hi) = (20f64.ln(), 20_000f64.ln());
    (freq.ln() - lo) / (hi - lo) * WIDTH
}

fn y_of(db: f64) -> f64 {
    let db = db.clamp(-EQ_GAIN_LIMIT_DB * 1.5, EQ_GAIN_LIMIT_DB * 1.5);
    HEIGHT / 2.0 - db / (EQ_GAIN_LIMIT_DB * 1.5) * (HEIGHT / 2.0)
}

/// The curve the bands make together (20 Hz–20 kHz, log scale, ±18 dB), as
/// an SVG line over a 0 dB rule, with a dot per band. Decorative for screen
/// readers: the sliders carry the values. `dimmed` while the EQ is off.
#[component]
pub fn EqCurve(gains: [f64; EQ_BAND_COUNT], #[props(default)] dimmed: bool) -> Element {
    let points = curve_points(&gains, POINTS);
    let mut line = String::with_capacity(POINTS * 14);
    for (i, (freq, db)) in points.iter().enumerate() {
        let cmd = if i == 0 { 'M' } else { 'L' };
        line.push_str(&format!("{cmd}{:.1} {:.1} ", x_of(*freq), y_of(*db)));
    }
    let area = format!("{line}L{WIDTH:.1} {:.1} L0 {:.1} Z", HEIGHT / 2.0, HEIGHT / 2.0);
    // Encodes the curve for tests ("31:+6.0 …").
    let signature = EQ_BANDS
        .iter()
        .zip(gains)
        .map(|(band, gain)| format!("{}:{gain:+.1}", band.label))
        .collect::<Vec<_>>()
        .join(" ");

    rsx! {
        svg {
            class: "rs-eq-curve",
            class: if dimmed { "rs-eq-curve--off" },
            view_box: "0 0 {WIDTH} {HEIGHT}",
            preserve_aspect_ratio: "none",
            "aria-hidden": "true",
            "data-testid": "eq-curve",
            "data-gains": "{signature}",
            line { class: "rs-eq-curve__zero", x1: "0", y1: "{HEIGHT / 2.0}", x2: "{WIDTH}", y2: "{HEIGHT / 2.0}" }
            path { class: "rs-eq-curve__area", d: "{area}" }
            path { class: "rs-eq-curve__line", d: "{line}" }
            for band in EQ_BANDS.iter() {
                circle {
                    key: "{band.label}",
                    class: "rs-eq-curve__dot",
                    cx: "{x_of(band.freq):.1}",
                    cy: "{y_of(response_db(&gains, band.freq)):.1}",
                    r: "2.5",
                }
            }
        }
    }
}
