#include <chrono>
#include <iostream>
#include <thread>
#include "ipmb_ffi.h"

int main()
{
    ipmb_ffi::Label label = ipmb_ffi::ipmb_label();
    ipmb_ffi::ipmb_label_insert(&label, "cc");

    ipmb_ffi::Options options;
    options.identifier = "solar.com";
    options.label = &label;
    options.token = "";
    options.controller_affinity = true;

    void* sender = nullptr;
    void* receiver = nullptr;
    auto r = ipmb_ffi::ipmb_join(options, ipmb_ffi::TIMEOUT_INFINITE, &sender, &receiver);
    if (r != 0) {
        std::cout << "Join failed: " << r << "\n";
        return -1;
    }

    std::cout << "Join succeed\n";

    std::thread t([sender]() mutable {
        auto op = ipmb_ffi::ipmb_label_op_leaf("a");

        ipmb_ffi::Selector selector;
        selector.label_op = &op;
        selector.mode = ipmb_ffi::SelectorMode::kUnicast;
        selector.ttl = 0;

        const uint8_t buffer[5] { 0, 1, 2, 3, 4 };

        while (true) {
            std::this_thread::sleep_for(std::chrono::seconds(2));

            auto message = ipmb_ffi::ipmb_message(selector, 2, buffer, sizeof(buffer));

            auto r = ipmb_ffi::ipmb_send(&sender, message);
            if (r != 0) {
                break;
            }
        }

        ipmb_ffi::ipmb_label_op_drop(op);
        ipmb_ffi::ipmb_sender_drop(sender);
    });

    while (true) {
        ipmb_ffi::Message message;
        auto r = ipmb_ffi::ipmb_recv(&receiver, &message, ipmb_ffi::TIMEOUT_INFINITE);
        if (r != 0) {
            break;
        }

        uint16_t format;
        const uint8_t* ptr;
        uint32_t size;
        ipmb_ffi::ipmb_message_bytes_data(&message, &format, &ptr, &size);

        auto region = ipmb_ffi::ipmb_message_memory_region_retrieve(&message, 0);
        auto region_ptr = ipmb_ffi::ipmb_memory_region_map(&region, 0, 32, nullptr);

        std::cout << "Receive: bytes_message(" << size << " bytes)\n";
        std::cout << "Receive: memory_region(" << (void*)region_ptr << ")\n";

        ipmb_ffi::ipmb_memory_region_drop(region);
        ipmb_ffi::ipmb_message_drop(message);
    }

    ipmb_ffi::ipmb_label_drop(label);
    ipmb_ffi::ipmb_receiver_drop(receiver);

    t.join();

    return 0;
}