# Change Log

## Unreleased

- Implement `Not` for `LabelOp`.

## ipmb-js@v0.7.6

### Features
- Add `Object`, `value` method to get `MachPort`/`HANDLE`.

### Fixes

- Implement `Drop` for `ThreadsafeFunction`.

## v0.7.3

### Changes

- `ipmb.h`: Add enum value to Error.
- `ipmb-js`: Refactor thread model.

### Features

- Add `version_pre`.
- `ipmb.h`: Add `Version` api.

```c++
ipmb::Version version;
std::cout << +version.major << "." << +version.minor << "." << +version.patch << "-" << version.pre << "\n";
```

- `ipmb-js`: Add `LabelOp.toString` method.

## v0.7.1

### Features

- Add `MemoryRegistry::maintain`.
- `ipmb-js`: Try recv before spawn promise.

## v0.7.0

### Changes

- Refactor and stabilize version protocol.
- Update `bincode` to `2.0.0-rc.3`.
- macOS: Use root bootstrap port for ipc between daemon and user processes.

### Features

- Add `MemoryRegion::ref_count` for getting cross-process reference count.
- `ipmb.h`: Implement copy constructor for `MemoryRegion`.
- Add `MemoryRegistry::allc_with_free`, free function will be called when region became free.

```rust
fn main() {
    let mut registry  = ipmb::MemoryRegistry::default();
    {
        let _region = registry.alloc_with_free(0, || {
            println!("freed.");
        });
    }
}
```

- `ipmb.h`: Add `Message::object_retrieve`.
- Add `tag` to `MemoryRegistry::alloc/alloc_with_free`.

```rust
fn main() {
    let mut registry  = ipmb::MemoryRegistry::default();
    registry.alloc(10 << 10, Some("photo"));
}
```

## v0.6.4

### Changes

- `ipmb.h`: Compatible with c++14.

## v0.6.3

### Features

- `ipmb.h`: Add default constructor for `MemoryRegion`.

### Fixes

- Full memory barrier for shared ref counting.
 
```diff
- rc.fetch_add(val as _, Ordering::Relaxed)
+ rc.fetch_add(val as _, Ordering::SeqCst)
```
