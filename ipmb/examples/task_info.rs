use ipmb::label;
use std::{env, process::Command, time::Duration};

fn main() {
    let mut args = env::args();
    let command = args.next().unwrap();
    let is_main = args.next().is_none();

    let current = if is_main { "sun" } else { "moon" };

    let (tx, mut rx) =
        ipmb::join::<(), ()>(ipmb::Options::new("com.solar", label!(current), ""), None).unwrap();

    if is_main {
        let mut child = Command::new(command).arg("moon").spawn().unwrap();

        let mut selector = ipmb::Selector::unicast("moon");
        selector.ttl = Duration::from_secs(5);
        let mut message = ipmb::Message::new(selector, ());

        #[cfg(target_os = "macos")]
        unsafe {
            message
                .objects
                .push(ipmb::Object::from_raw(libc::mach_task_self()));
        }

        tx.send(message).unwrap();

        child.wait().unwrap();
    } else {
        let message = rx.recv(None).unwrap();

        #[cfg(target_os = "macos")]
        unsafe {
            use std::mem;

            let mut data: libc::mach_task_basic_info_data_t = mem::zeroed();
            let mut count = libc::MACH_TASK_BASIC_INFO_COUNT;
            let r = libc::task_info(
                message.objects[0].as_raw(),
                libc::MACH_TASK_BASIC_INFO,
                &mut data as *mut libc::mach_task_basic_info_data_t as *mut _,
                &mut count,
            );
            assert_eq!(r, libc::KERN_SUCCESS);

            let virtual_size = data.virtual_size;
            println!(
                "task({}): virtual_size({})",
                message.objects[0].as_raw(),
                virtual_size
            );
        }
    }
}
