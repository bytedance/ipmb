use ipmb::{label, MessageBox};
use serde::{Deserialize, Serialize};
use std::thread;
use std::time::Duration;
use type_uuid::TypeUuid;

fn main() {
    let (sender, _) = ipmb::join::<MultipleMessage, MultipleMessage>(
        ipmb::Options::new("com.solar", label!("single-type"), ""),
        None,
    )
    .unwrap();

    loop {
        sender
            .send(ipmb::Message::new(
                ipmb::Selector::unicast("a"),
                MultipleMessage::MyMessage(MyMessage {
                    foo: 10,
                    bar: false,
                }),
            ))
            .unwrap();

        thread::sleep(Duration::from_secs(1));
    }
}

#[derive(Debug, Serialize, Deserialize, TypeUuid)]
#[uuid = "7b07473e-9659-4d47-a502-8245d71c0078"]
struct MyMessage {
    foo: i32,
    bar: bool,
}

#[derive(MessageBox)]
enum MultipleMessage {
    String(String),
    I32(i32),
    MyMessage(MyMessage),
}
