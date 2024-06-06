use serde::{Deserialize, Serialize};
use std::{
    env,
    process::Command,
    thread,
    time::{Duration, SystemTime},
};
use type_uuid::TypeUuid;

fn main() {
    let mut args = env::args();
    let command = args.next().unwrap();
    let arg = args.next();

    if arg.is_none() {
        let options = ipmb::Options::new("com.ipmb.latency", ipmb::label!("receiver"), "");
        let (_, mut receiver) = ipmb::join::<MyMessage, MyMessage>(options, None).unwrap();

        let mut child = Command::new(command).arg("arg").spawn().unwrap();

        while let Ok(message) = receiver.recv(None) {
            let d = SystemTime::now()
                .duration_since(message.payload.create)
                .unwrap();
            println!("{}ms", d.as_micros() as f64 / 1000.);
        }

        child.kill().unwrap();
    } else {
        let options = ipmb::Options::new("com.ipmb.latency", ipmb::label!("sender"), "");
        let (sender, _) = ipmb::join::<MyMessage, MyMessage>(options, None).unwrap();

        loop {
            let message = ipmb::Message::new(
                ipmb::Selector::unicast("receiver"),
                MyMessage {
                    create: SystemTime::now(),
                },
            );
            if sender.send(message).is_err() {
                break;
            }
            thread::sleep(Duration::from_secs(1));
        }
    }
}

#[derive(Debug, Serialize, Deserialize, TypeUuid)]
#[uuid = "d4adfc76-f5f4-40b0-8e28-8a51a12f5e46"]
pub struct MyMessage {
    create: SystemTime,
}
