<table>
   <tr>
      <td align="center">
         <img src="logo.png" width="25%">

An interprocess message bus system built in Rust, which can be used to pass messages between multiple processes, even including kernel objects (HANDLE/MachPort/FD).
      </td>
   </tr>
</table>

[![Crates.io](https://img.shields.io/crates/v/ipmb.svg?label=ipmb)](https://crates.io/crates/ipmb)
[![npm version](https://img.shields.io/npm/v/ipmb-js.svg?label=ipmb-js)](https://www.npmjs.com/package/ipmb-js)

## Goals

- **Easy to use**: `join`, `send`, `recv`, that's everything
- **Bus architecture**: No server or client, messages can be freely transmitted among multiple endpoints
- **Message typing**: An endpoint can send or receive multiple message types simultaneously without all endpoints defining a complete global message structure
- **Practical features**: Object, Memory Region, Selector and so on

## Getting Started

```toml
[dependencies]
ipmb = "0.8"
```

`earth.rs`:
```rust
use ipmb::label;

fn main () {
    // Join your bus 
    let options = ipmb::Options::new("com.solar", label!("earth"), "");
    let (sender, receiver) = ipmb::join::<String, String>(options, None).expect("Join com.solar failed");

    // Receive messages
    while let Ok(message) = receiver.recv(None) {
        log::info!("received: {}", message.payload);
    }
}
```

`moon.rs`:
```rust
use ipmb::label;
use std::thread;
use std::time::Duration;

fn main () {
    // Join your bus 
    let options = ipmb::Options::new("com.solar", label!("moon"), "");
    let (sender, receiver) = ipmb::join::<String, String>(options, None).expect("Join com.solar failed");

    loop {
        // Create a message
        let selector = ipmb::Selector::unicast("earth");
        let mut message = ipmb::Message::new(selector, "hello world".to_string());

        // Send the message
        sender.send(message).expect("Send message failed");
        
        thread::sleep(Duration::from_secs(1));
    }
}
```
## Concepts

### Identifier

An identifier is a system-level unique name for a bus, and only endpoints on the same bus can communicate with each other. 
On macOS, it will be used to register the MachPort service, and on Windows, it will be used to create the corresponding named pipe.

### Label

Label is the description of an endpoint, and a message can be routed to an endpoint with a `LabelOp`.
A label can contain multiple elements, such as `label!("renderer", "codec")`.

### Selector

Selector is used to describe the routing rules of the message, which consists of 2 parts:

1. **SelectorMode**: Specify how to consume the message when multiple endpoints satisfy routing rules at the same time. 
    - `Unicast`: Only one endpoint can consume this message
    - `Multicast`: All endpoints can consume this message
2. **LabelOp**: Describe the matching rules of label, and supports logical operations of AND/OR/NOT.

### Payload

Payload is the body content of a message, and its type can be specified by the type parameter of the join function.
You can define your own message types:

```rust
use serde::{Deserialize, Serialize};
use type_uuid::TypeUuid;

#[derive(Debug, Serialize, Deserialize, TypeUuid)]
#[uuid = "7b07473e-9659-4d47-a502-8245d71c0078"]
struct MyMessage {
    foo: i32,
    bar: bool,
}

fn main() {
    let (sender, receiver) = ipmb::join::<MyMessage, MyMessage>(..).unwrap();
}
```

### MessageBox

MessageBox is a container for multiple message types, allowing endpoints to send/receive multiple message types.

```rust
use ipmb::MessageBox;

#[derive(MessageBox)]
enum MultipleMessage {
   String(String),
   I32(i32),
   MyMessage(MyMessage),
}

fn main() {
    let (sender, receiver) = ipmb::join::<MultipleMessage, MultipleMessage>(..).unwrap();
}
```

### Object

Object is the kernel object representation, MachPort on macOS, HANDLE on Windows, ipmb supports sending Object as message attachment to other endpoints.

```rust
fn main () {
   let mut message = ipmb::Message::new(..);
   let obj = unsafe { ipmb::Object::from_raw(libc::mach_task_self()) };
   message.objects.push(obj);
}
```

### MemoryRegion

MemoryRegion is a shared memory block, ipmb supports sending MemoryRegion as message attachment to other endpoints without copying.

```rust
fn main() {
   let mut message = ipmb::Message::new(..);
   let mut region = ipmb::MemoryRegion::new(16 << 10);
   let data = region.map(..).expect("Mapping failed");
   data[0] = 0x10;
   message.memory_regions.push(region);
}
```

### MemoryRegistry

Efficiently performs many MemoryRegions allocation by sharing and reusing MemoryRegions.

```rust
fn main() {
   let mut registry = ipmb::MemoryRegistry::default();
   // Alloc memory region from the registry
   let mut region = registry.alloc(8 << 20, None);
}
```

## Language Bindings

1. **C/C++**: `ipmb-ffi` provides `ipmb_ffi.h`/`ipmb.h`
2. **Node.js**: `ipmb-js` provides node package

## Supported Platforms

| Platform |     |
|----------|-----|
| macOS    | ✅  |
| Windows  | ✅  |
| Linux    | ✅  |

## Benchmark 

Intel(R) Core(TM) i7-9750H CPU @ 2.60GHz, macOS 13.4

```
[2023-06-29T08:54:48Z INFO  bench]       16 B    752,469/s    12.0 MB/s
[2023-06-29T08:54:48Z INFO  bench]       64 B    437,096/s    28.0 MB/s
[2023-06-29T08:54:48Z INFO  bench]     1.0 KB    412,224/s   422.1 MB/s
[2023-06-29T08:54:48Z INFO  bench]     4.1 KB    327,748/s     1.3 GB/s
[2023-06-29T08:54:49Z INFO  bench]    16.4 KB     33,261/s   544.9 MB/s
```

## License

ipmb is dual-licensed:

- [MIT license](LICENSE.MIT)
- [Apache License, Version 2.0](LICENSE.APACHE)
