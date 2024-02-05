use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use openrgb::{data::Color, OpenRGB};
use rustfft::{num_complex::Complex, FftPlanner};
use serde_json::Value;
use std::error::Error;
use std::io::BufReader;
use std::ops::Range;
use std::sync::{Arc, Mutex};

fn average(slice: &[f32]) -> f32 {
    let sum: f32 = slice.iter().sum();
    let count = slice.len();
    if count == 0 {
        0.0
    } else {
        sum / count as f32
    }
}

fn calculate_fft(data: &[f32], len: usize) -> Vec<Complex<f32>> {
    let fft = FftPlanner::new().plan_fft_forward(len);
    let mut buffer: Vec<Complex<f32>> = data
        .iter()
        .map(|&sample| Complex::new(sample, 0.0))
        .collect();
    fft.process(&mut buffer);
    buffer.truncate(len / 2);
    buffer
}

trait Scalable<T> {
    fn scale(&self, scalar: T) -> Self;
}

impl Scalable<f32> for Range<usize> {
    fn scale(&self, scalar: f32) -> Self {
        ((self.start as f32 * scalar).trunc() as usize)
            ..((self.end as f32 * scalar).trunc() as usize)
    }
}

enum Binning {
    Linear(usize),
    Logarithmic(usize),
    Ranges(Vec<Range<usize>>),
}

impl Binning {
    fn bin(&self, fft: &Vec<Complex<f32>>, sample_rate: f32) -> Vec<(Range<usize>, f32)> {
        let len = fft.len();
        let max_freq = sample_rate / 2f32;
        let scale = len as f32 / max_freq;

        match self {
            Binning::Linear(bins) => (0..*bins)
                .map(|bin| (bin..(bin + 1)).scale(max_freq / *bins as f32))
                .map(|range| {
                    let scaled_range = range.scale(scale);
                    let sum = fft[scaled_range].iter().sum::<Complex<f32>>().norm();
                    (range, sum)
                })
                .collect(),

            Binning::Logarithmic(log) => self
                .log_bins(max_freq, *log as f32)
                .iter()
                .map(|range| {
                    let scaled_range = range.scale(scale);
                    let sum = fft[scaled_range].iter().sum::<Complex<f32>>().norm();
                    (range.clone(), sum)
                })
                .collect(),

            Binning::Ranges(ranges) => ranges
                .iter()
                .map(|range| {
                    let scaled_range = range.scale(scale);
                    let sum = fft[scaled_range].iter().sum::<Complex<f32>>().norm();
                    (range.clone(), sum)
                })
                .collect(),
        }
    }

    fn log_bins(&self, max_freq: f32, base: f32) -> Vec<Range<usize>> {
        let mut bins = Vec::new();
        let mut current_freq = 1.0;
        let base_log = base.log(base);

        while current_freq < max_freq {
            let next_freq = base
                .powf((current_freq.log(base) + 1.0) / base_log)
                .round()
                .min(max_freq);
            bins.push((current_freq as usize)..(next_freq as usize));
            current_freq = next_freq;
        }

        bins
    }
}

fn paint(
    leds: &Vec<Option<(f32, f32)>>,
    bins: &Vec<(Range<usize>, f32)>,
    sample_rate: f32,
) -> Vec<Color> {
    leds.iter()
        .map(|led| match led {
            Some((x, y)) => {
                let bin = bins
                    .iter()
                    .find(|(range, _)| range.contains(&((*x * sample_rate / 2.0) as usize)))
                    .unwrap();
                if bin.1 > 1.0 {
                    Color::new(255, 0, 0)
                } else {
                    Color::new(0, 0, 0)
                }
            }
            None => Color::new(0, 0, 0),
        })
        .collect()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let led_config_json: Value =
        serde_json::from_reader(BufReader::new(std::fs::File::open("assets/razerkbd.json")?))
            .unwrap();

    let leds: Vec<Option<_>> = led_config_json["leds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|led| {
            if led.is_null() {
                None
            } else {
                Some((
                    led["x"].as_f64().unwrap() as f32,
                    led["y"].as_f64().unwrap() as f32,
                ))
            }
        })
        .collect();

    let client = OpenRGB::connect_to(("localhost", 6742)).await?;
    let controller_id = 0;

    let device = cpal::default_host()
        .input_devices()?
        .find(|dev| {
            dev.name()
                .expect("Cannot read device name")
                .contains("Stereo Mix")
        })
        .expect("No loopback device found");
    let config = device.default_input_config()?;
    let channels = config.channels().into();
    let sample_rate = config.sample_rate().0 as f32;

    let window_size_millis: f32 = 50.0;
    let window_size_samples = (window_size_millis / 1000.0 * sample_rate) as usize;
    let window = Arc::new(Mutex::new(Vec::<f32>::with_capacity(window_size_samples)));

    let window_cpal = window.clone();
    let stream = device.build_input_stream(
        &config.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let mono: Vec<f32> = data
                .chunks(channels)
                .map(|sample| average(sample))
                .collect();
            let mut window = window_cpal.lock().unwrap();
            window.extend(mono);
        },
        |err| eprintln!("Error: {err}"),
        None,
    )?;

    stream.play()?;

    let window_proc = window.clone();
    tokio::spawn(async move {
        loop {
            if let Some(colors) = {
                let mut window = window_proc.lock().unwrap();
                if window.len() >= window_size_samples {
                    let slice = &window[window.len() - window_size_samples..];
                    let fft = calculate_fft(slice, window_size_samples);
                    let bins = Binning::Linear(10).bin(&fft, sample_rate);
                    window.clear();
                    Some(paint(&leds, &bins, sample_rate))
                } else {
                    None
                }
            } {
                client.update_leds(controller_id, colors).await.unwrap();
            }
            tokio::time::sleep(std::time::Duration::from_millis(window_size_millis as u64)).await;
        }
    });

    std::thread::sleep(std::time::Duration::from_secs(600));
    Ok(())
}
