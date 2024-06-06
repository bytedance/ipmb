use ipmb::label;
use num_format::{Locale, ToFormattedString};
use rand::distributions::Alphanumeric;
use rand::Rng;
use std::env;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

fn main() {
    let mut builder = env_logger::Builder::new();
    builder.filter_level(log::LevelFilter::Info);
    builder.parse_default_env();
    builder.init();

    let mut args = env::args();
    let command = args.next().unwrap();
    let label = args.next();

    match label {
        None => {
            let (sender, _) = ipmb::join::<String, String>(
                ipmb::Options::new("bench.com", label!("sender"), ""),
                None,
            )
            .unwrap();

            for (i, (message_size, count)) in [
                (1 << 4, 1_000_000),
                (1 << 6, 250_000),
                (1 << 10, 15_000),
                (1 << 12, 8_000),
                (1 << 14, 1_000),
            ]
            .into_iter()
            .enumerate()
            {
                let mut child = Command::new(command.clone())
                    .arg(format!("receiver-{i}"))
                    .spawn()
                    .unwrap();

                // Wait receiver
                thread::sleep(Duration::from_secs(2));

                let mut selector = ipmb::Selector::unicast(format!("receiver-{i}"));
                selector.ttl = Duration::from_secs(2);

                let msg = ipmb::Message::new(selector.clone(), "0".to_string());
                let _ = sender.send(msg);

                let hello: String = rand::thread_rng()
                    .sample_iter(&Alphanumeric)
                    .take(message_size)
                    .map(char::from)
                    .collect();

                for _ in 0..count {
                    let msg = ipmb::Message::new(selector.clone(), hello.clone());
                    let _ = sender.send(msg);
                }

                let msg = ipmb::Message::new(selector, "1".to_string());
                let _ = sender.send(msg);

                child.wait().unwrap();
            }
        }
        Some(label) => {
            let (_, mut receiver) = ipmb::join::<String, String>(
                ipmb::Options::new("bench.com", label!(label), ""),
                None,
            )
            .unwrap();

            let mut start = None;
            let mut count = 0;
            let mut message_size = 0;

            loop {
                let msg = receiver.recv(None).unwrap();

                if msg.payload == "0" {
                    start = Some(Instant::now());
                }

                if msg.payload == "1" {
                    break;
                }

                count += 1;
                message_size = msg.payload.len();
            }

            let cost = start.expect("Lost start packet").elapsed().as_secs_f32();
            let packet = (count as f32 / cost) as usize;

            log::info!(
                "{:>10} {:>10}/s {:>10}/s",
                bytesize::ByteSize(message_size as _),
                packet.to_formatted_string(&Locale::en),
                bytesize::ByteSize((packet * message_size) as _),
            );
        }
    }
}
