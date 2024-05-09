#include <chrono>
#include <iostream>
#include <thread>
#include "ipmb.h"

int main() {
    ipmb::Version version;
    std::cout << +version.major << "." << +version.minor << "." << +version.patch;
    if (!version.pre.empty()) {
        std::cout << "-" << version.pre;
    }
    std::cout << "\n";

    ipmb::Options options(
            "com.solar",
            ipmb::Label {"cc"},
            "",
            true
    );

    ipmb::Sender sender;
    ipmb::Receiver receiver;
    ipmb::Error err;
    std::tie(sender, receiver, err) = ipmb::join(options, ipmb_ffi::TIMEOUT_INFINITE);
    if (err != ipmb::Error::kSuccess) {
        return -1;
    }

    std::cout << "Join succeed.\n";

    std::thread t([sender = std::move(sender)]() mutable {
        ipmb::MemoryRegistry registry;

        ipmb::Selector selector(
                ipmb::LabelOp("a"),
                ipmb_ffi::SelectorMode::kUnicast,
                0
        );

        const uint8_t buffer[5] {0, 1, 2, 3, 4};

        ipmb::Error err;

        while (true) {
            std::this_thread::sleep_for(std::chrono::seconds(2));

            ipmb::Message message(selector, 2, buffer, sizeof(buffer));

            ipmb::MemoryRegion region;
            std::tie(region, err) = registry.alloc(64, nullptr);
            if (err != ipmb::Error::kSuccess) {
                break;
            }

            message.memory_region_append(std::move(region));

            err = sender.send(std::move(message));
            if (err != ipmb::Error::kSuccess) {
                break;
            }
        }
    });

    for (;;) {
        ipmb::Message message;
        std::tie(message, err) = receiver.recv(ipmb_ffi::TIMEOUT_INFINITE);
        if (err != ipmb::Error::kSuccess) {
            break;
        }

        uint16_t format;
        const uint8_t* ptr;
        uint32_t size;
        std::tie(format, ptr, size, err) = message.bytes_data();
        if (err != ipmb::Error::kSuccess) {
            break;
        }

        ipmb::MemoryRegion region;
        std::tie(region, err) = message.memory_region_retrieve(0);
        if (err != ipmb::Error::kSuccess) {
            break;
        }

        uint8_t* region_ptr;
        intptr_t region_size;
        std::tie(region_ptr, region_size, err) = region.map(0, -1);
        if (err != ipmb::Error::kSuccess) {
            break;
        }

        std::cout << "format: " << format << ", ptr: " << (void*) ptr << ", size: " << size << ", region: "
                  << (void*) region_ptr << "," << region_size << "\n";
    }

    t.join();

    return 0;
}