//! The equalizer: ten bands (31 Hz to 16 kHz, ±12 dB), the presets, and the
//! biquad maths (RBJ Audio EQ Cookbook peaking filters).
//!
//! This is the one table and the one filter module. The Mac and web players
//! hand `EQ_BANDS` to Web Audio `BiquadFilterNode`s ("peaking" there is the
//! same cookbook formula), the iPhone player runs `EqProcessor` in its
//! `MTAudioProcessingTap`, and the Equalizer page draws its curve from
//! `response_db`. Nothing here allocates once built, so the iPhone tap can
//! call it from the audio thread.

// The processor and `live` only run in the iPhone build (and the tests).
#![cfg_attr(not(target_os = "ios"), allow(dead_code))]

use serde::{Deserialize, Serialize};

/// One band of the graphic EQ.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EqBand {
    pub freq: f64,
    /// Short label under the slider ("31", "1k").
    pub label: &'static str,
    /// Spoken label ("31 hertz", "1 kilohertz").
    pub spoken: &'static str,
}

const fn band(freq: f64, label: &'static str, spoken: &'static str) -> EqBand {
    EqBand { freq, label, spoken }
}

pub const EQ_BAND_COUNT: usize = 10;

/// Octave-spaced centre frequencies.
pub const EQ_BANDS: [EqBand; EQ_BAND_COUNT] = [
    band(31.0, "31", "31 hertz"),
    band(62.0, "62", "62 hertz"),
    band(125.0, "125", "125 hertz"),
    band(250.0, "250", "250 hertz"),
    band(500.0, "500", "500 hertz"),
    band(1000.0, "1k", "1 kilohertz"),
    band(2000.0, "2k", "2 kilohertz"),
    band(4000.0, "4k", "4 kilohertz"),
    band(8000.0, "8k", "8 kilohertz"),
    band(16000.0, "16k", "16 kilohertz"),
];

/// Every band's Q: about one octave wide, so neighbours blend smoothly.
pub const EQ_BAND_Q: f64 = std::f64::consts::SQRT_2;
/// Band gains run from -12 to +12 dB.
pub const EQ_GAIN_LIMIT_DB: f64 = 12.0;
/// Pre-gain runs from -12 to 0 dB (it only ever turns the signal down).
pub const EQ_PREAMP_MIN_DB: f64 = -12.0;
/// Sample rate the page's curve (and the automatic pre-gain) is worked out at.
const REFERENCE_RATE: f64 = 48_000.0;

type Gains = [f64; EQ_BAND_COUNT];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EqPreset {
    #[default]
    Flat,
    BassBoost,
    TrebleBoost,
    Vocal,
    Rock,
    Electronic,
    Acoustic,
    /// Gentle loudness: a little more low and high end, for quiet listening.
    LateNight,
    Custom,
}

impl EqPreset {
    pub const ALL: [EqPreset; 9] = [
        EqPreset::Flat,
        EqPreset::BassBoost,
        EqPreset::TrebleBoost,
        EqPreset::Vocal,
        EqPreset::Rock,
        EqPreset::Electronic,
        EqPreset::Acoustic,
        EqPreset::LateNight,
        EqPreset::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EqPreset::Flat => "Flat",
            EqPreset::BassBoost => "Bass Boost",
            EqPreset::TrebleBoost => "Treble Boost",
            EqPreset::Vocal => "Vocal",
            EqPreset::Rock => "Rock",
            EqPreset::Electronic => "Electronic",
            EqPreset::Acoustic => "Acoustic",
            EqPreset::LateNight => "Late Night",
            EqPreset::Custom => "Custom",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            EqPreset::Flat => "flat",
            EqPreset::BassBoost => "bass_boost",
            EqPreset::TrebleBoost => "treble_boost",
            EqPreset::Vocal => "vocal",
            EqPreset::Rock => "rock",
            EqPreset::Electronic => "electronic",
            EqPreset::Acoustic => "acoustic",
            EqPreset::LateNight => "late_night",
            EqPreset::Custom => "custom",
        }
    }

    /// The preset's band gains in dB; `None` for Custom (the user's own).
    pub fn gains(self) -> Option<Gains> {
        Some(match self {
            EqPreset::Flat => [0.0; EQ_BAND_COUNT],
            EqPreset::BassBoost => [6.0, 5.0, 4.0, 2.5, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            EqPreset::TrebleBoost => [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.5, 4.0, 5.0, 6.0],
            EqPreset::Vocal => [-2.0, -2.0, -1.0, 1.0, 3.0, 4.0, 3.0, 1.0, 0.0, -1.0],
            EqPreset::Rock => [5.0, 4.0, 3.0, 1.0, -1.0, -1.0, 1.0, 3.0, 4.0, 5.0],
            EqPreset::Electronic => [5.0, 4.0, 1.5, 0.0, -2.0, 1.5, 1.0, 2.0, 4.0, 5.0],
            EqPreset::Acoustic => [4.0, 4.0, 3.0, 1.0, 2.0, 1.5, 3.0, 3.5, 3.0, 2.0],
            EqPreset::LateNight => [4.0, 3.0, 1.5, 0.0, -1.0, -1.0, 0.0, 1.5, 2.5, 3.0],
            EqPreset::Custom => return None,
        })
    }
}

/// The equalizer setting (`AppSettings::equalizer`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EqSettings {
    pub enabled: bool,
    pub preset: EqPreset,
    /// The Custom preset's gains, in dB.
    pub custom: Gains,
    /// Work the pre-gain out from the curve (enough to keep its peak at 0 dB).
    pub auto_preamp: bool,
    /// Pre-gain in dB when `auto_preamp` is off.
    pub preamp_db: f64,
}

impl Default for EqSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            preset: EqPreset::Flat,
            custom: [0.0; EQ_BAND_COUNT],
            auto_preamp: true,
            preamp_db: 0.0,
        }
    }
}

fn clean_gain(db: f64) -> f64 {
    if db.is_finite() {
        db.clamp(-EQ_GAIN_LIMIT_DB, EQ_GAIN_LIMIT_DB)
    } else {
        0.0
    }
}

fn clean_preamp(db: f64) -> f64 {
    if db.is_finite() {
        db.clamp(EQ_PREAMP_MIN_DB, 0.0)
    } else {
        0.0
    }
}

impl EqSettings {
    /// The band gains in use: the preset's, or the custom ones.
    pub fn gains(&self) -> Gains {
        self.preset
            .gains()
            .unwrap_or(self.custom)
            .map(clean_gain)
    }

    /// The pre-gain in use, in dB (never above 0).
    pub fn preamp(&self) -> f64 {
        if self.auto_preamp {
            auto_preamp_db(&self.gains())
        } else {
            clean_preamp(self.preamp_db)
        }
    }

    /// Picks a preset. Custom starts from the curve that was showing.
    pub fn set_preset(&mut self, preset: EqPreset) {
        if preset == EqPreset::Custom && self.preset != EqPreset::Custom {
            self.custom = self.gains();
        }
        self.preset = preset;
    }

    /// Moves one band. A preset turns into Custom, starting from its curve.
    pub fn set_band(&mut self, index: usize, db: f64) {
        if index >= EQ_BAND_COUNT {
            return;
        }
        self.set_preset(EqPreset::Custom);
        self.custom[index] = (clean_gain(db) * 2.0).round() / 2.0;
    }

    /// Sets a manual pre-gain (turns the automatic one off).
    pub fn set_preamp(&mut self, db: f64) {
        self.auto_preamp = false;
        self.preamp_db = (clean_preamp(db) * 2.0).round() / 2.0;
    }

    /// "Off", or the preset's name.
    pub fn summary(&self) -> &'static str {
        if self.enabled {
            self.preset.label()
        } else {
            "Off"
        }
    }

    /// Whether the boosts can push loud songs past full scale at this pre-gain.
    pub fn may_clip(&self) -> bool {
        peak_db(&self.gains()) + self.preamp() > 0.25
    }
}

/// "+3 dB", "0 dB", "-4.5 dB".
pub fn db_label(db: f64) -> String {
    let rounded = (db * 2.0).round() / 2.0;
    let text = if rounded.fract() == 0.0 {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}")
    };
    if rounded > 0.0 {
        format!("+{text} dB")
    } else if rounded == 0.0 {
        "0 dB".to_string()
    } else {
        format!("{text} dB")
    }
}

// ---- Filters ---------------------------------------------------------------

/// A biquad's coefficients, normalised so a0 = 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BiquadCoeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl BiquadCoeffs {
    pub const IDENTITY: BiquadCoeffs = BiquadCoeffs { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };

    /// The cookbook peaking EQ. A band at or above 0.45 × the sample rate
    /// (16 kHz on a 32 kHz stream) can't be built and passes audio through.
    pub fn peaking(freq: f64, q: f64, gain_db: f64, sample_rate: f64) -> BiquadCoeffs {
        if gain_db == 0.0 || !(sample_rate > 0.0) || freq <= 0.0 || freq >= sample_rate * 0.45 {
            return BiquadCoeffs::IDENTITY;
        }
        let a = 10f64.powf(gain_db / 40.0);
        let w0 = std::f64::consts::TAU * freq / sample_rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha / a;
        BiquadCoeffs {
            b0: (1.0 + alpha * a) / a0,
            b1: (-2.0 * cos) / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: (-2.0 * cos) / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }

    /// The filter's gain at `freq`, in dB.
    pub fn magnitude_db(&self, freq: f64, sample_rate: f64) -> f64 {
        let w = std::f64::consts::TAU * freq / sample_rate;
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        let num_re = self.b0 + self.b1 * c1 + self.b2 * c2;
        let num_im = -(self.b1 * s1 + self.b2 * s2);
        let den_re = 1.0 + self.a1 * c1 + self.a2 * c2;
        let den_im = -(self.a1 * s1 + self.a2 * s2);
        let num = num_re * num_re + num_im * num_im;
        let den = den_re * den_re + den_im * den_im;
        10.0 * (num / den).log10()
    }
}

/// The whole curve's gain at `freq` (no pre-gain), in dB.
pub fn response_db(gains: &Gains, freq: f64) -> f64 {
    EQ_BANDS
        .iter()
        .zip(gains)
        .map(|(band, &gain)| {
            BiquadCoeffs::peaking(band.freq, EQ_BAND_Q, gain, REFERENCE_RATE).magnitude_db(freq, REFERENCE_RATE)
        })
        .sum()
}

/// `count` points of the curve, log-spaced from 20 Hz to 20 kHz.
pub fn curve_points(gains: &Gains, count: usize) -> Vec<(f64, f64)> {
    let count = count.max(2);
    let (lo, hi) = (20f64.ln(), 20_000f64.ln());
    (0..count)
        .map(|i| {
            let freq = (lo + (hi - lo) * i as f64 / (count - 1) as f64).exp();
            (freq, response_db(gains, freq))
        })
        .collect()
}

/// The curve's highest point, in dB.
fn peak_db(gains: &Gains) -> f64 {
    curve_points(gains, 96)
        .into_iter()
        .map(|(_, db)| db)
        .fold(f64::NEG_INFINITY, f64::max)
}

/// Enough pre-gain to bring the curve's peak to 0 dB, in half-dB steps.
pub fn auto_preamp_db(gains: &Gains) -> f64 {
    let peak = peak_db(gains).max(0.0);
    clean_preamp(-(peak * 2.0).ceil() / 2.0)
}

// ---- Processing (iPhone tap) ------------------------------------------------

/// Channels `EqProcessor` keeps filter state for; more are passed through.
pub const EQ_MAX_CHANNELS: usize = 8;

/// The ten filters and the pre-gain for one stream. Transposed direct form II
/// in f64 (the 31 Hz band's poles sit close to the unit circle).
pub struct EqProcessor {
    coeffs: [BiquadCoeffs; EQ_BAND_COUNT],
    state: [[[f64; 2]; EQ_BAND_COUNT]; EQ_MAX_CHANNELS],
    preamp: f64,
    active: bool,
    sample_rate: f64,
    /// The `live` version these coefficients were built from.
    version: u32,
}

impl Default for EqProcessor {
    fn default() -> Self {
        Self {
            coeffs: [BiquadCoeffs::IDENTITY; EQ_BAND_COUNT],
            state: [[[0.0; 2]; EQ_BAND_COUNT]; EQ_MAX_CHANNELS],
            preamp: 1.0,
            active: false,
            sample_rate: 0.0,
            version: u32::MAX,
        }
    }
}

impl EqProcessor {
    /// Rebuilds the filters. `enabled` false passes audio through untouched.
    pub fn configure(&mut self, enabled: bool, gains: &Gains, preamp_db: f64, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.active = enabled && sample_rate > 0.0;
        self.preamp = 10f64.powf(clean_preamp(preamp_db) / 20.0);
        for (slot, (band, &gain)) in self.coeffs.iter_mut().zip(EQ_BANDS.iter().zip(gains)) {
            *slot = BiquadCoeffs::peaking(band.freq, EQ_BAND_Q, clean_gain(gain), sample_rate);
        }
    }

    /// Clears the filter memory (a seek or a new stream).
    pub fn reset(&mut self) {
        self.state = [[[0.0; 2]; EQ_BAND_COUNT]; EQ_MAX_CHANNELS];
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    #[inline]
    fn run(&mut self, channel: usize, x: f64) -> f64 {
        let mut y = x * self.preamp;
        let state = &mut self.state[channel];
        for (c, z) in self.coeffs.iter().zip(state.iter_mut()) {
            let out = c.b0 * y + z[0];
            z[0] = c.b1 * y - c.a1 * out + z[1];
            z[1] = c.b2 * y - c.a2 * out;
            y = out;
        }
        y
    }

    /// Filters one channel's samples in place (non-interleaved buffers).
    pub fn process_planar(&mut self, channel: usize, samples: &mut [f32]) {
        if !self.active || channel >= EQ_MAX_CHANNELS {
            return;
        }
        for s in samples.iter_mut() {
            *s = self.run(channel, *s as f64) as f32;
        }
    }

    /// Filters interleaved frames in place.
    pub fn process_interleaved(&mut self, samples: &mut [f32], channels: usize) {
        if !self.active || channels == 0 {
            return;
        }
        for frame in samples.chunks_mut(channels) {
            for (channel, s) in frame.iter_mut().enumerate().take(EQ_MAX_CHANNELS) {
                *s = self.run(channel, *s as f64) as f32;
            }
        }
    }
}

/// The setting as the audio thread sees it: atomics, so the tap never locks.
/// `publish` (UI side) bumps the version; `EqProcessor::sync` rebuilds the
/// filters when it changes.
pub mod live {
    use super::{EqProcessor, EqSettings, EQ_BAND_COUNT};
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

    static VERSION: AtomicU32 = AtomicU32::new(0);
    static ENABLED: AtomicBool = AtomicBool::new(false);
    static PREAMP: AtomicU64 = AtomicU64::new(0);
    static GAINS: [AtomicU64; EQ_BAND_COUNT] = [const { AtomicU64::new(0) }; EQ_BAND_COUNT];

    pub fn publish(settings: &EqSettings) {
        ENABLED.store(settings.enabled, Ordering::Relaxed);
        PREAMP.store(settings.preamp().to_bits(), Ordering::Relaxed);
        for (slot, gain) in GAINS.iter().zip(settings.gains()) {
            slot.store(gain.to_bits(), Ordering::Relaxed);
        }
        VERSION.fetch_add(1, Ordering::Release);
    }

    pub fn enabled() -> bool {
        ENABLED.load(Ordering::Relaxed)
    }

    impl EqProcessor {
        /// Picks up a newer setting or a new sample rate. Allocation-free.
        pub fn sync(&mut self, sample_rate: f64) {
            let version = VERSION.load(Ordering::Acquire);
            if version == self.version && sample_rate == self.sample_rate {
                return;
            }
            self.version = version;
            let mut gains = [0.0; EQ_BAND_COUNT];
            for (gain, slot) in gains.iter_mut().zip(GAINS.iter()) {
                *gain = f64::from_bits(slot.load(Ordering::Relaxed));
            }
            let preamp = f64::from_bits(PREAMP.load(Ordering::Relaxed));
            self.configure(enabled(), &gains, preamp, sample_rate);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn bands_are_octaves_from_31_hz_to_16_khz() {
        assert_eq!(EQ_BANDS.len(), 10);
        assert_eq!(EQ_BANDS[0].freq, 31.0);
        assert_eq!(EQ_BANDS[9].freq, 16000.0);
        assert!(EQ_BANDS.windows(2).all(|w| (w[1].freq / w[0].freq - 2.0).abs() < 0.05));
    }

    #[test]
    fn peaking_coefficients_match_the_cookbook() {
        // 1 kHz, Q 1.41421, +6 dB at 48 kHz (worked out independently).
        let c = BiquadCoeffs::peaking(1000.0, EQ_BAND_Q, 6.0, 48_000.0);
        assert!(close(c.b0, 1.031486802693398), "b0 {}", c.b0);
        assert!(close(c.b1, -1.9201576605325716), "b1 {}", c.b1);
        assert!(close(c.b2, 0.9052398225995872), "b2 {}", c.b2);
        assert!(close(c.a1, -1.9201576605325716), "a1 {}", c.a1);
        assert!(close(c.a2, 0.9367266252929856), "a2 {}", c.a2);
        // -12 dB at 31 Hz, 44.1 kHz.
        let c = BiquadCoeffs::peaking(31.0, EQ_BAND_Q, -12.0, 44_100.0);
        assert!(close(c.b0, 0.9976741700476902), "b0 {}", c.b0);
        assert!(close(c.b1, -1.9937684944948213), "b1 {}", c.b1);
        assert!(close(c.b2, 0.9961137715185362), "b2 {}", c.b2);
        assert!(close(c.a2, 0.9937879415662265), "a2 {}", c.a2);
    }

    #[test]
    fn peaking_gain_is_exact_at_the_centre() {
        for gain in [-12.0, -3.0, 4.5, 12.0] {
            let c = BiquadCoeffs::peaking(2000.0, EQ_BAND_Q, gain, 48_000.0);
            assert!((c.magnitude_db(2000.0, 48_000.0) - gain).abs() < 1e-6);
            // Far from the centre the band does (almost) nothing.
            assert!(c.magnitude_db(20.0, 48_000.0).abs() < 0.05);
        }
    }

    #[test]
    fn flat_and_out_of_range_bands_pass_through() {
        assert_eq!(BiquadCoeffs::peaking(1000.0, EQ_BAND_Q, 0.0, 48_000.0), BiquadCoeffs::IDENTITY);
        assert_eq!(BiquadCoeffs::peaking(16_000.0, EQ_BAND_Q, 6.0, 22_050.0), BiquadCoeffs::IDENTITY);
        assert!(response_db(&[0.0; 10], 440.0).abs() < 1e-12);
    }

    #[test]
    fn presets_stay_in_range_and_have_unique_keys() {
        let mut keys = std::collections::HashSet::new();
        for preset in EqPreset::ALL {
            assert!(keys.insert(preset.key()));
            // The key is the serialized name.
            assert_eq!(serde_json::to_string(&preset).unwrap(), format!("\"{}\"", preset.key()));
            if let Some(gains) = preset.gains() {
                assert!(gains.iter().all(|g| g.abs() <= EQ_GAIN_LIMIT_DB));
            }
        }
        assert_eq!(EqPreset::Flat.gains(), Some([0.0; 10]));
        assert_eq!(EqPreset::Custom.gains(), None);
        // Boosts are where the names say.
        let bass = EqPreset::BassBoost.gains().unwrap();
        assert!(bass[0] > bass[9]);
        let treble = EqPreset::TrebleBoost.gains().unwrap();
        assert!(treble[9] > treble[0]);
    }

    #[test]
    fn auto_preamp_keeps_the_peak_at_zero() {
        assert_eq!(auto_preamp_db(&[0.0; 10]), 0.0);
        assert_eq!(auto_preamp_db(&[-6.0; 10]), 0.0);
        for preset in EqPreset::ALL {
            let mut eq = EqSettings { enabled: true, ..EqSettings::default() };
            eq.set_preset(preset);
            assert!(!eq.may_clip(), "{preset:?} clips at {}", eq.preamp());
            assert!(eq.preamp() <= 0.0 && eq.preamp() >= EQ_PREAMP_MIN_DB);
        }
        // Neighbouring boosts add up: more than the biggest single band.
        let bass = EqPreset::BassBoost.gains().unwrap();
        assert!(auto_preamp_db(&bass) < -6.0);
    }

    #[test]
    fn moving_a_band_turns_a_preset_into_custom() {
        let mut eq = EqSettings::default();
        eq.set_preset(EqPreset::Rock);
        eq.set_band(2, 7.3);
        assert_eq!(eq.preset, EqPreset::Custom);
        let mut expected = EqPreset::Rock.gains().unwrap();
        expected[2] = 7.5;
        assert_eq!(eq.gains(), expected);
        eq.set_band(0, 40.0);
        assert_eq!(eq.gains()[0], EQ_GAIN_LIMIT_DB);
        eq.set_preamp(-30.0);
        assert!(!eq.auto_preamp);
        assert_eq!(eq.preamp(), EQ_PREAMP_MIN_DB);
    }

    #[test]
    fn old_settings_and_bad_values_load_safely() {
        let eq: EqSettings = serde_json::from_str(r#"{"enabled":true,"preset":"rock"}"#).unwrap();
        assert_eq!(eq.preset, EqPreset::Rock);
        assert!(eq.auto_preamp);
        let eq = EqSettings { custom: [f64::NAN; 10], preset: EqPreset::Custom, ..EqSettings::default() };
        assert_eq!(eq.gains(), [0.0; 10]);
    }

    #[test]
    fn processor_matches_the_curve_and_passes_through_when_off() {
        let fs = 48_000.0;
        let mut gains = [0.0; 10];
        gains[5] = 6.0; // 1 kHz
        let mut eq = EqProcessor::default();
        eq.configure(true, &gains, 0.0, fs);
        // A 1 kHz sine comes out about 6 dB louder once settled.
        let n = 48_000;
        let mut buf: Vec<f32> = (0..n)
            .map(|i| (std::f64::consts::TAU * 1000.0 * i as f64 / fs).sin() as f32 * 0.25)
            .collect();
        eq.process_planar(0, &mut buf);
        let peak = buf[n / 2..].iter().fold(0f32, |m, s| m.max(s.abs())) as f64;
        let gain = 20.0 * (peak / 0.25).log10();
        assert!((gain - 6.0).abs() < 0.1, "gain {gain}");

        let mut off = EqProcessor::default();
        off.configure(false, &gains, -6.0, fs);
        let mut same = vec![0.5f32, -0.25, 0.125];
        off.process_interleaved(&mut same, 2);
        assert_eq!(same, vec![0.5, -0.25, 0.125]);
    }

    #[test]
    fn labels() {
        assert_eq!(db_label(3.0), "+3 dB");
        assert_eq!(db_label(0.0), "0 dB");
        assert_eq!(db_label(-4.5), "-4.5 dB");
        let mut eq = EqSettings::default();
        assert_eq!(eq.summary(), "Off");
        eq.enabled = true;
        eq.set_preset(EqPreset::LateNight);
        assert_eq!(eq.summary(), "Late Night");
    }
}
