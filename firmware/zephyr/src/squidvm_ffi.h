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
	SQDP_ACTION_NONE = 0,
	SQDP_ACTION_BEGIN_INSTALL = 1,
	SQDP_ACTION_WRITE_INSTALL_CHUNK = 2,
	SQDP_ACTION_COMMIT_INSTALL = 3,
	SQDP_ACTION_BEGIN_TEMP_RUN = 4,
	SQDP_ACTION_WRITE_TEMP_RUN_CHUNK = 5,
	SQDP_ACTION_COMMIT_TEMP_RUN = 6,
	SQDP_ACTION_BEGIN_RESOURCE_INSTALL = 7,
	SQDP_ACTION_WRITE_RESOURCE_CHUNK = 8,
	SQDP_ACTION_COMMIT_RESOURCE_INSTALL = 9,
} SqdpActionKind;

typedef struct {
	SqdpActionKind kind;
	const uint8_t *app_id;
	size_t app_id_len;
	const uint8_t *resource_path;
	size_t resource_path_len;
	const uint8_t *staging_path;
	size_t staging_path_len;
	size_t offset;
	const uint8_t *bytes;
	size_t bytes_len;
	size_t total_len;
} SqdpAction;

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
typedef void (*SqvmDebugOutputCallback)(void *user_data, const uint8_t *message, size_t message_len);
typedef int32_t (*SqvmIndicatorWriteCallback)(void *user_data, bool value);
typedef int32_t (*SqvmIndicatorToggleCallback)(void *user_data);
typedef int32_t (*SqvmIndicatorReadCallback)(void *user_data, bool *out);
typedef int32_t (*SqvmIndicatorBreatheCallback)(void *user_data);
typedef int32_t (*SqvmTimerEveryCallback)(
	void *user_data,
	const uint8_t *event,
	size_t event_len,
	int32_t interval_ms);
typedef int32_t (*SqvmTimerAfterCallback)(
	void *user_data,
	const uint8_t *event,
	size_t event_len,
	int32_t delay_ms);

typedef struct {
	void *user_data;
	SqvmTraceCallback trace;
	SqvmReadExactAtCallback read_exact_at;
	SqvmDebugOutputCallback debug_output;
	SqvmIndicatorWriteCallback indicator_write;
	SqvmIndicatorToggleCallback indicator_toggle;
	SqvmIndicatorReadCallback indicator_read;
	SqvmIndicatorBreatheCallback indicator_breathe;
	SqvmTimerEveryCallback timer_every;
	SqvmTimerAfterCallback timer_after;
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
SqdpStatus sqdp_prepare_transfer_begin(
	const uint8_t *request,
	size_t request_len,
	void *session,
	SqdpAction *out_action);
SqdpStatus sqdp_prepare_transfer_chunk(
	const uint8_t *request,
	size_t request_len,
	const void *session,
	SqdpAction *out_action);
SqdpStatus sqdp_complete_transfer_chunk(
	void *session,
	const uint8_t *bytes,
	size_t bytes_len);
SqdpStatus sqdp_prepare_transfer_commit(
	const uint8_t *request,
	size_t request_len,
	const void *session,
	SqdpAction *out_action);
void sqdp_clear_transfer_session(void *session);
SqdpStatus sqdp_prepare_resource_begin(
	const uint8_t *request,
	size_t request_len,
	void *session,
	SqdpAction *out_action);
SqdpStatus sqdp_prepare_resource_chunk(
	const uint8_t *request,
	size_t request_len,
	const void *session,
	SqdpAction *out_action);
SqdpStatus sqdp_complete_resource_chunk(
	void *session,
	const uint8_t *bytes,
	size_t bytes_len);
SqdpStatus sqdp_prepare_resource_commit(
	const uint8_t *request,
	size_t request_len,
	const void *session,
	SqdpAction *out_action);
void sqdp_clear_resource_session(void *session);

#ifdef __cplusplus
}
#endif

#endif
