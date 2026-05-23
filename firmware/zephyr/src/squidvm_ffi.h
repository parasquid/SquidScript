#ifndef SQUIDSCRIPT_SQUIDVM_FFI_H
#define SQUIDSCRIPT_SQUIDVM_FFI_H

#include <stddef.h>
#include <stdbool.h>
#include <stdint.h>

#define SQVM_STORAGE_TRANSFER_CAPACITY 1024

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
	SQVM_STATUS_OK = 0,
	SQVM_STATUS_INVALID_ARGUMENT = 1,
	SQVM_STATUS_VM_ERROR = 2,
} SqvmStatus;

typedef enum {
	SQDP_STATUS_OK = 0,
	SQDP_STATUS_INVALID_ARGUMENT = 1,
	SQDP_STATUS_BUFFER_TOO_SMALL = 2,
	SQDP_STATUS_ENCODE_ERROR = 3,
} SqdpStatus;

typedef enum {
	SQVM_DISPATCH_COMPLETE = 0,
	SQVM_DISPATCH_PENDING_STORAGE = 1,
} SqvmDispatchOutcome;

typedef enum {
	SQVM_STORAGE_REQUEST_NONE = 0,
	SQVM_STORAGE_REQUEST_SQBC_READ = 1,
	SQVM_STORAGE_REQUEST_STATE_LOAD = 2,
	SQVM_STORAGE_REQUEST_STATE_SAVE = 3,
	SQVM_STORAGE_REQUEST_STATE_RESET = 4,
} SqvmStorageRequestKind;

typedef struct {
	SqvmStorageRequestKind kind;
	size_t offset;
	size_t len;
	uint8_t bytes[SQVM_STORAGE_TRANSFER_CAPACITY];
} SqvmStorageRequest;

typedef struct {
	bool has_len;
	size_t len;
	uint8_t bytes[SQVM_STORAGE_TRANSFER_CAPACITY];
} SqvmStorageCompletion;

typedef struct {
	SqvmStatus status;
	SqvmDispatchOutcome outcome;
	SqvmStorageRequest storage;
} SqvmDispatchResult;

typedef void (*SqvmTraceCallback)(void *user_data, const uint8_t *message, size_t message_len);
typedef int32_t (*SqvmReadExactAtCallback)(
	void *user_data,
	size_t offset,
	uint8_t *out,
	size_t out_len);

typedef struct {
	void *user_data;
	SqvmTraceCallback trace;
	SqvmReadExactAtCallback read_exact_at;
} SqvmCallbacks;

size_t sqvm_context_size(void);
size_t sqvm_context_align(void);
size_t sqvm_storage_transfer_capacity(void);
SqvmStatus sqvm_context_prepare(void *context, size_t context_len);
SqvmStatus sqvm_context_init_in_place(
	void *context,
	SqvmCallbacks callbacks,
	uint8_t *scratch,
	size_t scratch_len);
SqvmStatus sqvm_dispatch(
	void *context,
	SqvmCallbacks callbacks,
	const uint8_t *event,
	size_t event_len);
SqvmStatus sqvm_dispatch_start_resumable(
	void *context,
	SqvmCallbacks callbacks,
	const uint8_t *event,
	size_t event_len,
	SqvmDispatchResult *out_result);
SqvmStatus sqvm_dispatch_resume_storage(
	void *context,
	SqvmCallbacks callbacks,
	const SqvmStorageCompletion *completion,
	SqvmDispatchResult *out_result);
SqdpStatus sqdp_encode_empty_response(
	uint8_t opcode,
	uint8_t status,
	uint32_t sequence,
	uint8_t *out,
	size_t out_cap,
	size_t *out_len);
SqdpStatus sqdp_encode_hello_response(
	uint8_t opcode,
	uint32_t sequence,
	const uint8_t *target,
	size_t target_len,
	const uint8_t *firmware,
	size_t firmware_len,
	bool diagnostic,
	uint8_t *out,
	size_t out_cap,
	size_t *out_len);
SqdpStatus sqdp_encode_error_response(
	uint8_t opcode,
	uint32_t sequence,
	int64_t code,
	const uint8_t *message,
	size_t message_len,
	uint8_t *out,
	size_t out_cap,
	size_t *out_len);

#ifdef __cplusplus
}
#endif

#endif
