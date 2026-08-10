//! Microphone capture, and the resampling that gets it to whisper's 16 kHz.
//!
//! Capture stays in the **GUI process**: device enumeration, the OS permission
//! prompt and a live level meter all belong where the user is. Only the finished
//! audio crosses to the transcription worker.
//!
//! ## The resampler is hand-written
//!
//! Input devices run at 44.1 or 48 kHz; whisper wants exactly 16 kHz. Dropping every
//! third sample would be the obvious conversion and the wrong one: everything above
//! 8 kHz folds back into the audible band as aliasing, and sibilants — the part of
//! speech with the most energy up there — turn into noise spread across the
//! consonants. Recognition quality degrades in a way that looks like a bad model.
//!
//! So the conversion is a windowed-sinc filter that low-passes and resamples in one
//! pass. It is written here rather than pulled from a crate because the property that
//! matters — that a tone above the new Nyquist limit is *rejected* rather than folded
//! — is then something the tests below actually check.

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use report_core::STT_SAMPLE_RATE;

/// An in-progress recording.
///
/// The `cpal::Stream` is not `Send`, so a `Recorder` has to stay on the thread that
/// created it — in practice the UI thread, which is where the button is.
pub struct Recorder {
    stream: cpal::Stream,
    captured: Arc<Mutex<Vec<f32>>>,
    device_rate: u32,
}

impl Recorder {
    /// Open the default input device and start capturing.
    pub fn start() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .context("no microphone available — check the system's input device")?;
        let config = device
            .default_input_config()
            .context("could not read the microphone's configuration")?;

        let device_rate = config.sample_rate();
        let channels = config.channels() as usize;
        let captured: Arc<Mutex<Vec<f32>>> = Arc::default();

        let sink = captured.clone();
        let on_error = |error| tracing::error!("audio: input stream error: {error}");

        // Devices report whichever format they like; anything other than these would
        // need its own conversion, and saying so beats recording silence.
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                config.into(),
                move |data: &[f32], _: &_| push(&sink, data, channels),
                on_error,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_input_stream(
                config.into(),
                move |data: &[i16], _: &_| {
                    let scaled: Vec<f32> =
                        data.iter().map(|s| *s as f32 / i16::MAX as f32).collect();
                    push(&sink, &scaled, channels);
                },
                on_error,
                None,
            ),
            other => anyhow::bail!("unsupported microphone sample format {other:?}"),
        }
        .context("opening the microphone")?;

        stream.play().context("starting the microphone")?;
        tracing::info!("audio: recording at {device_rate} Hz, {channels} channel(s)");

        Ok(Self { stream, captured, device_rate })
    }

    /// Stop and return the audio at 16 kHz mono, ready for whisper.
    pub fn finish(self) -> Result<Vec<f32>> {
        // Explicit rather than relying on the drop order below, so no samples arrive
        // between reading the buffer and releasing the device.
        drop(self.stream);

        let captured = self
            .captured
            .lock()
            .map_err(|_| anyhow::anyhow!("the audio buffer was poisoned by a panic"))?
            .clone();

        Ok(resample(&captured, self.device_rate, STT_SAMPLE_RATE))
    }
}

/// Mix a device buffer down to mono and append it.
fn push(sink: &Arc<Mutex<Vec<f32>>>, data: &[f32], channels: usize) {
    let Ok(mut buffer) = sink.lock() else { return };
    if channels <= 1 {
        buffer.extend_from_slice(data);
        return;
    }
    // Averaged, not just the first channel: on a stereo device the speaker may be
    // closer to one side, and taking one channel would halve the level.
    buffer
        .extend(data.chunks(channels).map(|frame| frame.iter().sum::<f32>() / frame.len() as f32));
}

/// Half-width of the filter kernel, in input samples.
///
/// 32 either side gives a transition band narrow enough that speech is untouched
/// while everything above the new Nyquist is well down — and it costs 64 multiplies
/// per output sample, which for a minute of speech is nothing.
const HALF_TAPS: isize = 32;

/// Resample mono audio, low-passing to prevent aliasing.
///
/// Returns the input untouched when the rates already match, which is the common case
/// on a device that happens to run at 16 kHz.
pub fn resample(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if input.is_empty() || from_rate == 0 || to_rate == 0 {
        return Vec::new();
    }
    if from_rate == to_rate {
        return input.to_vec();
    }

    let ratio = to_rate as f64 / from_rate as f64;
    let out_len = (input.len() as f64 * ratio).floor() as usize;

    // Cut off just below the lower of the two Nyquist limits. When downsampling this
    // is what removes the content that would otherwise fold; when upsampling it
    // limits the interpolation to the band that actually carries signal.
    let nyquist = (from_rate.min(to_rate) as f64) / 2.0;
    let cutoff = 0.45 * 2.0 * nyquist; // 0.9 of the limit, leaving a transition band
                                       // In cycles per *input* sample, which is the domain the kernel is evaluated in.
    let fc = (cutoff / from_rate as f64).min(0.5);

    let mut out = Vec::with_capacity(out_len);
    for n in 0..out_len {
        // Where this output sample sits on the input timeline.
        let position = n as f64 / ratio;
        let centre = position.floor() as isize;

        let mut sum = 0.0f64;
        let mut weight = 0.0f64;
        for k in -HALF_TAPS..=HALF_TAPS {
            let index = centre + k;
            if index < 0 || index as usize >= input.len() {
                continue;
            }
            let distance = position - index as f64;
            let tap = sinc(2.0 * fc * distance) * blackman(distance / (HALF_TAPS as f64 + 1.0));
            sum += input[index as usize] as f64 * tap;
            weight += tap;
        }
        // Normalised by the weight actually used, so samples near the ends — where
        // the kernel is clipped — keep their level instead of fading out.
        out.push(if weight.abs() > 1e-9 { (sum / weight) as f32 } else { 0.0 });
    }
    out
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-9 {
        return 1.0;
    }
    let pi_x = std::f64::consts::PI * x;
    pi_x.sin() / pi_x
}

/// Blackman window over `t` in [-1, 1]; zero outside.
fn blackman(t: f64) -> f64 {
    if t.abs() >= 1.0 {
        return 0.0;
    }
    let x = std::f64::consts::PI * (t + 1.0);
    0.42 - 0.5 * x.cos() + 0.08 * (2.0 * x).cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sine at `freq` Hz sampled at `rate`.
    fn tone(freq: f64, rate: u32, samples: usize) -> Vec<f32> {
        (0..samples)
            .map(|n| (2.0 * std::f64::consts::PI * freq * n as f64 / rate as f64).sin() as f32)
            .collect()
    }

    /// Peak amplitude, ignoring the filter's edge transients at either end.
    fn amplitude(signal: &[f32]) -> f32 {
        let margin = HALF_TAPS as usize * 2;
        if signal.len() <= margin * 2 {
            return 0.0;
        }
        signal[margin..signal.len() - margin].iter().fold(0.0f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn matching_rates_pass_through_untouched() {
        let input = tone(440.0, 16_000, 1000);
        assert_eq!(resample(&input, 16_000, 16_000), input);
    }

    #[test]
    fn the_output_length_follows_the_rate_ratio() {
        let input = tone(440.0, 48_000, 48_000);
        let output = resample(&input, 48_000, 16_000);
        assert!(
            (output.len() as i64 - 16_000).abs() <= 1,
            "one second in must be one second out, got {}",
            output.len()
        );
    }

    #[test]
    fn speech_band_tones_survive_at_full_level() {
        // 1 kHz is squarely in the range that carries speech; it must come through
        // essentially untouched.
        for rate in [44_100u32, 48_000] {
            let input = tone(1_000.0, rate, rate as usize);
            let output = resample(&input, rate, 16_000);
            let level = amplitude(&output);
            assert!(level > 0.9, "1 kHz lost {level} of its level at {rate} Hz");
            assert!(level < 1.1, "1 kHz gained level ({level}) at {rate} Hz");
        }
    }

    #[test]
    fn tones_above_the_new_nyquist_are_rejected_rather_than_folded() {
        // The whole point of filtering before decimating. A 12 kHz tone at 48 kHz
        // would fold to 4 kHz — right in the middle of speech — at full amplitude if
        // the input were simply decimated 3:1.
        let input = tone(12_000.0, 48_000, 48_000);
        let output = resample(&input, 48_000, 16_000);
        let level = amplitude(&output);
        assert!(level < 0.05, "12 kHz aliased through at {level}; the filter is not working");
    }

    #[test]
    fn a_tone_just_below_the_limit_is_attenuated_not_passed() {
        // 7.5 kHz sits inside the transition band; it should be well down but need
        // not be gone.
        let input = tone(7_500.0, 48_000, 48_000);
        assert!(amplitude(&resample(&input, 48_000, 16_000)) < 0.7);
    }

    #[test]
    fn a_constant_signal_keeps_its_value() {
        // Catches a mis-normalised kernel, which would show up as a gain error on
        // everything.
        let input = vec![0.5f32; 48_000];
        let output = resample(&input, 48_000, 16_000);
        let middle = &output[100..output.len() - 100];
        for sample in middle {
            assert!((sample - 0.5).abs() < 0.01, "DC gain drifted: {sample}");
        }
    }

    #[test]
    fn the_edges_keep_their_level_rather_than_fading_out() {
        // The kernel is clipped at the boundaries; without renormalising, the first
        // and last samples would fade in and out audibly.
        let input = vec![0.5f32; 48_000];
        let output = resample(&input, 48_000, 16_000);
        assert!((output[0] - 0.5).abs() < 0.05, "first sample faded: {}", output[0]);
        let last = *output.last().unwrap();
        assert!((last - 0.5).abs() < 0.05, "last sample faded: {last}");
    }

    #[test]
    fn upsampling_also_works() {
        // Not the common case, but a device running at 8 kHz would need it, and a
        // resampler that only handled one direction would fail silently.
        let input = tone(1_000.0, 8_000, 8_000);
        let output = resample(&input, 8_000, 16_000);
        assert_eq!(output.len(), 16_000);
        assert!(amplitude(&output) > 0.9);
    }

    #[test]
    fn degenerate_input_returns_nothing_rather_than_panicking() {
        assert!(resample(&[], 48_000, 16_000).is_empty());
        assert!(resample(&[0.1, 0.2], 0, 16_000).is_empty());
        assert!(resample(&[0.1, 0.2], 48_000, 0).is_empty());
    }

    #[test]
    fn stereo_is_averaged_not_half_dropped() {
        let sink: Arc<Mutex<Vec<f32>>> = Arc::default();
        // Left silent, right at full level: averaging gives 0.5, taking one channel
        // would give 0.0 or 1.0.
        push(&sink, &[0.0, 1.0, 0.0, 1.0], 2);
        assert_eq!(*sink.lock().unwrap(), vec![0.5, 0.5]);
    }

    #[test]
    fn mono_is_passed_straight_through() {
        let sink: Arc<Mutex<Vec<f32>>> = Arc::default();
        push(&sink, &[0.1, 0.2, 0.3], 1);
        assert_eq!(*sink.lock().unwrap(), vec![0.1, 0.2, 0.3]);
    }
}
