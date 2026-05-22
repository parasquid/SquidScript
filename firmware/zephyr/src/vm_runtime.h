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
#define SQ_VM_RUNTIME_CONTEXT_BYTES 65536
#define SQ_VM_RUNTIME_SCRATCH_BYTES 4096
#define SQ_VM_RUNTIME_EVENT_LEN 32

enum sq_vm_runtime_status {
	SQ_VM_RUNTIME_IDLE = 0,
	SQ_VM_RUNTIME_RUNNING = 1,
	SQ_VM_RUNTIME_COMPLETE = 2,
	SQ_VM_RUNTIME_ERROR = 3,
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
};

void sq_vm_runtime_init(struct sq_vm_runtime *runtime);

int sq_vm_runtime_dispatch(struct sq_vm_runtime *runtime,
			   const struct sq_vm_storage_backend *backend, const char *event);

int sq_vm_runtime_start(struct sq_vm_runtime *runtime,
			const struct sq_vm_storage_backend *backend, const char *event);

#ifdef __cplusplus
}
#endif

#endif
