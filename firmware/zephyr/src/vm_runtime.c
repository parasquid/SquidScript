#include "vm_runtime_internal.h"

void sqvm_ffi_panic_abort(void)
{
	printk("sqvm ffi panic\n");
	k_panic();
	CODE_UNREACHABLE;
}

K_THREAD_STACK_DEFINE(sq_vm_runtime_work_stack, SQ_VM_RUNTIME_WORK_STACK_SIZE);
static struct k_thread sq_vm_runtime_work_thread;
static struct k_sem sq_vm_runtime_done_sem;
static struct sq_vm_runtime *sq_vm_runtime_active_work;
static bool sq_vm_runtime_work_sem_initialized;
static bool sq_vm_runtime_work_thread_started;

static bool runtime_has_pending_storage(const struct sq_vm_runtime *runtime);

static void runtime_trace(void *user_data, const uint8_t *message, size_t message_len)
{
	struct sq_vm_runtime *runtime = user_data;

	if (runtime->trace_count >= SQ_VM_RUNTIME_TRACE_MAX) {
		memmove(runtime->traces[0], runtime->traces[1],
			(SQ_VM_RUNTIME_TRACE_MAX - 1) * SQ_VM_RUNTIME_TRACE_LEN);
		runtime->trace_count = SQ_VM_RUNTIME_TRACE_MAX - 1;
	}

	size_t len = message_len;
	if (len >= SQ_VM_RUNTIME_TRACE_LEN) {
		len = SQ_VM_RUNTIME_TRACE_LEN - 1;
	}
	memcpy(runtime->traces[runtime->trace_count], message, len);
	runtime->traces[runtime->trace_count][len] = '\0';
	runtime->trace_count++;
}

int sq_vm_runtime_record_trace(struct sq_vm_runtime *runtime, const uint8_t *message,
			       size_t message_len)
{
	if (runtime == NULL || (message == NULL && message_len > 0)) {
		return -EINVAL;
	}
	runtime_trace(runtime, message, message_len);
	return 0;
}

int32_t runtime_read_exact_at(void *user_data, size_t offset, uint8_t *out, size_t out_len)
{
	struct sq_vm_runtime *runtime = user_data;

	if (runtime == NULL || runtime->backend == NULL || runtime->backend->read_sqbc == NULL ||
	    out == NULL) {
		return -EINVAL;
	}
	runtime->dispatch_sqbc_read_count++;
	runtime->dispatch_sqbc_read_bytes += out_len;
	return runtime->backend->read_sqbc(runtime->backend->user_data, offset, out, out_len);
}

static void runtime_record_pending_sqbc_read(struct sq_vm_runtime *runtime,
					     const SqvmStorageRequest *request,
					     const SqvmStorageCompletion *completion)
{
	if (runtime == NULL || request == NULL || completion == NULL ||
	    request->kind != SQVM_STORAGE_REQUEST_SQBC_READ || !completion->has_len) {
		return;
	}
	runtime->dispatch_sqbc_read_count++;
	runtime->dispatch_sqbc_read_bytes += completion->len;
}

static void runtime_finish_dispatch_metrics(struct sq_vm_runtime *runtime, uint64_t start_cycles)
{
	uint64_t elapsed_cycles = k_cycle_get_64() - start_cycles;

	runtime->dispatch_sequence++;
	runtime->last_dispatch_sequence = runtime->dispatch_sequence;
	runtime->last_dispatch_elapsed_us = k_cyc_to_us_floor64(elapsed_cycles);
	runtime->last_dispatch_sqbc_read_count = runtime->dispatch_sqbc_read_count;
	runtime->last_dispatch_sqbc_read_bytes = runtime->dispatch_sqbc_read_bytes;
}

static void runtime_debug_output(void *user_data, const uint8_t *message, size_t message_len)
{
	(void)sq_vm_runtime_record_output(user_data, message, message_len);
}

static void clear_dispatch_transfer(struct sq_vm_runtime *runtime)
{
	memset(&runtime->transfer, 0, sizeof(runtime->transfer));
#if IS_ENABLED(CONFIG_SQUIDSCRIPT_ZEPHYR_DIAGNOSTIC)
	runtime->transfer_owner = SQ_VM_RUNTIME_TRANSFER_FREE;
#endif
	memset(&runtime->result, 0, sizeof(runtime->result));
	runtime->backend = NULL;
}

static void runtime_run_job(struct sq_vm_runtime *runtime)
{
	int result = 0;
	bool complete = false;

	if (runtime == NULL) {
		return;
	}
	if (runtime->start_apply_bindings) {
		result = sq_vm_runtime_prepare_app_start(runtime);
		runtime->start_apply_bindings = false;
	}
	if (result != 0) {
		runtime->result_code = result;
		runtime->dispatch_exited = false;
		runtime->status = SQ_VM_RUNTIME_ERROR;
		return;
	}

	result = sq_vm_runtime_dispatch_slice(runtime, &runtime->job_backend, runtime->event, 1,
					      &complete);

	runtime->result_code = result;
	if (result == 0 && !complete) {
		return;
	}
	sq_app_lifecycle_cancel_pending_after_dispatch_error(runtime, result);
	runtime->dispatch_exited = result == 0 && runtime->result.exited;
	runtime->status = result == 0 ? SQ_VM_RUNTIME_COMPLETE : SQ_VM_RUNTIME_ERROR;
}

static void runtime_worker_thread(void *arg1, void *arg2, void *arg3)
{
	ARG_UNUSED(arg1);
	ARG_UNUSED(arg2);
	ARG_UNUSED(arg3);

	runtime_run_job(sq_vm_runtime_active_work);
	sq_vm_runtime_active_work = NULL;
	k_sem_give(&sq_vm_runtime_done_sem);
}

static int sq_vm_runtime_submit_work(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL) {
		return -EINVAL;
	}
	if (sq_vm_runtime_active_work != NULL) {
		return -EBUSY;
	}
	k_sem_reset(&sq_vm_runtime_done_sem);
	sq_vm_runtime_active_work = runtime;
	runtime->work_submitted = true;
	k_thread_create(&sq_vm_runtime_work_thread, sq_vm_runtime_work_stack,
			K_THREAD_STACK_SIZEOF(sq_vm_runtime_work_stack),
			runtime_worker_thread, NULL, NULL, NULL, 5, 0, K_NO_WAIT);
	k_thread_name_set(&sq_vm_runtime_work_thread, "sq_vm_runtime");
	sq_vm_runtime_work_thread_started = true;
	return 0;
}

void sq_vm_runtime_init(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL || runtime->work_initialized) {
		return;
	}
	if (!sq_vm_runtime_work_sem_initialized) {
		k_sem_init(&sq_vm_runtime_done_sem, 0, 1);
		sq_vm_runtime_work_sem_initialized = true;
	}
	runtime->work_initialized = true;
	runtime->work_submitted = false;
	runtime->status = SQ_VM_RUNTIME_IDLE;
}

size_t sq_vm_runtime_work_stack_size(void)
{
	return K_THREAD_STACK_SIZEOF(sq_vm_runtime_work_stack);
}

int sq_vm_runtime_work_stack_unused(size_t *unused)
{
	if (unused == NULL) {
		return -EINVAL;
	}

#if defined(CONFIG_INIT_STACKS) && defined(CONFIG_THREAD_STACK_INFO)
	if (!sq_vm_runtime_work_thread_started) {
		*unused = sq_vm_runtime_work_stack_size();
		return 0;
	}
	return k_thread_stack_space_get(&sq_vm_runtime_work_thread, unused);
#else
	*unused = 0;
	return -ENOTSUP;
#endif
}

int sq_vm_runtime_wait_idle(struct sq_vm_runtime *runtime, int32_t timeout_ms)
{
	int64_t deadline_ms;

	if (runtime == NULL) {
		return -EINVAL;
	}
	if (runtime->status == SQ_VM_RUNTIME_RUNNING) {
		if (timeout_ms <= 0) {
			return -ETIMEDOUT;
		}

		deadline_ms = k_uptime_get() + timeout_ms;
		while (runtime->status == SQ_VM_RUNTIME_RUNNING) {
			k_timeout_t timeout;

			if (k_uptime_get() >= deadline_ms) {
				return -ETIMEDOUT;
			}
			if (runtime->work_submitted) {
				timeout = K_MSEC(MAX((int64_t)1, deadline_ms - k_uptime_get()));
				if (k_thread_join(&sq_vm_runtime_work_thread, timeout) != 0) {
					return -ETIMEDOUT;
				}
				runtime->work_submitted = false;
				continue;
			}
			if (runtime_has_pending_storage(runtime)) {
				int result = sq_vm_runtime_submit_work(runtime);

				if (result != 0) {
					return result;
				}
				continue;
			}
			k_sleep(K_MSEC(1));
		}
	}
	if (!runtime->work_initialized) {
		return 0;
	}
	if (runtime->work_submitted) {
		k_timeout_t timeout = timeout_ms <= 0 ? K_NO_WAIT : K_MSEC(timeout_ms);

		if (k_thread_join(&sq_vm_runtime_work_thread, timeout) != 0) {
			return -ETIMEDOUT;
		}
		runtime->work_submitted = false;
	}
	return 0;
}

void sq_vm_runtime_reset(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL) {
		return;
	}
	sq_vm_runtime_reset_vm_context(runtime);
	memset(&runtime->job_backend, 0, sizeof(runtime->job_backend));
	memset(runtime->event, 0, sizeof(runtime->event));
	memset(runtime->traces, 0, sizeof(runtime->traces));
	runtime->trace_count = 0;
	memset(runtime->current_app, 0, sizeof(runtime->current_app));
	runtime->start_apply_bindings = false;
	runtime->lifecycle_phase = SQ_VM_RUNTIME_LIFECYCLE_IDLE;
	memset(runtime->lifecycle_target_app, 0, sizeof(runtime->lifecycle_target_app));
	memset(runtime->lifecycle_previous_app, 0, sizeof(runtime->lifecycle_previous_app));
	runtime->arm_phase = SQ_VM_RUNTIME_ARM_IDLE;
	memset(runtime->arm_target_app, 0, sizeof(runtime->arm_target_app));
	runtime->planned_sleep_ready = false;
	runtime->planned_sleep_wake_after_ms = 0;
	strncpy(runtime->start_reason, "boot", sizeof(runtime->start_reason) - 1);
	runtime->start_reason[sizeof(runtime->start_reason) - 1] = '\0';
	memset(runtime->return_stack, 0, sizeof(runtime->return_stack));
	runtime->return_stack_count = 0;
	memset(runtime->armed_timers, 0, sizeof(runtime->armed_timers));
	runtime->armed_timer_count = 0;
	runtime_clear_active_bindings(runtime);
	memset(runtime->outputs, 0, sizeof(runtime->outputs));
	runtime->output_count = 0;
	memset(runtime->drawlog, 0, sizeof(runtime->drawlog));
	runtime->drawlog_count = 0;
	memset(runtime->timers, 0, sizeof(runtime->timers));
	runtime->indicator_state = false;
	runtime->indicator_pattern = SQ_VM_RUNTIME_INDICATOR_STEADY;
	runtime->indicator_pattern_step = 0;
	runtime->indicator_pattern_on = false;
	runtime->indicator_pattern_on_ms = 0;
	runtime->indicator_pattern_off_ms = 0;
	runtime->indicator_pattern_next_ms = 0;
	(void)sq_vm_runtime_apply_target_default_indicator_binding(runtime);
	runtime->gpio_configured_mask = 0;
	runtime->gpio_state_mask = 0;
	memset(runtime->wifi_profile, 0, sizeof(runtime->wifi_profile));
	runtime->wifi_profile_len = 0;
	memset(runtime->wifi_profile_ssid, 0, sizeof(runtime->wifi_profile_ssid));
	runtime->wifi_profile_ssid_len = 0;
	memset(runtime->wifi_profile_password, 0, sizeof(runtime->wifi_profile_password));
	runtime->wifi_profile_password_len = 0;
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	memset(runtime->wifi_station_ip, 0, sizeof(runtime->wifi_station_ip));
	runtime->wifi_station_connect_status = 0;
	runtime->wifi_station_disconnect_status = 0;
	runtime->wifi_ap_active = false;
	runtime->wifi_ap_start_events = 0;
	runtime->wifi_ap_stop_events = 0;
#endif
	runtime->dispatch_exited = false;
	runtime->dispatch_sequence = 0;
	runtime->dispatch_start_cycles = 0;
	runtime->last_dispatch_sequence = 0;
	runtime->last_dispatch_elapsed_us = 0;
	runtime->last_dispatch_sqbc_read_count = 0;
	runtime->last_dispatch_sqbc_read_bytes = 0;
	runtime->dispatch_sqbc_read_count = 0;
	runtime->dispatch_sqbc_read_bytes = 0;
	runtime->dispatch_started = false;
	runtime->result_code = 0;
	runtime->status = SQ_VM_RUNTIME_IDLE;
}

void sq_vm_runtime_reset_vm_context(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL) {
		return;
	}
	(void)sqvm_context_reset_in_place(runtime->context_words, sizeof(runtime->context_words));
	clear_dispatch_transfer(runtime);
	runtime->context_ready = false;
}

void sq_vm_runtime_set_store_mount_point(struct sq_vm_runtime *runtime, const char *mount_point)
{
	if (runtime != NULL) {
		runtime->store_mount_point = mount_point;
	}
}

void sq_vm_runtime_set_registry(struct sq_vm_runtime *runtime, const struct sq_app_registry *registry)
{
	if (runtime != NULL) {
		runtime->registry = registry;
	}
}

const char *sq_vm_runtime_status_name(SqvmStatus status)
{
	switch (status) {
	case SQVM_STATUS_OK:
		return "ok";
	case SQVM_STATUS_INVALID_ARGUMENT:
		return "invalid_argument";
	case SQVM_STATUS_VM_ERROR:
		return "vm_error";
	default:
		return "unknown";
	}
}

int sq_vm_runtime_status_to_errno(SqvmStatus status)
{
	switch (status) {
	case SQVM_STATUS_OK:
		return 0;
	case SQVM_STATUS_INVALID_ARGUMENT:
		return -EINVAL;
	case SQVM_STATUS_VM_ERROR:
		return -EIO;
	default:
		return -EIO;
	}
}

static bool runtime_has_pending_storage(const struct sq_vm_runtime *runtime)
{
	return runtime != NULL && runtime->dispatch_started &&
	       runtime->result.outcome == SQVM_DISPATCH_PENDING_STORAGE;
}

static const SqvmCallbacks runtime_callbacks = {
#include "generated_runtime_callbacks.inc"
};

int sq_vm_runtime_dispatch(struct sq_vm_runtime *runtime,
			   const struct sq_vm_storage_backend *backend, const char *event)
{
	bool complete = false;
	int result;

	do {
		result = sq_vm_runtime_dispatch_slice(runtime, backend, event, SIZE_MAX, &complete);
		if (result != 0) {
			return result;
		}
	} while (!complete);
	return 0;
}

int sq_vm_runtime_dispatch_slice(struct sq_vm_runtime *runtime,
				 const struct sq_vm_storage_backend *backend, const char *event,
				 size_t storage_completion_budget, bool *complete)
{
	SqvmStatus status;
	size_t completed_storage = 0;

	if (runtime == NULL || backend == NULL || event == NULL || complete == NULL) {
		return -EINVAL;
	}
	*complete = false;
	if (sqvm_context_size() > sizeof(runtime->context_words)) {
		return -ENOMEM;
	}
	if (!runtime->dispatch_started) {
		runtime->dispatch_sqbc_read_count = 0;
		runtime->dispatch_sqbc_read_bytes = 0;
		runtime->dispatch_start_cycles = k_cycle_get_64();
		clear_dispatch_transfer(runtime);
		runtime->backend = backend;
		if (!runtime->context_ready) {
			status = sqvm_context_prepare(runtime->context_words,
						      sizeof(runtime->context_words));
			if (status != SQVM_STATUS_OK) {
				runtime_finish_dispatch_metrics(runtime, runtime->dispatch_start_cycles);
				return sq_vm_runtime_status_to_errno(status);
			}
			int transfer_result =
				sq_vm_runtime_transfer_acquire(runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH);
			if (transfer_result != 0) {
				runtime_finish_dispatch_metrics(runtime, runtime->dispatch_start_cycles);
				return transfer_result;
			}
			status = sqvm_context_init_in_place(runtime->context_words, runtime,
							    &runtime_callbacks,
							    runtime->transfer.init_scratch,
							    sizeof(runtime->transfer.init_scratch));
			transfer_result =
				sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH);
			if (transfer_result != 0) {
				runtime_finish_dispatch_metrics(runtime, runtime->dispatch_start_cycles);
				return transfer_result;
			}
			if (status != SQVM_STATUS_OK) {
				runtime_finish_dispatch_metrics(runtime, runtime->dispatch_start_cycles);
				return sq_vm_runtime_status_to_errno(status);
			}
			runtime->context_ready = true;
		}
		status = sqvm_dispatch_start_resumable(runtime->context_words, runtime,
						       &runtime_callbacks,
						       (const uint8_t *)event, strlen(event),
						       &runtime->result);
		if (status != SQVM_STATUS_OK) {
			runtime_finish_dispatch_metrics(runtime, runtime->dispatch_start_cycles);
			return sq_vm_runtime_status_to_errno(status);
		}
		runtime->dispatch_started = true;
	} else {
		runtime->backend = backend;
	}

	while (runtime->result.outcome == SQVM_DISPATCH_PENDING_STORAGE &&
	       completed_storage < storage_completion_budget) {
		int transfer_result =
			sq_vm_runtime_transfer_acquire(runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION);
		if (transfer_result != 0) {
			runtime_finish_dispatch_metrics(runtime, runtime->dispatch_start_cycles);
			return transfer_result;
		}
		int storage_result = sq_vm_storage_complete_request(backend, &runtime->result.storage,
								   &runtime->transfer.completion);
		transfer_result = sq_vm_runtime_transfer_release(runtime,
								 SQ_VM_RUNTIME_TRANSFER_COMPLETION);
		if (transfer_result != 0) {
			runtime_finish_dispatch_metrics(runtime, runtime->dispatch_start_cycles);
			return transfer_result;
		}
		if (storage_result != 0) {
			runtime_finish_dispatch_metrics(runtime, runtime->dispatch_start_cycles);
			return storage_result;
		}
		runtime_record_pending_sqbc_read(runtime, &runtime->result.storage,
						 &runtime->transfer.completion);
		completed_storage++;
		status = sqvm_dispatch_resume_storage(runtime->context_words, runtime,
						      &runtime_callbacks,
						      &runtime->transfer.completion,
						      &runtime->result);
		if (status != SQVM_STATUS_OK) {
			runtime_finish_dispatch_metrics(runtime, runtime->dispatch_start_cycles);
			return sq_vm_runtime_status_to_errno(status);
		}
	}

	if (runtime->result.outcome == SQVM_DISPATCH_PENDING_STORAGE) {
		return 0;
	}

	runtime->dispatch_exited = runtime->result.outcome == SQVM_DISPATCH_COMPLETE &&
				   runtime->result.exited;
	runtime->dispatch_started = false;
	runtime_finish_dispatch_metrics(runtime, runtime->dispatch_start_cycles);
	*complete = runtime->result.outcome == SQVM_DISPATCH_COMPLETE;
	return *complete ? 0 : -EIO;
}

int sq_vm_runtime_start(struct sq_vm_runtime *runtime,
			const struct sq_vm_storage_backend *backend, const char *event)
{
	if (event == NULL) {
		return -EINVAL;
	}
	return sq_vm_runtime_start_event(runtime, backend, (const uint8_t *)event, strlen(event));
}

int sq_vm_runtime_start_event(struct sq_vm_runtime *runtime,
			      const struct sq_vm_storage_backend *backend,
			      const uint8_t *event, size_t event_len)
{
	bool apply_bindings;

	if (runtime == NULL || backend == NULL || event == NULL) {
		return -EINVAL;
	}
	sq_vm_runtime_init(runtime);
	if (runtime->status == SQ_VM_RUNTIME_RUNNING) {
		return -EBUSY;
	}
	if (runtime->work_submitted) {
		int result = sq_vm_runtime_wait_idle(runtime, 250);
		if (result != 0) {
			return result;
		}
	}
	if (event_len == 0 || event_len >= sizeof(runtime->event)) {
		return -EINVAL;
	}

	runtime->job_backend = *backend;
	runtime->backend = &runtime->job_backend;
	apply_bindings = event_len == 9u && memcmp(event, "app.start", 9u) == 0;
	runtime->start_apply_bindings = apply_bindings;
	memmove(runtime->event, event, event_len);
	runtime->event[event_len] = '\0';
	runtime->result_code = 0;
	runtime->dispatch_exited = false;
	runtime->dispatch_started = false;
	runtime->status = SQ_VM_RUNTIME_RUNNING;
	int submit_result = sq_vm_runtime_submit_work(runtime);
	if (submit_result != 0) {
		runtime->start_apply_bindings = false;
		runtime->status = SQ_VM_RUNTIME_ERROR;
		runtime->result_code = submit_result;
		return submit_result;
	}
	return 0;
}

int sq_vm_runtime_record_output(struct sq_vm_runtime *runtime, const uint8_t *message,
				size_t message_len)
{
	if (runtime == NULL || (message == NULL && message_len > 0)) {
		return -EINVAL;
	}
	size_t slot = runtime->output_count;
	if (slot >= SQ_VM_RUNTIME_OUTPUT_MAX) {
		memmove(runtime->outputs[0], runtime->outputs[1],
			(SQ_VM_RUNTIME_OUTPUT_MAX - 1) * SQ_VM_RUNTIME_OUTPUT_LEN);
		slot = SQ_VM_RUNTIME_OUTPUT_MAX - 1;
		runtime->output_count = SQ_VM_RUNTIME_OUTPUT_MAX - 1;
	}
	size_t len = message_len;
	if (len >= SQ_VM_RUNTIME_OUTPUT_LEN) {
		len = SQ_VM_RUNTIME_OUTPUT_LEN - 1;
	}
	memcpy(runtime->outputs[slot], message, len);
	runtime->outputs[slot][len] = '\0';
	runtime->output_count++;
	return 0;
}

int sq_vm_runtime_record_drawlog(struct sq_vm_runtime *runtime, const char *line)
{
	if (runtime == NULL || line == NULL) {
		return -EINVAL;
	}
	size_t slot = runtime->drawlog_count;
	if (slot >= SQ_VM_RUNTIME_DRAWLOG_MAX) {
		memmove(runtime->drawlog[0], runtime->drawlog[1],
			(SQ_VM_RUNTIME_DRAWLOG_MAX - 1) * SQ_VM_RUNTIME_DRAWLOG_LEN);
		slot = SQ_VM_RUNTIME_DRAWLOG_MAX - 1;
		runtime->drawlog_count = SQ_VM_RUNTIME_DRAWLOG_MAX - 1;
	}
	size_t len = 0;
	while (len < SQ_VM_RUNTIME_DRAWLOG_LEN - 1 && line[len] != '\0') {
		len++;
	}
	memcpy(runtime->drawlog[slot], line, len);
	runtime->drawlog[slot][len] = '\0';
	runtime->drawlog_count++;
	return 0;
}

int sq_vm_runtime_record_device_error(struct sq_vm_runtime *runtime, const char *line)
{
	if (runtime == NULL || line == NULL) {
		return -EINVAL;
	}
	size_t slot = runtime->device_error_count;
	if (slot >= SQ_VM_RUNTIME_DEVICE_ERROR_MAX) {
		memmove(runtime->device_errors[0], runtime->device_errors[1],
			(SQ_VM_RUNTIME_DEVICE_ERROR_MAX - 1) * SQ_VM_RUNTIME_DEVICE_ERROR_LEN);
		slot = SQ_VM_RUNTIME_DEVICE_ERROR_MAX - 1;
		runtime->device_error_count = SQ_VM_RUNTIME_DEVICE_ERROR_MAX - 1;
	}
	size_t len = 0;
	while (len < SQ_VM_RUNTIME_DEVICE_ERROR_LEN - 1 && line[len] != '\0') {
		len++;
	}
	memcpy(runtime->device_errors[slot], line, len);
	runtime->device_errors[slot][len] = '\0';
	runtime->device_error_count++;
	return 0;
}

int sq_vm_runtime_poll(struct sq_vm_runtime *runtime)
{
	char event[SQ_VM_RUNTIME_EVENT_LEN];

	if (runtime == NULL) {
		return 0;
	}
	(void)sq_vm_runtime_poll_indicator(runtime);
	if (sq_vm_runtime_poll_input_buttons(runtime) != 0) {
		return -EIO;
	}
	if (runtime->status == SQ_VM_RUNTIME_RUNNING) {
		if (runtime->work_submitted &&
		    k_thread_join(&sq_vm_runtime_work_thread, K_NO_WAIT) == 0) {
			runtime->work_submitted = false;
		}
		if (!runtime->work_submitted && runtime_has_pending_storage(runtime)) {
			return sq_vm_runtime_submit_work(runtime);
		}
		return 0;
	}
	if (runtime->job_backend.read_sqbc == NULL) {
		return 0;
	}
	if (sq_vm_runtime_next_due_timer(runtime, event, sizeof(event)) != 0) {
		return 0;
	}
	return sq_vm_runtime_start(runtime, &runtime->job_backend, event);
}
