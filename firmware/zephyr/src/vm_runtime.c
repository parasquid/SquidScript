#include "vm_runtime.h"

#include <errno.h>
#include <string.h>
#include <stddef.h>

#define SQ_VM_RUNTIME_WORK_STACK_SIZE 16384

K_THREAD_STACK_DEFINE(sq_vm_runtime_work_stack, SQ_VM_RUNTIME_WORK_STACK_SIZE);
static struct k_work_q sq_vm_runtime_work_q;
static bool sq_vm_runtime_work_q_started;

static void runtime_trace(void *user_data, const uint8_t *message, size_t message_len)
{
	struct sq_vm_runtime *runtime = user_data;

	if (runtime->trace_count >= SQ_VM_RUNTIME_TRACE_MAX) {
		return;
	}

	size_t len = message_len;
	if (len >= SQ_VM_RUNTIME_TRACE_LEN) {
		len = SQ_VM_RUNTIME_TRACE_LEN - 1;
	}
	memcpy(runtime->traces[runtime->trace_count], message, len);
	runtime->traces[runtime->trace_count][len] = '\0';
	runtime->trace_count++;
}

static int32_t runtime_read_exact_at(void *user_data, size_t offset, uint8_t *out, size_t out_len)
{
	struct sq_vm_runtime *runtime = user_data;

	if (runtime->backend == NULL || runtime->backend->read_sqbc == NULL) {
		return -EINVAL;
	}
	return runtime->backend->read_sqbc(runtime->backend->user_data, offset, out, out_len);
}

static void clear_dispatch_state(struct sq_vm_runtime *runtime)
{
	memset(runtime->context_words, 0, sizeof(runtime->context_words));
	memset(runtime->scratch, 0, sizeof(runtime->scratch));
	memset(&runtime->result, 0, sizeof(runtime->result));
	memset(&runtime->completion, 0, sizeof(runtime->completion));
	memset(runtime->traces, 0, sizeof(runtime->traces));
	runtime->trace_count = 0;
	runtime->backend = NULL;
}

static void runtime_work_handler(struct k_work *work)
{
	struct sq_vm_runtime *runtime = CONTAINER_OF(work, struct sq_vm_runtime, work);
	int result = sq_vm_runtime_dispatch(runtime, &runtime->job_backend, runtime->event);

	runtime->result_code = result;
	runtime->status = result == 0 ? SQ_VM_RUNTIME_COMPLETE : SQ_VM_RUNTIME_ERROR;
}

void sq_vm_runtime_init(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL || runtime->work_initialized) {
		return;
	}
	if (!sq_vm_runtime_work_q_started) {
		k_work_queue_start(&sq_vm_runtime_work_q, sq_vm_runtime_work_stack,
				   K_THREAD_STACK_SIZEOF(sq_vm_runtime_work_stack), 5, NULL);
		sq_vm_runtime_work_q_started = true;
	}
	k_work_init(&runtime->work, runtime_work_handler);
	runtime->work_initialized = true;
	runtime->status = SQ_VM_RUNTIME_IDLE;
}

int sq_vm_runtime_dispatch(struct sq_vm_runtime *runtime,
			   const struct sq_vm_storage_backend *backend, const char *event)
{
	SqvmCallbacks callbacks;
	SqvmStatus status;

	if (runtime == NULL || backend == NULL || event == NULL) {
		return -EINVAL;
	}
	if (sqvm_context_size() > sizeof(runtime->context_words)) {
		return -ENOMEM;
	}

	clear_dispatch_state(runtime);
	runtime->backend = backend;
	callbacks = (SqvmCallbacks){
		.user_data = runtime,
		.trace = runtime_trace,
		.read_exact_at = runtime_read_exact_at,
	};

	status = sqvm_context_prepare(runtime->context_words, sizeof(runtime->context_words));
	if (status != SQVM_STATUS_OK) {
		return -EIO;
	}
	status = sqvm_context_init_in_place(runtime->context_words, callbacks, runtime->scratch,
					    sizeof(runtime->scratch));
	if (status != SQVM_STATUS_OK) {
		return -EIO;
	}
	status = sqvm_dispatch_start_resumable(runtime->context_words, callbacks,
					       (const uint8_t *)event, strlen(event),
					       &runtime->result);
	if (status != SQVM_STATUS_OK) {
		return -EIO;
	}

	while (runtime->result.outcome == SQVM_DISPATCH_PENDING_STORAGE) {
		int storage_result = sq_vm_storage_complete_request(backend, &runtime->result.storage,
								   &runtime->completion);
		if (storage_result != 0) {
			return storage_result;
		}
		status = sqvm_dispatch_resume_storage(runtime->context_words, callbacks,
						      &runtime->completion, &runtime->result);
		if (status != SQVM_STATUS_OK) {
			return -EIO;
		}
	}

	return runtime->result.outcome == SQVM_DISPATCH_COMPLETE ? 0 : -EIO;
}

int sq_vm_runtime_start(struct sq_vm_runtime *runtime,
			const struct sq_vm_storage_backend *backend, const char *event)
{
	size_t event_len;

	if (runtime == NULL || backend == NULL || event == NULL) {
		return -EINVAL;
	}
	sq_vm_runtime_init(runtime);
	if (runtime->status == SQ_VM_RUNTIME_RUNNING) {
		return -EBUSY;
	}
	event_len = strlen(event);
	if (event_len == 0 || event_len >= sizeof(runtime->event)) {
		return -EINVAL;
	}

	runtime->job_backend = *backend;
	memcpy(runtime->event, event, event_len + 1);
	runtime->result_code = 0;
	runtime->status = SQ_VM_RUNTIME_RUNNING;
	k_work_submit_to_queue(&sq_vm_runtime_work_q, &runtime->work);
	return 0;
}
