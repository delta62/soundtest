#[macro_use]
mod macros;
mod alsa;

use alsa::{Device, DeviceConfig};
use dasp::signal::{self as signal, Signal};
use dasp::sample::conv;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

const SAMPLE_RATE: u32 = 44_100;

enum Message {
    WantMoreData,
}

fn main() {
    let mut signal = signal::rate(SAMPLE_RATE as f64).const_hz(1000.00).sine();
    let (tx, rx) = mpsc::channel();

    let mut buffer: VecDeque<f32> = VecDeque::with_capacity(6_000);
    for _ in 0..6_000 {
        let sample = conv::f64::to_f32(signal.next());
        buffer.push_back(sample);
    }

    let buffer = Arc::new(Mutex::new(buffer));
    let t_buffer = buffer.clone();

    thread::spawn(|| {
        let config = DeviceConfig {
            sample_rate: SAMPLE_RATE,
            channels: 1,
            buffer_target_us: 42_000,
            period_target_us: 8_000,
        };

        let device = Device::with_config(config).unwrap();
        println!("{:#?}", device);

        device.run(move |queue, wanted| {
            let mut buffer = t_buffer.lock().unwrap();

            for _ in 0..wanted {
                match buffer.pop_front() {
                    Some(sample) => queue.push_back(sample),
                    None => println!("Not enough data!!"),
                };
            }

            tx.send(Message::WantMoreData).unwrap();
        });
    });

    loop {
        let msg = rx.recv().unwrap();

        match msg {
            Message::WantMoreData => {
                let mut buffer = buffer.lock().unwrap();
                let getting = buffer.capacity() - buffer.len();

                for _ in 0..getting {
                    let sample = conv::f64::to_f32(signal.next());
                    buffer.push_back(sample);
                }
            }
        }
    }
}
