use cpal::{
    platform::Host, traits::{DeviceTrait, HostTrait, StreamTrait}
};

use ringbuf::HeapRb;

pub struct Loopback<'a> {
    pub host: Host,
    pub input_device: &'a str,
    pub output_device: &'a str,
    pub latency: f32,
}

impl Loopback<'_> {
    pub fn new() -> Self {
        Loopback {
            host: cpal::default_host(),
            input_device: "default",
            output_device: "default",
            latency: 150.0,
        }
    }

    pub fn start(&mut self) -> anyhow::Result<()> {
            let host = &self.host;
        
            // Find devices.
            let input_device = if &self.input_device == &"default" {
                host.default_input_device()
            } else {
                host.input_devices()?
                    .find(|x| x.name().map(|y| &stringify!(y) == &self.input_device).unwrap_or(false))
            }
            .expect("Failed to find input device!");
        
            let output_device = if &self.output_device == &"default" {
                host.default_output_device()
            } else {
                host.output_devices()?
                    .find(|x| x.name().map(|y| &stringify!(y) == &self.output_device).unwrap_or(false))
            }
            .expect("Failed to find output device!");
        
            println!("Using input device: \"{}\"", input_device.name()?);
            println!("Using output device: \"{}\"", output_device.name()?);
        
            // We'll try and use the same configuration between streams to keep it simple.
            let config: cpal::StreamConfig = input_device.default_input_config()?.into();
        
            // Create a delay in case the input and output devices aren't synced.
            let latency_frames = (&self.latency / 1_000.0) * config.sample_rate.0 as f32;
            let latency_samples = latency_frames as usize * config.channels as usize;
        
            // The buffer to share samples
            let ring = HeapRb::<f32>::new(latency_samples * 2);
            let (mut producer, mut consumer) = ring.split();
        
            // Fill the samples with 0.0 equal to the length of the delay.
            for _ in 0..latency_samples {
                // The ring buffer has twice as much space as necessary to add latency here,
                // so this should never fail
                producer.push(0.0).unwrap();
            }
        
            let input_data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let mut output_fell_behind = false;
                for &sample in data {
                    if producer.push(sample).is_err() {
                        output_fell_behind = true;
                    }
                }
                if output_fell_behind {
                    eprintln!("Output stream fell behind! Try increasing latency.");
                }
            };
        
            let output_data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut input_fell_behind = false;
                for sample in data {
                    *sample = match consumer.pop() {
                        Some(s) => s,
                        None => {
                            input_fell_behind = true;
                            0.0
                        }
                    };
                }
                if input_fell_behind {
                    eprintln!("Input stream fell behind! Try increasing latency.");
                }
            };
        
            // Build streams.
            println!(
                "Attempting to build both streams with f32 samples and `{:?}`.",
                config
            );
            let input_stream = input_device.build_input_stream(&config, input_data_fn, err_fn, None)?;
            let output_stream = output_device.build_output_stream(&config, output_data_fn, err_fn, None)?;
            println!("Successfully built streams.");
        
            // Play the streams.
            println!(
                "Starting the input and output streams with `{}` milliseconds of latency.",
                &self.latency
            );
            input_stream.play()?;
            output_stream.play()?;
        
            // Run for 3 seconds before closing.
            println!("Playing for 3 seconds... ");
            std::thread::sleep(std::time::Duration::from_secs(3));
            drop(input_stream);
            drop(output_stream);
            println!("Done!");
            Ok(())
    }
}


fn err_fn(err: cpal::StreamError) {
    eprintln!("An error occurred on stream: {}", err);
}