use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use rustfft::{FftPlanner, num_complex::Complex};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use crossterm::{
    cursor,
    style::{self, Color},
    terminal::{self, ClearType},
    ExecutableCommand,
    QueueableCommand,
};
use std::io::{self, Write};

#[derive(Clone)]
struct AudioData {
    amplitude: f32,
    frequency: f32,
    wavelength: f32,
}

impl AudioData {
    fn new() -> Self {
        Self {
            amplitude: 0.0,
            frequency: 0.0,
            wavelength: 0.0,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize audio host
    let host = cpal::default_host();
    
    // List available input devices
    let input_devices: Vec<Device> = host.input_devices()?.collect();
    
    if input_devices.is_empty() {
        eprintln!("No input devices found!");
        return Ok(());
    }

    println!("\nAvailable input devices:");
    for (i, device) in input_devices.iter().enumerate() {
        println!("{}. {}", i, device.name()?);
    }

    // Get user selection
    print!("\nSelect a device (0-{}): ", input_devices.len() - 1);
    io::stdout().flush()?;
    
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let device_index: usize = input.trim().parse()?;
    
    if device_index >= input_devices.len() {
        eprintln!("Invalid device selection!");
        return Ok(());
    }

    let device = &input_devices[device_index];
    println!("Selected device: {}", device.name()?);

    // Get supported input config
    let config = device.default_input_config()?;
    println!("Sample rate: {} Hz", config.sample_rate().0);
    println!("Channels: {}", config.channels());
    println!("Sample format: {:?}", config.sample_format());

    // Create shared audio data
    let audio_data = Arc::new(Mutex::new(AudioData::new()));
    let audio_data_clone = audio_data.clone();

    // Create audio stream
    let stream = match config.sample_format() {
        SampleFormat::F32 => create_stream::<f32>(device, &config.into(), audio_data_clone)?,
        SampleFormat::I16 => create_stream::<i16>(device, &config.into(), audio_data_clone)?,
        SampleFormat::U16 => create_stream::<u16>(device, &config.into(), audio_data_clone)?,
        _ => return Err("Unsupported sample format".into()),
    };

    // Start the stream
    stream.play()?;
    println!("\n🎤 Recording started! Press Ctrl+C to stop.\n");

    // Setup terminal for real-time display
    let mut stdout = io::stdout();
    stdout.execute(terminal::Clear(ClearType::All))?;
    stdout.execute(cursor::MoveTo(0, 0))?;

    let mut last_update = Instant::now();
    let update_interval = Duration::from_millis(100); // Update every 100ms

    // Main display loop
    loop {
        if last_update.elapsed() >= update_interval {
            let data = audio_data.lock().unwrap();
            
            // Clear screen and move cursor to top
            stdout.execute(terminal::Clear(ClearType::All))?;
            stdout.execute(cursor::MoveTo(0, 0))?;
            
            // Display header
            stdout.queue(style::SetForegroundColor(Color::Cyan))?;

            // Display audio data
            stdout.queue(style::SetForegroundColor(Color::Green))?;
            println!("📊 Audio Analysis:");
            println!("  Amplitude: {:.3}", data.amplitude);
            println!("  Frequency: {:.1} Hz", data.frequency);
            println!("  Wavelength: {:.2} cm", data.wavelength);
            
            stdout.queue(style::SetForegroundColor(Color::Magenta))?;
            println!("\n🎼 Frequency Range:");
            if data.frequency < 80.0 {
                println!("  Bass (0-80 Hz)");
            } else if data.frequency < 250.0 {
                println!("  Low Mid (80-250 Hz)");
            } else if data.frequency < 2000.0 {
                println!("  Mid (250-2000 Hz)");
            } else if data.frequency < 4000.0 {
                println!("  High Mid (2000-4000 Hz)");
            } else {
                println!("  Treble (4000+ Hz)");
            }
            
            // Show note information
            stdout.queue(style::SetForegroundColor(Color::Blue))?;
            println!("\n🎵 Note Information:");
            let note = frequency_to_note(data.frequency);
            println!("  Note: 🤷");
            
            stdout.queue(style::SetForegroundColor(Color::Red))?;
            println!("\n⚠️  Note Detection Work in Progress - Data may be inaccurate!");
            
            stdout.queue(style::SetForegroundColor(Color::White))?;
            println!("\nPress Ctrl+C to stop recording...");
            
            stdout.flush()?;
            last_update = Instant::now();
        }
        
        thread::sleep(Duration::from_millis(10));
    }
}

fn create_stream<T>(
    device: &Device,
    config: &StreamConfig,
    audio_data: Arc<Mutex<AudioData>>,
) -> Result<Stream, Box<dyn std::error::Error>>
where
    T: cpal::Sample + cpal::SizedSample + cpal::FromSample<f32> + std::marker::Send + 'static,
    f32: cpal::FromSample<T>,
{
    let sample_rate = config.sample_rate.0 as f32;
    let _channels = config.channels as usize;
    
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            // Convert samples to f32
            let samples: Vec<f32> = data.iter().map(|s| s.to_sample()).collect();
            
            // Calculate amplitude (RMS)
            let amplitude = calculate_rms(&samples);
            
            // Calculate frequency using FFT
            let frequency = calculate_frequency(&samples, sample_rate);
            
            // Calculate wavelength (assuming speed of sound = 343 m/s)
            let wavelength = if frequency > 0.0 { 343.0 / frequency * 100.0 } else { 0.0 }; // in cm
            
            // Update shared data
            if let Ok(mut data) = audio_data.lock() {
                data.amplitude = amplitude;
                data.frequency = frequency;
                data.wavelength = wavelength;
            }
        },
        |err| eprintln!("Audio stream error: {}", err),
        None,
    )?;
    
    Ok(stream)
}

fn frequency_to_note(frequency: f32) -> String {
    if frequency <= 0.0 {
        return "Silence".to_string();
    }
    
    // A4 = 440 Hz is our reference
    let a4_freq = 440.0;
    let _semitone_ratio = 2.0_f32.powf(1.0 / 12.0);
    
    // Calculate semitones from A4
    let semitones = (frequency / a4_freq).log2() * 12.0;
    let rounded_semitones = semitones.round() as i32;
    
    // Note names
    let note_names = ["A", "A#", "B", "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#"];
    
    // Calculate octave and note
    let octave = 4 + (rounded_semitones / 12);
    let note_index = ((rounded_semitones % 12) + 12) % 12;
    
    format!("{}{}", note_names[note_index as usize], octave)
}

fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    
    let sum_squares: f32 = samples.iter().map(|&x| x * x).sum();
    (sum_squares / samples.len() as f32).sqrt()
}

fn calculate_frequency(samples: &[f32], sample_rate: f32) -> f32 {
    if samples.len() < 1024 {
        return 0.0;
    }
    
    // Take a window of samples for FFT
    let window_size = 1024;
    let mut complex_samples: Vec<Complex<f32>> = samples[..window_size]
        .iter()
        .map(|&x| Complex::new(x, 0.0))
        .collect();
    
    // Apply window function (Hann window)
    for (i, sample) in complex_samples.iter_mut().enumerate() {
        let window_value = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (window_size - 1) as f32).cos());
        sample.re *= window_value;
    }
    
    // Perform FFT
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(window_size);
    fft.process(&mut complex_samples);
    
    // Find the peak frequency
    let mut max_magnitude = 0.0;
    let mut peak_bin = 0;
    
    for (i, &sample) in complex_samples.iter().enumerate() {
        let magnitude = sample.norm();
        if magnitude > max_magnitude && i > 0 && i < window_size / 2 {
            max_magnitude = magnitude;
            peak_bin = i;
        }
    }
    
    // Convert bin to frequency
    (peak_bin as f32 * sample_rate) / window_size as f32
}
