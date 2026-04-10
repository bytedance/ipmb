#pragma once

#include <functional>
#include <initializer_list>
#include <tuple>
#include <type_traits>
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

    template <typename P>
    using OwnedPtr = std::unique_ptr<std::remove_pointer_t<P>, void (*)(P)>;

    class Label {
    public:
        Label();

        Label(std::initializer_list<std::string> list);

        void append(const std::string& s);

    private:
     OwnedPtr<ipmb_ffi::Label> ptr_;

     friend class Options;

     friend class Selector;
    };

    class LabelOp {
    public:
        LabelOp(bool v);

        LabelOp(const std::string& s);

        LabelOp(const char* s);

        void op_not();

        void op_and(LabelOp right);

        void op_or(LabelOp right);

       private:
        OwnedPtr<ipmb_ffi::LabelOp> ptr_;

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

       private:
        ipmb_ffi::Label label_raw_;
    };

    class Selector {
     public:
      Selector(LabelOp label_op, ipmb_ffi::SelectorMode mode, uint32_t ttl);

      LabelOp label_op;
      ipmb_ffi::SelectorMode mode;
      uint32_t ttl;

      ipmb_ffi::Selector as_ffi();

     private:
      ipmb_ffi::LabelOp label_op_raw_;
    };

    class MemoryRegion {
    public:
     static std::tuple<MemoryRegion, Error> create(uintptr_t size);

     std::tuple<uint8_t*, intptr_t, Error> map(uintptr_t offset, intptr_t size);

     std::tuple<uint32_t, Error> ref_count();

     std::tuple<MemoryRegion, Error> clone();

    private:
     MemoryRegion(ipmb_ffi::MemoryRegion raw);

     OwnedPtr<ipmb_ffi::MemoryRegion> ptr_;

     friend class Message;

     friend class MemoryRegistry;
    };

    class MemoryRegistry {
     public:
      MemoryRegistry();

      std::tuple<MemoryRegion, Error> alloc(uintptr_t min_size,
                                            const std::string* tag);

      std::tuple<MemoryRegion, Error> alloc_with_free(
          uintptr_t min_size,
          const std::string* tag,
          std::function<void()> free);

      Error maintain();

     private:
      OwnedPtr<ipmb_ffi::MemoryRegistry> ptr_;
    };

    class Message {
     public:
      Message(Selector& selector,
              uint16_t format,
              const uint8_t* ptr,
              uint32_t size);

      std::tuple<uint16_t, const uint8_t*, uint32_t, Error> bytes_data();

      void object_append(ipmb_ffi::Object obj);

      std::tuple<ipmb_ffi::Object, Error> object_retrieve(uintptr_t index);

      std::tuple<ipmb_ffi::Object, Error> object_get(uintptr_t index);

      void memory_region_append(MemoryRegion region);

      std::tuple<MemoryRegion, Error> memory_region_retrieve(uintptr_t index);

     private:
      Message(ipmb_ffi::Message raw);

      OwnedPtr<ipmb_ffi::Message> ptr_;

      friend class Sender;

      friend class Receiver;
    };

    class Receiver;

    class Sender {
    public:
     Error send(Message message);

    private:
        friend std::tuple<Sender, Receiver, Error> join(Options& options, uint32_t timeout);

        explicit Sender(ipmb_ffi::Sender raw);

        OwnedPtr<ipmb_ffi::Sender> ptr_;
    };

    class Receiver {
     public:
      std::tuple<Message, Error> recv(uint32_t timeout);

     private:
      friend std::tuple<Sender, Receiver, Error> join(Options& options,
                                                      uint32_t timeout);

      explicit Receiver(ipmb_ffi::Receiver raw);

      OwnedPtr<ipmb_ffi::Receiver> ptr_;
    };

    std::tuple<Sender, Receiver, Error> join(Options& options, uint32_t timeout);
}
