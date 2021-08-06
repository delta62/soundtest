#[macro_use]
mod macros;
mod alsa;

use alsa::{Device, DeviceConfig};
use dasp::signal::{self as signal, Signal};
use dasp::sample::conv;

const SAMPLE_RATE: u32 = 44_100;

fn main() {
    let config = DeviceConfig {
        sample_rate: SAMPLE_RATE,
        channels: 1,
        buffer_target_us: 8_000,
        period_target_us: 4_000,
    };

    let mut signal = signal::rate(SAMPLE_RATE as f64).const_hz(261.63).sine();

    let device = Device::with_config(config).unwrap();
    println!("{:?}", device);

    device.run(move |queue, wanted| {
        for _ in 0..wanted {
            queue.push_back(conv::f64::to_f32(signal.next()));
        }
    });
}
