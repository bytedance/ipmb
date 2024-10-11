#pragma once

#include <functional>
#include <initializer_list>
#include <tuple>
#include "ipmb_ffi.h"

namespace ipmb {
    enum class Error {
        kSuccess = 0,
        kUnknown = 1,
        kTimeout = 2,
        kDecode = 3,
        kVersionMismatch = 4,
        kTokenMismatch = 5,
        kPermissionDenied = 6,
    };

    class Version {
    public:
        Version();

        uint8_t major;
        uint8_t minor;
        uint8_t patch;
        std::string pre;
    };

    class Label {
    public:
        Label();

        Label(std::initializer_list<std::string> list);

        Label(Label& other) = delete;

        Label& operator=(const Label& other) = delete;

        Label(Label&& other) noexcept;

        Label& operator=(Label&& other) noexcept;

        ~Label();

    private:
        ipmb_ffi::Label raw_ = nullptr;

        friend class Options;

        friend class Selector;
    };

    class LabelOp {
    public:
        LabelOp(bool v);

        LabelOp(const std::string& s);

        LabelOp(const char* s);

        LabelOp(const LabelOp& other) = delete;

        LabelOp& operator=(const LabelOp& other) = delete;

        LabelOp(LabelOp&& other) noexcept;

        LabelOp& operator=(LabelOp&& other) noexcept;

        ~LabelOp();

        void op_not();

        void op_and(LabelOp&& right);

        void op_or(LabelOp&& right);

    private:
        ipmb_ffi::LabelOp raw_ = nullptr;

        friend class Selector;
    };

    class Options {
    public:
        Options(std::string identifier, Label label, std::string token, bool controller_affinity);

        std::string identifier;
        Label label;
        std::string token;
        bool controller_affinity;

        ipmb_ffi::Options as_ffi();
    };

    class Selector {
    public:
        Selector(LabelOp label_op, ipmb_ffi::SelectorMode mode, uint32_t ttl);

        LabelOp label_op;
        ipmb_ffi::SelectorMode mode;
        uint32_t ttl;

        ipmb_ffi::Selector as_ffi();
    };

    class MemoryRegion {
    public:
        MemoryRegion() = default;

        MemoryRegion(uintptr_t size);

        MemoryRegion(const MemoryRegion& other) = delete;

        MemoryRegion& operator=(const MemoryRegion& other) = delete;

        MemoryRegion(MemoryRegion&& other) noexcept;

        MemoryRegion& operator=(MemoryRegion&& other) noexcept;

        ~MemoryRegion();

        bool valid();

        std::tuple<uint8_t*, intptr_t, Error> map(uintptr_t offset, intptr_t size);

        std::tuple<uint32_t, Error> ref_count();

        MemoryRegion clone();

    private:
        ipmb_ffi::MemoryRegion raw_ = nullptr;

        MemoryRegion(ipmb_ffi::MemoryRegion raw);

        friend class Message;

        friend class MemoryRegistry;
    };

    class MemoryRegistry {
    public:
        MemoryRegistry();

        MemoryRegistry(const MemoryRegistry& other) = delete;

        MemoryRegistry& operator=(const MemoryRegistry& other) = delete;

        MemoryRegistry(MemoryRegistry&& other) noexcept;

        MemoryRegistry& operator=(MemoryRegistry&& other) noexcept;

        ~MemoryRegistry();

        std::tuple<MemoryRegion, Error> alloc(uintptr_t min_size, const std::string* tag);

        std::tuple<MemoryRegion, Error>
        alloc_with_free(uintptr_t min_size, const std::string* tag, std::function<void()> free);

        Error maintain();

    private:
        ipmb_ffi::MemoryRegistry raw_ = nullptr;
    };

    class Message {
    public:
        Message() = default;

        Message(Selector& selector, uint16_t format, const uint8_t* ptr, uint32_t size);

        Message(const Message& other) = delete;

        Message& operator=(const Message& other) = delete;

        Message(Message&& other) noexcept;

        Message& operator=(Message&& other) noexcept;

        ~Message();

        std::tuple<uint16_t, const uint8_t*, uint32_t, Error> bytes_data();

        void object_append(ipmb_ffi::Object obj);

        std::tuple<ipmb_ffi::Object, Error> object_retrieve(uintptr_t index);

        std::tuple<ipmb_ffi::Object, Error> object_get(uintptr_t index);

        void memory_region_append(MemoryRegion&& region);

        std::tuple<MemoryRegion, Error> memory_region_retrieve(uintptr_t index);

    private:
        ipmb_ffi::Message raw_ = nullptr;

        Message(ipmb_ffi::Message raw);

        friend class Sender;

        friend class Receiver;
    };

    class Receiver;

    class Sender {
    public:
        Sender() = default;

        Sender(const Sender& other) = delete;

        Sender& operator=(const Sender& other) = delete;

        Sender(Sender&& other) noexcept;

        Sender& operator=(Sender&& other) noexcept;

        ~Sender();

        Error send(Message&& message);

    private:
        friend std::tuple<Sender, Receiver, Error> join(Options& options, uint32_t timeout);

        explicit Sender(ipmb_ffi::Sender raw);

        ipmb_ffi::Sender raw_ = nullptr;
    };

    class Receiver {
    public:
        Receiver() = default;

        Receiver(const Receiver& other) = delete;

        Receiver& operator=(const Receiver& other) = delete;

        Receiver(Receiver&& other) noexcept;

        Receiver& operator=(Receiver&& other) noexcept;

        ~Receiver();

        std::tuple<Message, Error> recv(uint32_t timeout);

    private:
        friend std::tuple<Sender, Receiver, Error> join(Options& options, uint32_t timeout);

        explicit Receiver(ipmb_ffi::Receiver raw);

        ipmb_ffi::Receiver raw_ = nullptr;
    };

    std::tuple<Sender, Receiver, Error> join(Options& options, uint32_t timeout);
}
