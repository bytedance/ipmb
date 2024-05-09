#include "ipmb.h"

namespace ipmb {
    /// Version
    Version::Version() {
        ipmb_ffi::ipmb_version(&major, &minor, &patch);

        auto pre_raw = ipmb_ffi::ipmb_version_pre();

        const char* pre_ptr = nullptr;
        uintptr_t pre_len = 0;
        ipmb_ffi::ipmb_rstring_data(&pre_raw, &pre_ptr, &pre_len);

        pre = std::string(pre_ptr, pre_len);

        ipmb_ffi::ipmb_rstring_drop(pre_raw);
    }

    /// Label
    Label::Label() {
        raw_ = ipmb_ffi::ipmb_label();
    }

    Label::Label(std::initializer_list<std::string> list) {
        raw_ = ipmb_ffi::ipmb_label();

        for (auto& i: list) {
            ipmb_ffi::ipmb_label_insert(&raw_, i.c_str());
        }
    }

    Label::Label(Label&& other) noexcept {
        raw_ = other.raw_;
        other.raw_ = nullptr;
    }

    Label& Label::operator=(Label&& other) noexcept {
        if (this != &other) {
            if (raw_) {
                ipmb_ffi::ipmb_label_drop(raw_);
            }

            raw_ = other.raw_;
            other.raw_ = nullptr;
        }

        return *this;
    }

    Label::~Label() {
        if (raw_) {
            ipmb_ffi::ipmb_label_drop(raw_);
        }
    }

    /// LabelOp
    LabelOp::LabelOp(bool v) {
        raw_ = ipmb_ffi::ipmb_label_op_bool(v);
    }

    LabelOp::LabelOp(const std::string& s) {
        raw_ = ipmb_ffi::ipmb_label_op_leaf(s.c_str());
    }

    LabelOp::LabelOp(const char* s) {
        raw_ = ipmb_ffi::ipmb_label_op_leaf(s);
    }

    LabelOp::LabelOp(ipmb::LabelOp&& other) noexcept {
        raw_ = other.raw_;
        other.raw_ = nullptr;
    }

    LabelOp& LabelOp::operator=(LabelOp&& other) noexcept {
        if (this != &other) {
            if (raw_) {
                ipmb_ffi::ipmb_label_op_drop(raw_);
            }

            raw_ = other.raw_;
            other.raw_ = nullptr;
        }

        return *this;
    }

    LabelOp::~LabelOp() {
        if (raw_) {
            ipmb_ffi::ipmb_label_op_drop(raw_);
        }
    }

    void LabelOp::op_not() {
        if (raw_) {
            raw_ = ipmb_ffi::ipmb_label_op_not(raw_);
        }
    }

    void LabelOp::op_and(LabelOp&& right) {
        if (!raw_ || !right.raw_) {
            return;
        }

        raw_ = ipmb_ffi::ipmb_label_op_and(raw_, right.raw_);

        right.raw_ = nullptr;
    }

    void LabelOp::op_or(LabelOp&& right) {
        if (!raw_ || !right.raw_) {
            return;
        }

        raw_ = ipmb_ffi::ipmb_label_op_or(raw_, right.raw_);

        right.raw_ = nullptr;
    }

    /// Options
    Options::Options(std::string identifier, Label label, std::string token, bool controller_affinity)
            : identifier(std::move(identifier)),
              label(std::move(label)),
              token(std::move(token)),
              controller_affinity(controller_affinity) {}

    ipmb_ffi::Options Options::as_ffi() {
        ipmb_ffi::Options options {
                identifier.c_str(),
                &label.raw_,
                token.c_str(),
                controller_affinity
        };

        return options;
    }

    /// Selector
    Selector::Selector(ipmb::LabelOp label_op, ipmb_ffi::SelectorMode mode, uint32_t ttl)
            : label_op(std::move(label_op)),
              mode(mode),
              ttl(ttl) {}

    ipmb_ffi::Selector Selector::as_ffi() {
        ipmb_ffi::Selector selector {
                &label_op.raw_,
                mode,
                ttl
        };

        return selector;
    }

    /// MemoryRegion
    MemoryRegion::MemoryRegion(uintptr_t size) {
        raw_ = ipmb_ffi::ipmb_memory_region(size);
    }

    /**
    MemoryRegion::MemoryRegion(const MemoryRegion& other) {
        if (other.raw_) {
            raw_ = ipmb_ffi::ipmb_memory_region_clone(&other.raw_);
        }
    }

    MemoryRegion& MemoryRegion::operator=(const MemoryRegion& other) {
        if (raw_) {
            ipmb_ffi::ipmb_memory_region_drop(raw_);
            raw_ = nullptr;
        }

        if (other.raw_) {
            raw_ = ipmb_ffi::ipmb_memory_region_clone(&other.raw_);
        }

        return *this;
    }
    */

    MemoryRegion MemoryRegion::clone() {
        if (!raw_) {
            return MemoryRegion();
        }
        return MemoryRegion(ipmb_ffi::ipmb_memory_region_clone(&raw_));
    }

    MemoryRegion::MemoryRegion(MemoryRegion&& other) noexcept {
        raw_ = other.raw_;
        other.raw_ = nullptr;
    }

    MemoryRegion& MemoryRegion::operator=(MemoryRegion&& other) noexcept {
        if (this != &other) {
            if (raw_) {
                ipmb_ffi::ipmb_memory_region_drop(raw_);
            }

            raw_ = other.raw_;
            other.raw_ = nullptr;
        }

        return *this;
    }

    MemoryRegion::~MemoryRegion() {
        if (raw_) {
            ipmb_ffi::ipmb_memory_region_drop(raw_);
        }
    }

    std::tuple<uint8_t*, intptr_t, Error> MemoryRegion::map(uintptr_t offset, intptr_t size) {
        if (!raw_) {
            return std::make_tuple(nullptr, 0, Error::kUnknown);
        }

        intptr_t real_size = 0;
        auto ptr = ipmb_ffi::ipmb_memory_region_map(&raw_, offset, size, &real_size);
        if (ptr) {
            return std::make_tuple(ptr, real_size, Error::kSuccess);
        } else {
            return std::make_tuple(nullptr, 0, Error::kUnknown);
        }
    }

    std::tuple<uint32_t, Error> MemoryRegion::ref_count() {
        if (!raw_) {
            return std::make_tuple(0, Error::kUnknown);
        }

        return std::make_tuple(ipmb_ffi::ipmb_memory_region_ref_count(&raw_), Error::kSuccess);
    }

    MemoryRegion::MemoryRegion(ipmb_ffi::MemoryRegion raw) : raw_(raw) {}

    /// MemoryRegistry
    MemoryRegistry::MemoryRegistry() {
        raw_ = ipmb_ffi::ipmb_memory_registry();
    }

    MemoryRegistry::MemoryRegistry(MemoryRegistry&& other) noexcept {
        raw_ = other.raw_;
        other.raw_ = nullptr;
    }

    MemoryRegistry& MemoryRegistry::operator=(MemoryRegistry&& other) noexcept {
        if (this != &other) {
            if (raw_) {
                ipmb_ffi::ipmb_memory_registry_drop(raw_);
            }

            raw_ = other.raw_;
            other.raw_ = nullptr;
        }

        return *this;
    }

    MemoryRegistry::~MemoryRegistry() {
        if (raw_) {
            ipmb_ffi::ipmb_memory_registry_drop(raw_);
        }
    }

    std::tuple<MemoryRegion, Error> MemoryRegistry::alloc(uintptr_t min_size, const std::string* tag) {
        if (!raw_) {
            return std::make_tuple(MemoryRegion(), Error::kUnknown);
        }

        const char* tag_raw = nullptr;
        if (tag) {
            tag_raw = tag->c_str();
        }

        return std::make_tuple(MemoryRegion(ipmb_ffi::ipmb_memory_registry_alloc(&raw_, min_size, tag_raw)),
                               Error::kSuccess);
    }

    extern "C" void free_function(void* free_context) {
        auto fp = static_cast<std::function<void()>*>(free_context);
        (*fp)();
        delete fp;
    }

    std::tuple<MemoryRegion, Error>
    MemoryRegistry::alloc_with_free(uintptr_t min_size, const std::string* tag, std::function<void()> free) {
        if (!raw_) {
            return std::make_tuple(MemoryRegion(), Error::kUnknown);
        }

        const char* tag_raw = nullptr;
        if (tag) {
            tag_raw = tag->c_str();
        }

        auto f = new std::function<void()>(std::move(free));

        return std::make_tuple(MemoryRegion(ipmb_ffi::ipmb_memory_registry_alloc_with_free(
                &raw_, min_size, tag_raw,
                f,
                free_function
        )), Error::kSuccess);
    }

    Error MemoryRegistry::maintain() {
        if (!raw_) {
            return Error::kUnknown;
        }

        ipmb_ffi::ipmb_memory_registry_maintain(&raw_);
        return Error::kSuccess;
    }

    /// Message
    Message::Message(Selector& selector, uint16_t format, const uint8_t* ptr, uint32_t size) {
        raw_ = ipmb_ffi::ipmb_message(selector.as_ffi(), format, ptr, size);
    }

    Message::Message(Message&& other) noexcept {
        raw_ = other.raw_;
        other.raw_ = nullptr;
    }

    Message& Message::operator=(Message&& other) noexcept {
        if (this != &other) {
            if (raw_) {
                ipmb_ffi::ipmb_message_drop(raw_);
            }

            raw_ = other.raw_;
            other.raw_ = nullptr;
        }

        return *this;
    }

    Message::~Message() {
        if (raw_) {
            ipmb_ffi::ipmb_message_drop(raw_);
        }
    }

    std::tuple<uint16_t, const uint8_t*, uint32_t, Error> Message::bytes_data() {
        if (!raw_) {
            return std::make_tuple(0, nullptr, 0, Error::kUnknown);
        }

        uint16_t format = 0;
        const uint8_t* ptr = nullptr;
        uint32_t size = 0;
        ipmb_ffi::ipmb_message_bytes_data(&raw_, &format, &ptr, &size);

        return std::make_tuple(format, ptr, size, Error::kSuccess);
    }

    void Message::object_append(ipmb_ffi::Object obj) {
        if (raw_) {
            ipmb_ffi::ipmb_message_object_append(&raw_, obj);
        }
    }

    std::tuple<ipmb_ffi::Object, Error> Message::object_retrieve(uintptr_t index) {
        if (!raw_) {
            return std::make_tuple(0, Error::kUnknown);
        }

        auto obj = ipmb_ffi::ipmb_message_object_retrieve(&raw_, index);
        if (obj) {
            return std::make_tuple(obj, Error::kSuccess);
        } else {
            return std::make_tuple(0, Error::kUnknown);
        }
    }

    std::tuple<ipmb_ffi::Object, Error> Message::object_get(uintptr_t index) {
        if (!raw_) {
            return std::make_tuple(0, Error::kUnknown);
        }

        auto obj = ipmb_ffi::ipmb_message_object_get(&raw_, index);
        if (obj) {
            return std::make_tuple(obj, Error::kSuccess);
        } else {
            return std::make_tuple(0, Error::kUnknown);
        }
    }

    void Message::memory_region_append(MemoryRegion&& region) {
        if (!raw_ || !region.raw_) {
            return;
        }

        ipmb_ffi::ipmb_message_memory_region_append(&raw_, region.raw_);
        region.raw_ = nullptr;
    }

    std::tuple<MemoryRegion, Error> Message::memory_region_retrieve(uintptr_t index) {
        if (!raw_) {
            return std::make_tuple(MemoryRegion(), Error::kUnknown);
        }

        auto region_raw = ipmb_ffi::ipmb_message_memory_region_retrieve(&raw_, index);
        if (region_raw) {
            return std::make_tuple(MemoryRegion(region_raw), Error::kSuccess);
        } else {
            return std::make_tuple(MemoryRegion(), Error::kUnknown);
        }
    }

    Message::Message(ipmb_ffi::Message raw) : raw_(raw) {}

    /// Sender
    Sender::Sender(ipmb_ffi::Sender raw) : raw_(raw) {}

    Sender::Sender(Sender&& other) noexcept {
        raw_ = other.raw_;
        other.raw_ = nullptr;
    }

    Sender& Sender::operator=(Sender&& other) noexcept {
        if (this != &other) {
            if (raw_) {
                ipmb_ffi::ipmb_sender_drop(raw_);
            }

            raw_ = other.raw_;
            other.raw_ = nullptr;
        }

        return *this;
    }

    Sender::~Sender() {
        if (raw_) {
            ipmb_ffi::ipmb_sender_drop(raw_);
        }
    }

    Error Sender::send(Message&& message) {
        if (!raw_ || !message.raw_) {
            return Error::kUnknown;
        }

        auto r = ipmb_ffi::ipmb_send(&raw_, message.raw_);
        message.raw_ = nullptr;

        switch (r) {
            case ipmb_ffi::ERROR_CODE_SUCCESS:
                return Error::kSuccess;
            case ipmb_ffi::ERROR_CODE_TIMEOUT:
                return Error::kTimeout;
            case ipmb_ffi::ERROR_CODE_DECODE:
                return Error::kDecode;
            case ipmb_ffi::ERROR_CODE_TOKEN_MISMATCH:
                return Error::kTokenMismatch;
            case ipmb_ffi::ERROR_CODE_VERSION_MISMATCH:
                return Error::kVersionMismatch;
            default:
                return Error::kUnknown;
        }
    }

    /// Receiver
    Receiver::Receiver(ipmb_ffi::Receiver raw) : raw_(raw) {}

    Receiver::Receiver(Receiver&& other) noexcept {
        raw_ = other.raw_;
        other.raw_ = nullptr;
    }

    Receiver& Receiver::operator=(Receiver&& other) noexcept {
        if (this != &other) {
            if (raw_) {
                ipmb_ffi::ipmb_receiver_drop(raw_);
            }

            raw_ = other.raw_;
            other.raw_ = nullptr;
        }

        return *this;
    }

    Receiver::~Receiver() {
        if (raw_) {
            ipmb_ffi::ipmb_receiver_drop(raw_);
        }
    }

    std::tuple<Message, Error> Receiver::recv(uint32_t timeout) {
        if (!raw_) {
            return std::make_tuple(Message(), Error::kUnknown);
        }

        ipmb_ffi::Message m_raw = nullptr;
        auto r = ipmb_ffi::ipmb_recv(&raw_, &m_raw, timeout);

        switch (r) {
            case ipmb_ffi::ERROR_CODE_SUCCESS:
                return std::make_tuple(Message {m_raw}, Error::kSuccess);
            case ipmb_ffi::ERROR_CODE_TIMEOUT:
                return std::make_tuple(Message(), Error::kTimeout);
            case ipmb_ffi::ERROR_CODE_DECODE:
                return std::make_tuple(Message(), Error::kDecode);
            case ipmb_ffi::ERROR_CODE_TOKEN_MISMATCH:
                return std::make_tuple(Message(), Error::kTokenMismatch);
            case ipmb_ffi::ERROR_CODE_VERSION_MISMATCH:
                return std::make_tuple(Message(), Error::kVersionMismatch);
            default:
                return std::make_tuple(Message(), Error::kUnknown);
        }
    }

    /// join
    std::tuple<Sender, Receiver, Error> join(Options& options, uint32_t timeout) {
        ipmb_ffi::Sender s_raw = nullptr;
        ipmb_ffi::Receiver r_raw = nullptr;
        auto r = ipmb_ffi::ipmb_join(options.as_ffi(), timeout, &s_raw, &r_raw);

        switch (r) {
            case ipmb_ffi::ERROR_CODE_SUCCESS:
                return std::make_tuple(Sender(s_raw), Receiver(r_raw), Error::kSuccess);
            case ipmb_ffi::ERROR_CODE_TIMEOUT:
                return std::make_tuple(Sender(), Receiver(), Error::kTimeout);
            case ipmb_ffi::ERROR_CODE_DECODE:
                return std::make_tuple(Sender(), Receiver(), Error::kDecode);
            case ipmb_ffi::ERROR_CODE_TOKEN_MISMATCH:
                return std::make_tuple(Sender(), Receiver(), Error::kTokenMismatch);
            case ipmb_ffi::ERROR_CODE_VERSION_MISMATCH:
                return std::make_tuple(Sender(), Receiver(), Error::kVersionMismatch);
            default:
                return std::make_tuple(Sender(), Receiver(), Error::kUnknown);
        }
    }
}
