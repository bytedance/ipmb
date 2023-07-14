use ipmb::label;
use std::process::Command;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;
use std::{env, thread};

fn main() {
    let mut args = env::args();
    let command = args.next().unwrap();

    match args.next() {
        None => {
            let (tx, mut rx) = ipmb::join::<ipmb::BytesMessage, ipmb::BytesMessage>(
                ipmb::Options::new("reliability", label!("0"), ""),
                None,
            )
            .unwrap();

            let mut wait_list = vec![];
            let mut kill_list = vec![];

            for i in 1..4 {
                let child = Command::new(command.clone())
                    .arg(i.to_string())
                    .spawn()
                    .unwrap();

                match i {
                    2 | 3 => {
                        wait_list.push(child);
                    }
                    _ => {
                        kill_list.push(child);
                    }
                }
            }

            thread::spawn(move || while rx.recv(None).is_ok() {});

            for _ in 0..10_000 {
                tx.send(ipmb::Message::new(
                    ipmb::Selector::multicast(ipmb::LabelOp::True),
                    ipmb::BytesMessage {
                        format: 0,
                        data: vec![0x00, 0x01, 0x02, 0x03],
                    },
                ))
                .unwrap();
            }

            let (guard_tx, guard_rx) = mpsc::channel();

            thread::spawn(move || {
                for mut child in wait_list {
                    child.wait().unwrap();
                }
                guard_tx.send(()).unwrap();
            });

            match guard_rx.recv_timeout(Duration::from_secs(5)) {
                Err(RecvTimeoutError::Timeout) => {
                    for mut child in kill_list {
                        child.kill().unwrap();
                    }
                    panic!("Timeout");
                }
                _ => {
                    for mut child in kill_list {
                        child.kill().unwrap();
                    }
                }
            }
        }
        Some(i) => {
            let (tx, mut rx) = ipmb::join::<ipmb::BytesMessage, ipmb::BytesMessage>(
                ipmb::Options::new("reliability", label!(i.to_string()), ""),
                None,
            )
            .unwrap();

            match i.as_str() {
                "1" => {
                    drop(tx);
                    while rx.recv(None).is_ok() {}
                }
                "2" => {
                    drop(rx);
                    for _ in 0..10_000 {
                        tx.send(ipmb::Message::new(
                            ipmb::Selector::multicast(ipmb::LabelOp::True),
                            ipmb::BytesMessage {
                                format: 0,
                                data: vec![0x00, 0x01, 0x02, 0x03],
                            },
                        ))
                        .unwrap();
                    }
                }
                _ => {
                    drop(tx);
                    drop(rx);
                }
            }
        }
    }
}
