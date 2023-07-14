use ipmb::{label, MessageBox};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};
use std::{env, thread};
use type_uuid::TypeUuid;

fn main() {
    let mut builder = env_logger::Builder::new();
    builder.filter_level(log::LevelFilter::Info);
    builder.parse_default_env();
    builder.init();

    let label = env::args().skip(1).next().unwrap();

    let target = match label.as_str() {
        "b" => ipmb::LabelOp::from("a").or("c"),
        "c" => ipmb::LabelOp::from("a").or("b"),
        _ => ipmb::LabelOp::from("b").or("c"),
    };

    let mut options = ipmb::Options::new("solar.com", label!(label), "");
    options.controller_affinity = true;

    let (sender, mut receiver) =
        ipmb::join::<MyMessageBox, MyMessageBox>(options, None).expect("Join solar.com failed");

    thread::spawn(move || loop {
        let mut msg = receiver.recv(None).expect("Receive message failed");

        match msg.payload {
            MyMessageBox::MyMessage(payload) => {
                log::info!("payload: {}", payload.val);

                let mut region = msg.memory_regions.remove(0);
                log::info!("{:x?}", region.map(..).unwrap());
            }
            MyMessageBox::CMessage(_) => {}
            MyMessageBox::BytesMessage(bytes) => {
                log::info!("{bytes:?} region: {}", msg.memory_regions.len());
            }
        }
    });

    let mut registry = ipmb::MemoryRegistry::default();

    loop {
        let mut msg = ipmb::Message::new(
            ipmb::Selector::multicast(target.clone()),
            MyMessageBox::MyMessage(MyMessage {
                create: SystemTime::now(),
                val: format!("from: {}", label),
            }),
        );

        let mut region = registry.alloc(16, None);
        region.map(..).unwrap()[0] = 0x2e;

        msg.memory_regions.push(region);

        if let Err(err) = sender.send(msg) {
            log::error!("{:?}", err);
        }

        let mut msg = ipmb::Message::new(
            ipmb::Selector::multicast(ipmb::LabelOp::from("cc")),
            MyMessageBox::BytesMessage(ipmb::BytesMessage {
                format: 0,
                data: vec![0x01, 0x03, 0x05, 0x07],
            }),
        );

        msg.memory_regions.push(registry.alloc(32, None));

        if let Err(err) = sender.send(msg) {
            log::error!("{:?}", err);
        }

        thread::sleep(Duration::from_secs(2));
    }
}

#[derive(Debug, Serialize, Deserialize, TypeUuid)]
#[uuid = "d4adfc76-f5f4-40b0-8e28-8a51a12f5e46"]
pub struct MyMessage {
    create: SystemTime,
    val: String,
}

#[derive(Debug, Serialize, Deserialize, TypeUuid)]
#[uuid = "d4adfc76-f5f4-40b0-8e28-8a51a12f5e47"]
#[repr(C)]
pub struct CMessage {
    val: i32,
}

#[derive(Debug, MessageBox)]
pub enum MyMessageBox {
    MyMessage(MyMessage),
    CMessage(CMessage),
    BytesMessage(ipmb::BytesMessage),
}
