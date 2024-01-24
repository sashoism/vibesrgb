use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rustfft::{num_complex::Complex, FftPlanner};
use std::error::Error;

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

    let stream = device.build_input_stream(
        &config.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let mono: Vec<f32> = data
                .chunks(channels)
                .map(|sample| average(sample))
                .collect();
            let num_samples = mono.len();
            let fft = calculate_fft(&mono, num_samples);

            let chunks: Vec<f32> = fft
                .chunks(num_samples * 1000 / sample_rate as usize)
                .map(|chunk| chunk.iter().sum::<Complex<f32>>().norm())
                .collect();

            clearscreen::clear().expect("failed to clear screen");
            for (i, magnitude) in chunks.iter().enumerate() {
                println!("{}: {:.2}", i, magnitude);
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
