use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rustfft::{num_complex::Complex, FftPlanner};
use std::error::Error;
use std::ops::Range;

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
    fn scale(&self, other: f32) -> Self {
        ((self.start as f32 * other).trunc() as usize)..((self.end as f32 * other).trunc() as usize)
    }
}

enum Binning {
    Linear(usize),
    Logarithmic(usize),
    Custom(Vec<Range<usize>>),
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

            Binning::Custom(ranges) => ranges
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

fn main() -> Result<(), Box<dyn Error>> {
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

    // let binning = Binning::Linear(12);
    let binning = Binning::Logarithmic(2);
    // let binning = Binning::Custom(vec![
    //     0..50,
    //     50..100,
    //     100..1000,
    //     1000..2000,
    //     2000..6000,
    //     6000..10000,
    //     10000..14000,
    //     14000..20000,
    //     20000..24000,
    // ]);

    let stream = device.build_input_stream(
        &config.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let mono: Vec<f32> = data
                .chunks(channels)
                .map(|sample| average(sample))
                .collect();
            let num_samples = mono.len();
            let fft = calculate_fft(&mono, num_samples);
            let bins = binning.bin(&fft, sample_rate);

            clearscreen::clear().expect("failed to clear screen");
            for (range, magnitude) in bins.iter() {
                println!("{:?}: {:.2}", range, magnitude);
            }
        },
        |err| eprintln!("Error: {err}"),
        None,
    )?;

    stream.play()?;

    std::thread::sleep(std::time::Duration::from_secs(600));
    Ok(())
}

// #[tokio::main]
// use openrgb::{data::Color, OpenRGB};
// let client = OpenRGB::connect_to(("blade", 6742)).await?;
// client.update_led(0, 45, Color::new(255, 127, 0)).await?;
