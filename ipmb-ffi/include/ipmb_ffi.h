#pragma once

#include <cstdarg>
#include <cstdint>
#include <cstdlib>
#include <ostream>
#include <new>

namespace ipmb_ffi {

constexpr static const uint32_t TIMEOUT_INFINITE = ~0u;

enum class SelectorMode {
    kUnicast,
    kMulticast,
};

using RString = void*;

using ErrorCode = int32_t;

/// Label
using Label = void*;

/// Options
struct Options {
    const char *identifier;
    const Label *label;
    const char *token;
    bool controller_affinity;
};

/// Sender
using Sender = void*;

/// Receiver
using Receiver = void*;

/// Message
using Message = void*;

/// MemoryRegistry
using MemoryRegistry = void*;

/// MemoryRegion
using MemoryRegion = void*;

/// LabelOp
using LabelOp = void*;

/// Selector
struct Selector {
    const LabelOp *label_op;
    SelectorMode mode;
    uint32_t ttl;
};

/// Kernel Object
using Object = uint64_t;

constexpr static const ErrorCode ERROR_CODE_SUCCESS = 0;

constexpr static const ErrorCode ERROR_CODE_UNKNOWN = -1;

constexpr static const ErrorCode ERROR_CODE_TIMEOUT = -2;

constexpr static const ErrorCode ERROR_CODE_DECODE = -3;

constexpr static const ErrorCode ERROR_CODE_VERSION_MISMATCH = -4;

constexpr static const ErrorCode ERROR_CODE_TOKEN_MISMATCH = -5;

constexpr static const ErrorCode ERROR_CODE_PERMISSION_DENIED = -6;

extern "C" {

void ipmb_rstring_data(const RString *rstring, const char **ptr, uintptr_t *size);

void ipmb_rstring_drop(RString rstring);

/// Get version
void ipmb_version(uint8_t *major, uint8_t *minor, uint8_t *patch);

RString ipmb_version_pre();

/// Join bus
ErrorCode ipmb_join(Options options, uint32_t timeout, Sender *p_sender, Receiver *p_receiver);

void ipmb_sender_drop(Sender sender);

ErrorCode ipmb_send(Sender *sender, Message message);

void ipmb_receiver_drop(Receiver receiver);

ErrorCode ipmb_recv(Receiver *receiver, Message *p_message, uint32_t timeout);

MemoryRegistry ipmb_memory_registry();

void ipmb_memory_registry_drop(MemoryRegistry registry);

MemoryRegion ipmb_memory_registry_alloc(MemoryRegistry *registry,
                                        uintptr_t min_size,
                                        const char *tag);

MemoryRegion ipmb_memory_registry_alloc_with_free(MemoryRegistry *registry,
                                                  uintptr_t min_size,
                                                  const char *tag,
                                                  void *free_context,
                                                  void (*free)(void*));

void ipmb_memory_registry_maintain(MemoryRegistry *registry);

Message ipmb_message(Selector selector, uint16_t format, const uint8_t *ptr, uint32_t size);

void ipmb_message_drop(Message message);

void ipmb_message_bytes_data(const Message *message,
                             uint16_t *format,
                             const uint8_t **ptr,
                             uint32_t *size);

void ipmb_message_object_append(Message *message, Object obj);

/// Retrieve object from message with ownership
Object ipmb_message_object_retrieve(Message *message, uintptr_t index);

/// Get object from message without ownership
Object ipmb_message_object_get(const Message *message, uintptr_t index);

/// Drop object with ownership
void ipmb_object_drop(Object obj);

void ipmb_message_memory_region_append(Message *message, MemoryRegion region);

/// Retrieve memory region from message with ownership
MemoryRegion ipmb_message_memory_region_retrieve(Message *message, uintptr_t index);

/// Get memory region from message without ownership
MemoryRegion ipmb_message_memory_region_get(Message *message, uintptr_t index);

MemoryRegion ipmb_memory_region(uintptr_t size);

void ipmb_memory_region_drop(MemoryRegion region);

uint8_t *ipmb_memory_region_map(MemoryRegion *region,
                                uintptr_t offset,
                                intptr_t size,
                                intptr_t *real_size);

/// Get reference count of memory region
uint32_t ipmb_memory_region_ref_count(const MemoryRegion *region);

/// Clone a new MemoryRegion and share the underlying kernel object.
/// # Safety
/// - region must be a valid MemoryRegion.
MemoryRegion ipmb_memory_region_clone(const MemoryRegion *region);

Label ipmb_label();

void ipmb_label_drop(Label label);

void ipmb_label_insert(Label *label, const char *s);

LabelOp ipmb_label_op_bool(bool v);

LabelOp ipmb_label_op_leaf(const char *s);

void ipmb_label_op_drop(LabelOp left);

LabelOp ipmb_label_op_not(LabelOp left);

LabelOp ipmb_label_op_and(LabelOp left, LabelOp right);

LabelOp ipmb_label_op_or(LabelOp left, LabelOp right);

} // extern "C"

} // namespace ipmb_ffi
