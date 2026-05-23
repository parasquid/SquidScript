#ifndef SQUIDSCRIPT_VM_RUNTIME_H
#define SQUIDSCRIPT_VM_RUNTIME_H

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>
#include <zephyr/kernel.h>

#include "vm_storage.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SQ_VM_RUNTIME_TRACE_MAX 16
#define SQ_VM_RUNTIME_TRACE_LEN 32
#define SQ_VM_RUNTIME_OUTPUT_MAX 16
#define SQ_VM_RUNTIME_OUTPUT_LEN 64
#define SQ_VM_RUNTIME_TIMER_MAX 4
#define SQ_VM_RUNTIME_CONTEXT_BYTES 65536
#define SQ_VM_RUNTIME_SCRATCH_BYTES 4096
#define SQ_VM_RUNTIME_EVENT_LEN 32

enum sq_vm_runtime_status {
	SQ_VM_RUNTIME_IDLE = 0,
	SQ_VM_RUNTIME_RUNNING = 1,
	SQ_VM_RUNTIME_COMPLETE = 2,
	SQ_VM_RUNTIME_ERROR = 3,
};

struct sq_vm_runtime_timer {
	bool active;
	bool repeating;
	int32_t interval_ms;
	int64_t due_ms;
	char event[SQ_VM_RUNTIME_EVENT_LEN];
};

struct sq_vm_runtime {
	struct k_work work;
	bool work_initialized;
	uint64_t context_words[SQ_VM_RUNTIME_CONTEXT_BYTES / sizeof(uint64_t)];
	uint8_t scratch[SQ_VM_RUNTIME_SCRATCH_BYTES];
	SqvmDispatchResult result;
	SqvmStorageCompletion completion;
	const struct sq_vm_storage_backend *backend;
	struct sq_vm_storage_backend job_backend;
	char event[SQ_VM_RUNTIME_EVENT_LEN];
	enum sq_vm_runtime_status status;
	int result_code;
	char traces[SQ_VM_RUNTIME_TRACE_MAX][SQ_VM_RUNTIME_TRACE_LEN];
	size_t trace_count;
	char outputs[SQ_VM_RUNTIME_OUTPUT_MAX][SQ_VM_RUNTIME_OUTPUT_LEN];
	size_t output_count;
	bool indicator_state;
	bool indicator_gpio_configured;
	bool indicator_gpio_available;
	struct sq_vm_runtime_timer timers[SQ_VM_RUNTIME_TIMER_MAX];
};

void sq_vm_runtime_init(struct sq_vm_runtime *runtime);

void sq_vm_runtime_reset(struct sq_vm_runtime *runtime);

int sq_vm_runtime_dispatch(struct sq_vm_runtime *runtime,
			   const struct sq_vm_storage_backend *backend, const char *event);

int sq_vm_runtime_start(struct sq_vm_runtime *runtime,
			const struct sq_vm_storage_backend *backend, const char *event);

int sq_vm_runtime_record_output(struct sq_vm_runtime *runtime, const uint8_t *message,
				size_t message_len);
int sq_vm_runtime_indicator_write(struct sq_vm_runtime *runtime, bool value);
int sq_vm_runtime_indicator_toggle(struct sq_vm_runtime *runtime);
int sq_vm_runtime_indicator_read(struct sq_vm_runtime *runtime, bool *out);
int sq_vm_runtime_register_timer(struct sq_vm_runtime *runtime, const uint8_t *event,
				 size_t event_len, int32_t interval_ms, bool repeating);
int sq_vm_runtime_next_due_timer(struct sq_vm_runtime *runtime, char *event, size_t event_cap);
int sq_vm_runtime_poll(struct sq_vm_runtime *runtime);

#ifdef __cplusplus
}
#endif

#endif
