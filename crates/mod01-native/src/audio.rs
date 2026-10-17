use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use cpal::{
    SampleFormat, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

const SOURCE_RATE: f64 = 48_000.0;
const MAX_QUEUED_FRAMES: usize = 48_000;

#[derive(Default)]
struct Queue(VecDeque<[f32; 2]>);

pub struct AudioOutput {
    queue: Arc<Mutex<Queue>>,
    stream: Stream,
    output_rate: u32,
    started: AtomicBool,
}

impl AudioOutput {
    pub fn new() -> Result<Self, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("no audio output device is available")?;
        let supported = device
            .default_output_config()
            .map_err(|error| error.to_string())?;
        let config: StreamConfig = supported.config();
        let output_rate = config.sample_rate;
        let channels = usize::from(config.channels);
        let queue = Arc::new(Mutex::new(Queue::default()));
        let errors = |error| eprintln!("MOD-01 audio output: {error}");
        let stream = match supported.sample_format() {
            SampleFormat::F32 => {
                let queue = Arc::clone(&queue);
                device.build_output_stream(
                    config,
                    move |samples: &mut [f32], _| {
                        drain(&queue, samples, channels, |sample| sample);
                    },
                    errors,
                    None,
                )
            }
            SampleFormat::I16 => {
                let queue = Arc::clone(&queue);
                device.build_output_stream(
                    config,
                    move |samples: &mut [i16], _| {
                        drain(&queue, samples, channels, |sample| {
                            (sample * 32767.0) as i16
                        });
                    },
                    errors,
                    None,
                )
            }
            SampleFormat::U16 => {
                let queue = Arc::clone(&queue);
                device.build_output_stream(
                    config,
                    move |samples: &mut [u16], _| {
                        drain(&queue, samples, channels, |sample| {
                            ((sample + 1.0) * 32767.5) as u16
                        });
                    },
                    errors,
                    None,
                )
            }
            other => return Err(format!("unsupported audio sample format: {other:?}")),
        }
        .map_err(|error| error.to_string())?;
        Ok(Self {
            queue,
            stream,
            output_rate,
            started: AtomicBool::new(false),
        })
    }

    pub fn push(&self, left: &str, right: &str) -> Result<(), String> {
        let left = decode_channel(left)?;
        let right = decode_channel(right)?;
        if left.len() != right.len() {
            return Err("audio channels have different lengths".to_owned());
        }
        let frames = resample(&left, &right, self.output_rate);
        let mut queue = self.queue.lock().map_err(|error| error.to_string())?;
        if !self.started.load(Ordering::Relaxed) {
            queue.0.extend(std::iter::repeat_n(
                [0.0, 0.0],
                (self.output_rate / 30) as usize,
            ));
        }
        queue.0.extend(frames);
        while queue.0.len() > MAX_QUEUED_FRAMES {
            queue.0.pop_front();
        }
        drop(queue);
        if !self.started.swap(true, Ordering::Relaxed) {
            if let Err(error) = self.stream.play() {
                self.started.store(false, Ordering::Relaxed);
                return Err(error.to_string());
            }
        }
        Ok(())
    }

    pub fn clear(&self) {
        if self.started.swap(false, Ordering::Relaxed) {
            let _ = self.stream.pause();
        }
        if let Ok(mut queue) = self.queue.lock() {
            queue.0.clear();
        }
    }
}

fn decode_channel(encoded: &str) -> Result<Vec<f32>, String> {
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|error| error.to_string())?;
    if bytes.len() % 4 != 0 || bytes.len() > 48_000 * 4 {
        return Err("invalid audio channel length".to_owned());
    }
    bytes
        .chunks_exact(4)
        .map(|sample| {
            let value = f32::from_le_bytes(sample.try_into().map_err(|_| "invalid audio sample")?);
            if value.is_finite() {
                Ok(value.clamp(-1.0, 1.0))
            } else {
                Err("non-finite audio sample".to_owned())
            }
        })
        .collect()
}

fn resample(left: &[f32], right: &[f32], output_rate: u32) -> Vec<[f32; 2]> {
    if left.is_empty() || output_rate == 0 {
        return Vec::new();
    }
    let count =
        ((left.len() as f64 * f64::from(output_rate) / SOURCE_RATE).round() as usize).max(1);
    (0..count)
        .map(|index| {
            let source = index as f64 * SOURCE_RATE / f64::from(output_rate);
            let first = (source.floor() as usize).min(left.len() - 1);
            let second = (first + 1).min(left.len() - 1);
            let fraction = (source - first as f64) as f32;
            [
                left[first] + (left[second] - left[first]) * fraction,
                right[first] + (right[second] - right[first]) * fraction,
            ]
        })
        .collect()
}

fn drain<T: Copy>(
    queue: &Arc<Mutex<Queue>>,
    output: &mut [T],
    channels: usize,
    convert: impl Fn(f32) -> T,
) {
    if channels == 0 {
        return;
    }
    let Ok(mut queue) = queue.lock() else {
        return;
    };
    for frame in output.chunks_mut(channels) {
        let stereo = queue.0.pop_front().unwrap_or([0.0, 0.0]);
        for (index, slot) in frame.iter_mut().enumerate() {
            *slot = convert(if channels == 1 {
                (stereo[0] + stereo[1]) * 0.5
            } else {
                stereo[index.min(1)]
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::resample;

    #[test]
    fn resamples_to_device_rate() {
        let output = resample(&[0.0, 1.0, 0.0], &[1.0, 0.0, 1.0], 96_000);
        assert_eq!(output.len(), 6);
        assert_eq!(output[1], [0.5, 0.5]);
    }
}
