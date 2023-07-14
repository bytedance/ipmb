fn main() {
    let mut registry = ipmb::MemoryRegistry::default();

    let region = registry.alloc_with_free(0, None, || {
        println!("free");
    });

    drop(region);

    let _region = registry.alloc(0, None);
}
