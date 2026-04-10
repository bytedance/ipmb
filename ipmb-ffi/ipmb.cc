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
    Label::Label()
        : ptr_(ipmb_ffi::ipmb_label(),
               [](ipmb_ffi::Label raw) { ipmb_ffi::ipmb_label_drop(raw); }) {}

    Label::Label(std::initializer_list<std::string> list) : Label() {
      auto* raw = ptr_.get();
      for (auto& i : list) {
        ipmb_ffi::ipmb_label_insert(&raw, i.c_str());
      }
    }

    void Label::append(const std::string& s) {
      auto* raw = ptr_.get();
      ipmb_ffi::ipmb_label_insert(&raw, s.c_str());
    }

    /// LabelOp
    LabelOp::LabelOp(bool v)
        : ptr_(ipmb_ffi::ipmb_label_op_bool(v), [](ipmb_ffi::LabelOp raw) {
            ipmb_ffi::ipmb_label_op_drop(raw);
          }) {}

    LabelOp::LabelOp(const std::string& s) : LabelOp(s.c_str()) {}

    LabelOp::LabelOp(const char* s)
        : ptr_(ipmb_ffi::ipmb_label_op_leaf(s), [](ipmb_ffi::LabelOp raw) {
            ipmb_ffi::ipmb_label_op_drop(raw);
          }) {}

    void LabelOp::op_not() {
      auto* raw = ipmb_ffi::ipmb_label_op_not(ptr_.release());
      ptr_.reset(raw);
    }

    void LabelOp::op_and(LabelOp right) {
      auto* raw =
          ipmb_ffi::ipmb_label_op_and(ptr_.release(), right.ptr_.release());
      ptr_.reset(raw);
    }

    void LabelOp::op_or(LabelOp right) {
      auto* raw =
          ipmb_ffi::ipmb_label_op_or(ptr_.release(), right.ptr_.release());
      ptr_.reset(raw);
    }

    /// Options
    Options::Options(std::string identifier, Label label, std::string token, bool controller_affinity)
            : identifier(std::move(identifier)),
              label(std::move(label)),
              token(std::move(token)),
              controller_affinity(controller_affinity) {}

    ipmb_ffi::Options Options::as_ffi() {
      label_raw_ = label.ptr_.get();
      ipmb_ffi::Options options{identifier.c_str(), &label_raw_, token.c_str(),
                                controller_affinity};

      return options;
    }

    /// Selector
    Selector::Selector(ipmb::LabelOp label_op, ipmb_ffi::SelectorMode mode, uint32_t ttl)
            : label_op(std::move(label_op)),
              mode(mode),
              ttl(ttl) {}

    ipmb_ffi::Selector Selector::as_ffi() {
      label_op_raw_ = label_op.ptr_.get();
      ipmb_ffi::Selector selector{&label_op_raw_, mode, ttl};

      return selector;
    }

    /// MemoryRegion
    std::tuple<MemoryRegion, Error> MemoryRegion::create(uintptr_t size) {
      ipmb_ffi::MemoryRegion raw;
      switch (ipmb_ffi::ipmb_memory_region(size, &raw)) {
        case ipmb_ffi::ERROR_CODE_SUCCESS:
          return std::make_tuple(MemoryRegion(raw), Error::kSuccess);
        default:
          return std::make_tuple(MemoryRegion(nullptr), Error::kUnknown);
      }
    }

    std::tuple<MemoryRegion, Error> MemoryRegion::clone() {
      ipmb_ffi::MemoryRegion cloned_raw = nullptr;
      auto* raw = ptr_.get();
      auto r = ipmb_ffi::ipmb_memory_region_clone(&raw, &cloned_raw);
      switch (r) {
        case ipmb_ffi::ERROR_CODE_SUCCESS:
          return std::make_tuple(MemoryRegion(cloned_raw), Error::kSuccess);
        default:
          return std::make_tuple(MemoryRegion(nullptr), Error::kUnknown);
      }
    }

    std::tuple<uint8_t*, intptr_t, Error> MemoryRegion::map(uintptr_t offset,
                                                            intptr_t size) {
      intptr_t real_size = 0;
      auto* raw = ptr_.get();
      auto ptr =
          ipmb_ffi::ipmb_memory_region_map(&raw, offset, size, &real_size);
      if (ptr) {
        return std::make_tuple(ptr, real_size, Error::kSuccess);
      } else {
        return std::make_tuple(nullptr, 0, Error::kUnknown);
      }
    }

    std::tuple<uint32_t, Error> MemoryRegion::ref_count() {
      auto* raw = ptr_.get();
      return std::make_tuple(ipmb_ffi::ipmb_memory_region_ref_count(&raw),
                             Error::kSuccess);
    }

    MemoryRegion::MemoryRegion(ipmb_ffi::MemoryRegion raw)
        : ptr_(raw, [](ipmb_ffi::MemoryRegion raw) {
            ipmb_ffi::ipmb_memory_region_drop(raw);
          }) {}

    /// MemoryRegistry
    MemoryRegistry::MemoryRegistry()
        : ptr_(ipmb_ffi::ipmb_memory_registry(),
               [](ipmb_ffi::MemoryRegistry raw) {
                 ipmb_ffi::ipmb_memory_registry_drop(raw);
               }) {}

    std::tuple<MemoryRegion, Error> MemoryRegistry::alloc(
        uintptr_t min_size,
        const std::string* tag) {
      const char* tag_raw = nullptr;
      if (tag) {
        tag_raw = tag->c_str();
      }

      ipmb_ffi::MemoryRegion region_raw = nullptr;
      auto* raw = ptr_.get();
      auto r = ipmb_ffi::ipmb_memory_registry_alloc(&raw, min_size, tag_raw,
                                                    &region_raw);
      switch (r) {
        case ipmb_ffi::ERROR_CODE_SUCCESS:
            return std::make_tuple(MemoryRegion(region_raw),
                               Error::kSuccess);
        default:
          return std::make_tuple(MemoryRegion(nullptr), Error::kUnknown);
      }
    }

    extern "C" void free_function(void* free_context) {
        auto fp = static_cast<std::function<void()>*>(free_context);
        (*fp)();
        delete fp;
    }

    std::tuple<MemoryRegion, Error> MemoryRegistry::alloc_with_free(
        uintptr_t min_size,
        const std::string* tag,
        std::function<void()> free) {
      const char* tag_raw = nullptr;
      if (tag) {
        tag_raw = tag->c_str();
      }

      auto f = new std::function<void()>(std::move(free));

      ipmb_ffi::MemoryRegion region_raw = nullptr;
      auto* raw = ptr_.get();
      auto r = ipmb_ffi::ipmb_memory_registry_alloc_with_free(
          &raw, min_size, tag_raw, f, free_function, &region_raw);
      switch (r) {
        case ipmb_ffi::ERROR_CODE_SUCCESS:
            return std::make_tuple(MemoryRegion(region_raw),
                               Error::kSuccess);
        default:
          return std::make_tuple(MemoryRegion(nullptr), Error::kUnknown);
      }
    }

    Error MemoryRegistry::maintain() {
      auto* raw = ptr_.get();
      ipmb_ffi::ipmb_memory_registry_maintain(&raw);
      return Error::kSuccess;
    }

    /// Message
    Message::Message(Selector& selector,
                     uint16_t format,
                     const uint8_t* ptr,
                     uint32_t size)
        : Message(
              ipmb_ffi::ipmb_message(selector.as_ffi(), format, ptr, size)) {}

    std::tuple<uint16_t, const uint8_t*, uint32_t, Error>
    Message::bytes_data() {
      uint16_t format = 0;
      const uint8_t* ptr = nullptr;
      uint32_t size = 0;

      auto* raw = ptr_.get();
      ipmb_ffi::ipmb_message_bytes_data(&raw, &format, &ptr, &size);

      return std::make_tuple(format, ptr, size, Error::kSuccess);
    }

    void Message::object_append(ipmb_ffi::Object obj) {
      auto* raw = ptr_.get();
      ipmb_ffi::ipmb_message_object_append(&raw, obj);
    }

    std::tuple<ipmb_ffi::Object, Error> Message::object_retrieve(uintptr_t index) {
      auto* raw = ptr_.get();
      auto obj = ipmb_ffi::ipmb_message_object_retrieve(&raw, index);
      if (obj) {
        return std::make_tuple(obj, Error::kSuccess);
      } else {
        return std::make_tuple(0, Error::kUnknown);
      }
    }

    std::tuple<ipmb_ffi::Object, Error> Message::object_get(uintptr_t index) {
      auto* raw = ptr_.get();
      auto obj = ipmb_ffi::ipmb_message_object_get(&raw, index);
      if (obj) {
        return std::make_tuple(obj, Error::kSuccess);
      } else {
        return std::make_tuple(0, Error::kUnknown);
      }
    }

    void Message::memory_region_append(MemoryRegion region) {
      auto* raw = ptr_.get();
      ipmb_ffi::ipmb_message_memory_region_append(&raw, region.ptr_.release());
    }

    std::tuple<MemoryRegion, Error> Message::memory_region_retrieve(uintptr_t index) {
      auto* raw = ptr_.get();
      auto region_raw =
          ipmb_ffi::ipmb_message_memory_region_retrieve(&raw, index);
      if (region_raw) {
        return std::make_tuple(MemoryRegion(region_raw), Error::kSuccess);
      } else {
        return std::make_tuple(MemoryRegion(nullptr), Error::kUnknown);
      }
    }

    Message::Message(ipmb_ffi::Message raw)
        : ptr_(raw, [](ipmb_ffi::Message raw) {
            ipmb_ffi::ipmb_message_drop(raw);
          }) {}

    /// Sender
    Sender::Sender(ipmb_ffi::Sender raw)
        : ptr_(raw,
               [](ipmb_ffi::Sender raw) { ipmb_ffi::ipmb_sender_drop(raw); }) {}

    Error Sender::send(Message message) {
      auto* raw = ptr_.get();
      auto r = ipmb_ffi::ipmb_send(&raw, message.ptr_.release());

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
        case ipmb_ffi::ERROR_CODE_PERMISSION_DENIED:
          return Error::kPermissionDenied;
        default:
          return Error::kUnknown;
      }
    }

    /// Receiver
    Receiver::Receiver(ipmb_ffi::Receiver raw)
        : ptr_(raw, [](ipmb_ffi::Receiver raw) {
            ipmb_ffi::ipmb_receiver_drop(raw);
          }) {}

    std::tuple<Message, Error> Receiver::recv(uint32_t timeout) {
      ipmb_ffi::Message m_raw = nullptr;
      auto* raw = ptr_.get();
      auto r = ipmb_ffi::ipmb_recv(&raw, &m_raw, timeout);

      switch (r) {
        case ipmb_ffi::ERROR_CODE_SUCCESS:
          return std::make_tuple(Message(m_raw), Error::kSuccess);
        case ipmb_ffi::ERROR_CODE_TIMEOUT:
          return std::make_tuple(Message(nullptr), Error::kTimeout);
        case ipmb_ffi::ERROR_CODE_DECODE:
          return std::make_tuple(Message(nullptr), Error::kDecode);
        case ipmb_ffi::ERROR_CODE_TOKEN_MISMATCH:
          return std::make_tuple(Message(nullptr), Error::kTokenMismatch);
        case ipmb_ffi::ERROR_CODE_VERSION_MISMATCH:
          return std::make_tuple(Message(nullptr), Error::kVersionMismatch);
        case ipmb_ffi::ERROR_CODE_PERMISSION_DENIED:
          return std::make_tuple(Message(nullptr), Error::kPermissionDenied);
        default:
          return std::make_tuple(Message(nullptr), Error::kUnknown);
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
              return std::make_tuple(Sender(nullptr), Receiver(nullptr),
                                     Error::kTimeout);
            case ipmb_ffi::ERROR_CODE_DECODE:
              return std::make_tuple(Sender(nullptr), Receiver(nullptr),
                                     Error::kDecode);
            case ipmb_ffi::ERROR_CODE_TOKEN_MISMATCH:
              return std::make_tuple(Sender(nullptr), Receiver(nullptr),
                                     Error::kTokenMismatch);
            case ipmb_ffi::ERROR_CODE_VERSION_MISMATCH:
              return std::make_tuple(Sender(nullptr), Receiver(nullptr),
                                     Error::kVersionMismatch);
            case ipmb_ffi::ERROR_CODE_PERMISSION_DENIED:
              return std::make_tuple(Sender(nullptr), Receiver(nullptr),
                                     Error::kPermissionDenied);
            default:
              return std::make_tuple(Sender(nullptr), Receiver(nullptr),
                                     Error::kUnknown);
        }
    }
}
