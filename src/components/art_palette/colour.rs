//! Colour maths for the album-art palette: median cut over a downsampled
//! cover, then lightness adjusted until each accent meets WCAG AA on the
//! backgrounds it is drawn on. Pure functions, no Dioxus, so it is tested
//! here and runs anywhere (a blocking thread on native, the task on web).

/// An sRGB colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const WHITE: Rgb = Rgb(255, 255, 255);

    /// `#rrggbb`.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    /// Parses `#rrggbb` (the `#` is optional).
    pub fn from_hex(raw: &str) -> Option<Rgb> {
        let raw = raw.trim().trim_start_matches('#');
        if raw.len() != 6 || !raw.is_ascii() {
            return None;
        }
        let channel = |at: usize| u8::from_str_radix(&raw[at..at + 2], 16).ok();
        Some(Rgb(channel(0)?, channel(2)?, channel(4)?))
    }

    /// WCAG 2 relative luminance, 0 (black) to 1 (white).
    pub fn luminance(self) -> f64 {
        let linear = |c: u8| {
            let c = c as f64 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(self.0) + 0.7152 * linear(self.1) + 0.0722 * linear(self.2)
    }

    /// Max minus min channel, 0 to 1: how colourful it is, regardless of
    /// lightness (HSL saturation calls near-black reds "saturated").
    pub fn chroma(self) -> f64 {
        let max = self.0.max(self.1).max(self.2) as f64;
        let min = self.0.min(self.1).min(self.2) as f64;
        (max - min) / 255.0
    }

    fn to_hsl(self) -> (f64, f64, f64) {
        let (r, g, b) = (self.0 as f64 / 255.0, self.1 as f64 / 255.0, self.2 as f64 / 255.0);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;
        if (max - min).abs() < f64::EPSILON {
            return (0.0, 0.0, l);
        }
        let d = max - min;
        let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
        let h = if max == r {
            (g - b) / d + if g < b { 6.0 } else { 0.0 }
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        (h / 6.0, s, l)
    }

    fn from_hsl(h: f64, s: f64, l: f64) -> Rgb {
        let l = l.clamp(0.0, 1.0);
        if s <= 0.0 {
            let v = (l * 255.0).round() as u8;
            return Rgb(v, v, v);
        }
        let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
        let p = 2.0 * l - q;
        let hue = |mut t: f64| {
            if t < 0.0 {
                t += 1.0;
            }
            if t > 1.0 {
                t -= 1.0;
            }
            if t < 1.0 / 6.0 {
                p + (q - p) * 6.0 * t
            } else if t < 0.5 {
                q
            } else if t < 2.0 / 3.0 {
                p + (q - p) * (2.0 / 3.0 - t) * 6.0
            } else {
                p
            }
        };
        let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Rgb(byte(hue(h + 1.0 / 3.0)), byte(hue(h)), byte(hue(h - 1.0 / 3.0)))
    }

    fn with_lightness(self, l: f64) -> Rgb {
        let (h, s, _) = self.to_hsl();
        Rgb::from_hsl(h, s, l)
    }
}

/// WCAG contrast ratio, 1 to 21.
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (a.luminance(), b.luminance());
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// `share` of `b` over `a`, in sRGB (what CSS `color-mix(in srgb, …)` does).
pub fn mix(a: Rgb, b: Rgb, share: f64) -> Rgb {
    let t = share.clamp(0.0, 1.0);
    let channel = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
    Rgb(channel(a.0, b.0), channel(a.1, b.1), channel(a.2, b.2))
}

/// WCAG AA for body text.
pub const AA_TEXT: f64 = 4.5;
/// The playing row's wash: the accent at 12% over the list (components.css).
const ROW_WASH: f64 = 0.12;

/// Where an accent is drawn, and so what it must stay readable on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ground {
    /// The black stage: Now Playing, the mini player, the player bar, the
    /// stage panels' rows (`--stage-*` in tokens.css, lightest first).
    Stage,
    /// Light page lists in every theme (`--page` / `--surface`).
    LightPage,
    /// Dark page lists in every theme.
    DarkPage,
}

impl Ground {
    fn backgrounds(self) -> &'static [Rgb] {
        match self {
            Ground::Stage => &[Rgb(0x2a, 0x2b, 0x2f), Rgb(0x26, 0x28, 0x2c), Rgb(0x1b, 0x1c, 0x1f), Rgb(0, 0, 0)],
            Ground::LightPage => &[
                Rgb(0xd7, 0xd9, 0xdc),
                Rgb(0xe6, 0xe7, 0xea),
                Rgb(0xe9, 0xe6, 0xe1),
                Rgb(0xec, 0xee, 0xf0),
                Rgb(0xf8, 0xf8, 0xf9),
                Rgb(0xf9, 0xf7, 0xf4),
            ],
            Ground::DarkPage => &[
                Rgb(0x0e, 0x0f, 0x10),
                Rgb(0x18, 0x19, 0x1b),
                Rgb(0x1a, 0x18, 0x16),
                Rgb(0x22, 0x23, 0x26),
                Rgb(0x24, 0x21, 0x1e),
            ],
        }
    }

    fn is_dark(self) -> bool {
        !matches!(self, Ground::LightPage)
    }
}

/// The worst contrast of `accent` as text on `ground`, counting the
/// playing row's wash of the accent itself.
pub fn worst_contrast(accent: Rgb, ground: Ground) -> f64 {
    ground
        .backgrounds()
        .iter()
        .flat_map(|bg| [*bg, mix(*bg, accent, ROW_WASH)])
        .map(|bg| contrast(accent, bg))
        .fold(f64::INFINITY, f64::min)
}

/// The smallest lightness change (hue and saturation kept) that makes
/// `colour` reach `target` on every background of `ground`: lighter on dark
/// grounds, darker on light ones. Already readable colours come back as is.
pub fn readable_on(colour: Rgb, ground: Ground, target: f64) -> Rgb {
    if worst_contrast(colour, ground) >= target {
        return colour;
    }
    let (_, _, l) = colour.to_hsl();
    let (mut ok, mut bad) = if ground.is_dark() { (1.0, l) } else { (0.0, l) };
    for _ in 0..24 {
        let mid = (ok + bad) / 2.0;
        if worst_contrast(colour.with_lightness(mid), ground) >= target {
            ok = mid;
        } else {
            bad = mid;
        }
    }
    colour.with_lightness(ok)
}

/// The darkest change that makes white text reach `target` on `colour`.
fn under_white(colour: Rgb, target: f64) -> Rgb {
    if contrast(colour, Rgb::WHITE) >= target {
        return colour;
    }
    let (_, _, l) = colour.to_hsl();
    let (mut ok, mut bad) = (0.0, l);
    for _ in 0..24 {
        let mid = (ok + bad) / 2.0;
        if contrast(colour.with_lightness(mid), Rgb::WHITE) >= target {
            ok = mid;
        } else {
            bad = mid;
        }
    }
    colour.with_lightness(ok)
}

/// One colour of the cover and how much of it there is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swatch {
    pub colour: Rgb,
    /// Share of the sampled pixels, 0 to 1.
    pub share: f64,
}

/// Median cut: splits the pixels into up to `boxes` groups along their
/// widest channel and returns each group's average, most common first.
pub fn median_cut(pixels: &[Rgb], boxes: usize) -> Vec<Swatch> {
    if pixels.is_empty() || boxes == 0 {
        return Vec::new();
    }
    let channel = |p: &Rgb, c: usize| match c {
        0 => p.0,
        1 => p.1,
        _ => p.2,
    };
    // Widest channel and its range.
    let widest = |group: &[Rgb]| {
        (0..3)
            .map(|c| {
                let (lo, hi) = group
                    .iter()
                    .fold((255u8, 0u8), |(lo, hi), p| (lo.min(channel(p, c)), hi.max(channel(p, c))));
                (c, hi.saturating_sub(lo))
            })
            .max_by_key(|(_, range)| *range)
            .unwrap_or((0, 0))
    };
    let mut groups: Vec<Vec<Rgb>> = vec![pixels.to_vec()];
    while groups.len() < boxes {
        // Split the group where it matters most: many pixels, wide range.
        let pick = groups
            .iter()
            .enumerate()
            .filter(|(_, g)| g.len() > 1)
            .map(|(i, g)| (i, g.len() as u64 * widest(g).1 as u64))
            .filter(|(_, score)| *score > 0)
            .max_by_key(|(_, score)| *score);
        let Some((index, _)) = pick else { break };
        let mut group = groups.swap_remove(index);
        let (c, _) = widest(&group);
        group.sort_unstable_by_key(|p| channel(p, c));
        // Cut at the median value, keeping equal values on one side, so a
        // flat colour never ends up averaged with its neighbour.
        let median = channel(&group[group.len() / 2], c);
        let mut at = group.partition_point(|p| channel(p, c) < median);
        if at == 0 {
            at = group.partition_point(|p| channel(p, c) <= median);
        }
        let upper = group.split_off(at);
        groups.push(group);
        groups.push(upper);
    }
    let total = pixels.len() as f64;
    let mut swatches: Vec<Swatch> = groups
        .iter()
        .filter(|g| !g.is_empty())
        .map(|g| {
            let n = g.len() as u64;
            let sum = g.iter().fold((0u64, 0u64, 0u64), |s, p| {
                (s.0 + p.0 as u64, s.1 + p.1 as u64, s.2 + p.2 as u64)
            });
            Swatch {
                colour: Rgb((sum.0 / n) as u8, (sum.1 / n) as u8, (sum.2 / n) as u8),
                share: g.len() as f64 / total,
            }
        })
        .collect();
    swatches.sort_by(|a, b| b.share.total_cmp(&a.share));
    swatches
}

/// The colours the app takes from a cover. Every field is ready to use:
/// the accents are already adjusted for AA on their ground.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArtPalette {
    /// Main colour field (Now Playing's wash, ambient mode). White text
    /// stays at 7:1 on it.
    pub glow: Rgb,
    /// Second field colour for ambient mode's drift. Same rule as `glow`.
    pub tint: Rgb,
    /// Accent on the black stage (progress fills, stage rows).
    pub stage: Rgb,
    /// Accent on light page lists (the playing row).
    pub light: Rgb,
    /// Accent on dark page lists.
    pub dark: Rgb,
}

/// How many groups median cut makes.
const BOXES: usize = 8;
/// Below this chroma a swatch counts as grey.
const GREY: f64 = 0.12;
/// Swatches under this share are specks, not colours of the cover.
const SPECK: f64 = 0.03;
/// Contrast of white text on the field colours.
const FIELD_TEXT: f64 = 7.0;

impl ArtPalette {
    /// The palette of a cover from its pixels (any count; 32×32 is plenty).
    /// None for an empty image.
    pub fn from_pixels(pixels: &[Rgb]) -> Option<ArtPalette> {
        let swatches = median_cut(pixels, BOXES);
        let dominant = *swatches.first()?;
        // The accent: a colour that is both vivid and really there.
        let score = |s: &Swatch| s.share.sqrt() * (0.15 + s.colour.chroma());
        let candidates: Vec<&Swatch> = swatches.iter().filter(|s| s.share >= SPECK).collect();
        let accent = candidates
            .iter()
            .copied()
            .max_by(|a, b| score(a).total_cmp(&score(b)))
            .copied()
            .unwrap_or(dominant);
        // The field follows the cover's main colour, unless that is grey
        // (a black sleeve with one red mark glows red, not black).
        let glow = if dominant.colour.chroma() >= GREY { dominant } else { accent };
        let tint = candidates
            .iter()
            .copied()
            .filter(|s| s.colour != glow.colour)
            .max_by(|a, b| score(a).total_cmp(&score(b)))
            .copied()
            .unwrap_or(glow);
        Some(ArtPalette {
            glow: under_white(glow.colour, FIELD_TEXT),
            tint: under_white(tint.colour, FIELD_TEXT),
            stage: readable_on(accent.colour, Ground::Stage, AA_TEXT),
            light: readable_on(accent.colour, Ground::LightPage, AA_TEXT),
            dark: readable_on(accent.colour, Ground::DarkPage, AA_TEXT),
        })
    }

    /// Pixels as the sampler sends them: `[r, g, b, r, g, b, …]`.
    pub fn from_rgb_bytes(bytes: &[u8]) -> Option<ArtPalette> {
        let pixels: Vec<Rgb> = bytes.chunks_exact(3).map(|c| Rgb(c[0], c[1], c[2])).collect();
        Self::from_pixels(&pixels)
    }

    /// One line for the persistent cache: five hex colours.
    pub fn to_compact(self) -> String {
        [self.glow, self.tint, self.stage, self.light, self.dark]
            .map(Rgb::hex)
            .join(" ")
    }

    pub fn from_compact(raw: &str) -> Option<ArtPalette> {
        let mut parts = raw.split_whitespace().map(Rgb::from_hex);
        let palette = ArtPalette {
            glow: parts.next()??,
            tint: parts.next()??,
            stage: parts.next()??,
            light: parts.next()??,
            dark: parts.next()??,
        };
        parts.next().is_none().then_some(palette)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(parts: &[(Rgb, usize)]) -> Vec<Rgb> {
        parts.iter().flat_map(|(c, n)| std::iter::repeat(*c).take(*n)).collect()
    }

    #[test]
    fn contrast_matches_wcag_reference_values() {
        assert!((contrast(Rgb::WHITE, Rgb(0, 0, 0)) - 21.0).abs() < 1e-9);
        assert!((contrast(Rgb(0x77, 0x77, 0x77), Rgb::WHITE) - 4.48).abs() < 0.01);
        assert_eq!(contrast(Rgb(10, 20, 30), Rgb(10, 20, 30)), 1.0);
    }

    #[test]
    fn hex_round_trips() {
        let c = Rgb(0x2a, 0x7b, 0xe4);
        assert_eq!(c.hex(), "#2a7be4");
        assert_eq!(Rgb::from_hex("#2a7be4"), Some(c));
        assert_eq!(Rgb::from_hex("2A7BE4"), Some(c));
        assert_eq!(Rgb::from_hex("#2a7be"), None);
        assert_eq!(Rgb::from_hex("#zz7be4"), None);
    }

    #[test]
    fn hsl_round_trips_within_a_step() {
        for c in [Rgb(200, 30, 40), Rgb(12, 140, 90), Rgb(90, 90, 90), Rgb(250, 240, 10)] {
            let (h, s, l) = c.to_hsl();
            let back = Rgb::from_hsl(h, s, l);
            assert!((back.0 as i32 - c.0 as i32).abs() <= 1, "{c:?} -> {back:?}");
            assert!((back.1 as i32 - c.1 as i32).abs() <= 1, "{c:?} -> {back:?}");
            assert!((back.2 as i32 - c.2 as i32).abs() <= 1, "{c:?} -> {back:?}");
        }
    }

    #[test]
    fn median_cut_finds_the_dominant_colour() {
        let red = Rgb(200, 30, 40);
        let blue = Rgb(20, 40, 180);
        let swatches = median_cut(&image(&[(red, 700), (blue, 324)]), 8);
        assert_eq!(swatches[0].colour, red);
        assert!((swatches[0].share - 700.0 / 1024.0).abs() < 1e-9);
        assert!(swatches.iter().any(|s| s.colour == blue));
        let total: f64 = swatches.iter().map(|s| s.share).sum();
        assert!((total - 1.0).abs() < 1e-9);
    }

    #[test]
    fn median_cut_averages_a_noisy_field() {
        // A green field with ±10 noise per channel averages back to green.
        let pixels: Vec<Rgb> = (0..1024)
            .map(|i| {
                let n = (i * 7 % 21) as i32 - 10;
                Rgb((40 + n) as u8, (160 - n) as u8, (70 + n / 2) as u8)
            })
            .collect();
        let swatches = median_cut(&pixels, 4);
        let top = swatches[0].colour;
        assert!(top.1 > 140 && top.0 < 60 && top.2 < 90, "{top:?}");
    }

    #[test]
    fn median_cut_handles_edge_cases() {
        assert!(median_cut(&[], 8).is_empty());
        let one = median_cut(&[Rgb(1, 2, 3)], 8);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].share, 1.0);
        // A flat image can't be split.
        assert_eq!(median_cut(&image(&[(Rgb(9, 9, 9), 64)]), 8).len(), 1);
    }

    #[test]
    fn accent_prefers_a_vivid_colour_over_a_grey_majority() {
        // Mostly black sleeve with an orange mark.
        let orange = Rgb(240, 120, 20);
        let palette = ArtPalette::from_pixels(&image(&[(Rgb(12, 12, 14), 800), (orange, 224)])).unwrap();
        let (h, _, _) = palette.stage.to_hsl();
        let (orange_h, _, _) = orange.to_hsl();
        assert!((h - orange_h).abs() < 0.02, "stage accent keeps the hue: {:?}", palette.stage);
        // The glow is orange too (the black majority is grey).
        assert!(palette.glow.0 > palette.glow.2, "{:?}", palette.glow);
    }

    #[test]
    fn accents_meet_aa_on_their_grounds_for_any_cover() {
        let covers = [
            Rgb(255, 255, 255),
            Rgb(250, 240, 200),
            Rgb(255, 230, 0),
            Rgb(0, 0, 0),
            Rgb(10, 10, 60),
            Rgb(200, 30, 40),
            Rgb(30, 200, 90),
            Rgb(128, 128, 128),
            Rgb(42, 123, 228),
        ];
        for cover in covers {
            let p = ArtPalette::from_pixels(&image(&[(cover, 1024)])).unwrap();
            assert!(worst_contrast(p.stage, Ground::Stage) >= AA_TEXT, "{cover:?} stage {:?}", p.stage);
            assert!(worst_contrast(p.light, Ground::LightPage) >= AA_TEXT, "{cover:?} light {:?}", p.light);
            assert!(worst_contrast(p.dark, Ground::DarkPage) >= AA_TEXT, "{cover:?} dark {:?}", p.dark);
            assert!(contrast(p.glow, Rgb::WHITE) >= FIELD_TEXT, "{cover:?} glow {:?}", p.glow);
            assert!(contrast(p.tint, Rgb::WHITE) >= FIELD_TEXT, "{cover:?} tint {:?}", p.tint);
        }
    }

    #[test]
    fn readable_colours_are_left_alone_and_others_move_the_right_way() {
        // Already fine on the stage: unchanged.
        let pale = Rgb(180, 210, 250);
        assert_eq!(readable_on(pale, Ground::Stage, AA_TEXT), pale);
        // Dark navy gets lighter on the stage, yellow gets darker on paper.
        let navy = Rgb(10, 20, 90);
        assert!(readable_on(navy, Ground::Stage, AA_TEXT).luminance() > navy.luminance());
        let yellow = Rgb(255, 230, 0);
        assert!(readable_on(yellow, Ground::LightPage, AA_TEXT).luminance() < yellow.luminance());
    }

    #[test]
    fn adjustment_is_minimal() {
        // Just past the threshold, not all the way to white.
        let navy = Rgb(10, 20, 90);
        let fixed = readable_on(navy, Ground::Stage, AA_TEXT);
        assert!(worst_contrast(fixed, Ground::Stage) < AA_TEXT + 0.3, "{fixed:?}");
        assert_ne!(fixed, Rgb::WHITE);
    }

    #[test]
    fn compact_form_round_trips() {
        let p = ArtPalette::from_pixels(&image(&[(Rgb(200, 30, 40), 600), (Rgb(20, 40, 180), 424)])).unwrap();
        let line = p.to_compact();
        assert_eq!(line.split(' ').count(), 5);
        assert_eq!(ArtPalette::from_compact(&line), Some(p));
        assert_eq!(ArtPalette::from_compact("#000000 #111111"), None);
        assert_eq!(ArtPalette::from_compact(&format!("{line} #000000")), None);
    }

    #[test]
    fn rgb_bytes_parse_in_triples() {
        let bytes = [200u8, 30, 40].repeat(100);
        let p = ArtPalette::from_rgb_bytes(&bytes).unwrap();
        assert!(p.stage.0 > p.stage.2);
        assert_eq!(ArtPalette::from_rgb_bytes(&[]), None);
    }
}
