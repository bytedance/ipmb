use ipmb::label;
use std::time::Duration;

fn main() {
    let mut builder = env_logger::Builder::new();
    builder.filter_level(log::LevelFilter::Info);
    builder.parse_default_env();
    builder.init();

    let option = ipmb::Options::new("solar.com", label!("moon"), "");

    {
        let (_sender, _receiver) =
            ipmb::join::<(), ()>(option.clone(), None).expect("Join solar.com failed");
    }

    let (_sender, mut receiver) =
        ipmb::join::<(), ()>(option, None).expect("Join solar.com failed");

    loop {
        match receiver.recv(Some(Duration::from_millis(500))) {
            Err(ipmb::RecvError::Timeout) => {
                log::info!("Timeout");
                continue;
            }
            _ => break,
        }
    }
}
