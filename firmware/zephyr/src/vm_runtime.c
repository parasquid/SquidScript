#include "vm_runtime_internal.h"
#include "vm_runtime_display_backend.h"
#include "xteink_x4_button_probe.h"
#include "sq_errno.h"

void sqvm_ffi_panic_abort(void)
{
	printk("sqvm ffi panic\n");
	k_panic();
	CODE_UNREACHABLE;
}

K_THREAD_STACK_DEFINE(sq_vm_runtime_work_stack, SQ_VM_RUNTIME_WORK_STACK_SIZE);
K_THREAD_STACK_DEFINE(sq_vm_runtime_display_work_stack, SQ_VM_RUNTIME_DISPLAY_WORK_STACK_SIZE);
static struct k_thread sq_vm_runtime_work_thread;
static struct k_thread sq_vm_runtime_display_work_thread;
static struct k_sem sq_vm_runtime_done_sem;
static struct k_mutex sq_vm_runtime_display_work_lock;
static struct sq_vm_runtime *sq_vm_runtime_active_work;
static bool sq_vm_runtime_work_sem_initialized;
static bool sq_vm_runtime_display_work_lock_initialized;
static bool sq_vm_runtime_work_thread_started;
static bool sq_vm_runtime_display_work_thread_started;

struct sq_vm_runtime_display_flush_job {
	struct sq_vm_runtime *runtime;
	struct sq_vm_runtime_display_op ops[SQ_VM_RUNTIME_DISPLAY_OP_MAX];
	uint8_t op_count;
	enum sq_vm_runtime_display_refresh_mode refresh_mode;
};

static struct sq_vm_runtime_display_flush_job sq_vm_runtime_display_active_job;
static struct sq_vm_runtime_display_flush_job sq_vm_runtime_display_pending_job;
static bool sq_vm_runtime_display_active;
static bool sq_vm_runtime_display_pending;

static bool runtime_has_pending_storage(const struct sq_vm_runtime *runtime);

enum sq_vm_runtime_cap_kind {
	SQ_VM_RUNTIME_CAP_TIMER = 0,
	SQ_VM_RUNTIME_CAP_ARMED_TIMER,
	SQ_VM_RUNTIME_CAP_INPUT_BUTTON,
	SQ_VM_RUNTIME_CAP_BINDING,
	SQ_VM_RUNTIME_CAP_OUTPUT,
	SQ_VM_RUNTIME_CAP_DRAWLOG,
};

struct sq_vm_runtime_cap_def {
	const char *key;
	enum sq_vm_runtime_cap_kind kind;
	uint8_t hard_max;
};

static const struct sq_vm_runtime_cap_def runtime_cap_defs[] = {
	{"vm_runtime.timer_max", SQ_VM_RUNTIME_CAP_TIMER, SQ_VM_RUNTIME_TIMER_MAX},
	{"vm_runtime.armed_timer_max", SQ_VM_RUNTIME_CAP_ARMED_TIMER,
	 SQ_VM_RUNTIME_ARMED_TIMER_MAX},
	{"vm_runtime.input_button_max", SQ_VM_RUNTIME_CAP_INPUT_BUTTON,
	 SQ_VM_RUNTIME_INPUT_BUTTON_MAX},
	{"vm_runtime.active_binding_max", SQ_VM_RUNTIME_CAP_BINDING,
	 SQ_VM_RUNTIME_ACTIVE_BINDING_MAX},
	{"vm_runtime.output_max", SQ_VM_RUNTIME_CAP_OUTPUT, SQ_VM_RUNTIME_OUTPUT_MAX},
	{"vm_runtime.drawlog_max", SQ_VM_RUNTIME_CAP_DRAWLOG, SQ_VM_RUNTIME_DRAWLOG_MAX},
};

static void runtime_active_caps_set_hard_max(struct sq_vm_runtime *runtime)
{
	runtime->active_timer_max = SQ_VM_RUNTIME_TIMER_MAX;
	runtime->active_armed_timer_max = SQ_VM_RUNTIME_ARMED_TIMER_MAX;
	runtime->active_input_button_max = SQ_VM_RUNTIME_INPUT_BUTTON_MAX;
	runtime->active_binding_max = SQ_VM_RUNTIME_ACTIVE_BINDING_MAX;
	runtime->active_output_max = SQ_VM_RUNTIME_OUTPUT_MAX;
	runtime->active_drawlog_max = SQ_VM_RUNTIME_DRAWLOG_MAX;
}

static const struct sq_vm_runtime_cap_def *runtime_cap_def_for_key(const char *key)
{
	if (key == NULL) {
		return NULL;
	}
	for (size_t i = 0; i < ARRAY_SIZE(runtime_cap_defs); i++) {
		if (strcmp(runtime_cap_defs[i].key, key) == 0) {
			return &runtime_cap_defs[i];
		}
	}
	return NULL;
}

static uint8_t *runtime_cap_active_slot(struct sq_vm_runtime *runtime,
					const struct sq_vm_runtime_cap_def *def)
{
	if (runtime == NULL || def == NULL) {
		return NULL;
	}
	switch (def->kind) {
	case SQ_VM_RUNTIME_CAP_TIMER:
		return &runtime->active_timer_max;
	case SQ_VM_RUNTIME_CAP_ARMED_TIMER:
		return &runtime->active_armed_timer_max;
	case SQ_VM_RUNTIME_CAP_INPUT_BUTTON:
		return &runtime->active_input_button_max;
	case SQ_VM_RUNTIME_CAP_BINDING:
		return &runtime->active_binding_max;
	case SQ_VM_RUNTIME_CAP_OUTPUT:
		return &runtime->active_output_max;
	case SQ_VM_RUNTIME_CAP_DRAWLOG:
		return &runtime->active_drawlog_max;
	}
	return NULL;
}

static const uint8_t *runtime_cap_active_slot_const(const struct sq_vm_runtime *runtime,
						    const struct sq_vm_runtime_cap_def *def)
{
	if (runtime == NULL || def == NULL) {
		return NULL;
	}
	switch (def->kind) {
	case SQ_VM_RUNTIME_CAP_TIMER:
		return &runtime->active_timer_max;
	case SQ_VM_RUNTIME_CAP_ARMED_TIMER:
		return &runtime->active_armed_timer_max;
	case SQ_VM_RUNTIME_CAP_INPUT_BUTTON:
		return &runtime->active_input_button_max;
	case SQ_VM_RUNTIME_CAP_BINDING:
		return &runtime->active_binding_max;
	case SQ_VM_RUNTIME_CAP_OUTPUT:
		return &runtime->active_output_max;
	case SQ_VM_RUNTIME_CAP_DRAWLOG:
		return &runtime->active_drawlog_max;
	}
	return NULL;
}

static uint8_t runtime_cap_current_usage(const struct sq_vm_runtime *runtime,
					 const struct sq_vm_runtime_cap_def *def)
{
	if (runtime == NULL || def == NULL) {
		return 0;
	}
	switch (def->kind) {
	case SQ_VM_RUNTIME_CAP_TIMER: {
		uint8_t count = 0;

		for (size_t i = 0; i < SQ_VM_RUNTIME_TIMER_MAX; i++) {
			if (runtime->timers[i].active) {
				count++;
			}
		}
		return count;
	}
	case SQ_VM_RUNTIME_CAP_ARMED_TIMER:
		return runtime->armed_timer_count;
	case SQ_VM_RUNTIME_CAP_INPUT_BUTTON:
		return runtime->input_button_count;
	case SQ_VM_RUNTIME_CAP_BINDING:
		return runtime->active_binding_count;
	case SQ_VM_RUNTIME_CAP_OUTPUT:
		return runtime->output_count;
	case SQ_VM_RUNTIME_CAP_DRAWLOG:
		return runtime->drawlog_count;
	}
	return 0;
}

int sq_vm_runtime_cap_get(const struct sq_vm_runtime *runtime, const char *key, uint16_t *out)
{
	const struct sq_vm_runtime_cap_def *def = runtime_cap_def_for_key(key);
	const uint8_t *slot;

	if (runtime == NULL || out == NULL || def == NULL) {
		return -EINVAL;
	}
	slot = runtime_cap_active_slot_const(runtime, def);
	if (slot == NULL) {
		return -EINVAL;
	}
	*out = *slot == 0 ? def->hard_max : *slot;
	return 0;
}

int sq_vm_runtime_cap_set(struct sq_vm_runtime *runtime, const char *key, uint16_t value)
{
	const struct sq_vm_runtime_cap_def *def = runtime_cap_def_for_key(key);
	uint8_t *slot;

	if (runtime == NULL || def == NULL) {
		return -EINVAL;
	}
	if (value == 0 || value > def->hard_max || value > UINT8_MAX) {
		return -ERANGE;
	}
	if (runtime_cap_current_usage(runtime, def) > value) {
		return -EBUSY;
	}
	slot = runtime_cap_active_slot(runtime, def);
	if (slot == NULL) {
		return -EINVAL;
	}
	*slot = (uint8_t)value;
	return 0;
}

int sq_vm_runtime_cap_clear(struct sq_vm_runtime *runtime, const char *key)
{
	if (runtime == NULL) {
		return -EINVAL;
	}
	if (key == NULL || key[0] == '\0') {
		runtime_active_caps_set_hard_max(runtime);
		return 0;
	}
	const struct sq_vm_runtime_cap_def *def = runtime_cap_def_for_key(key);
	uint8_t *slot;

	if (def == NULL) {
		return -EINVAL;
	}
	slot = runtime_cap_active_slot(runtime, def);
	if (slot == NULL) {
		return -EINVAL;
	}
	*slot = def->hard_max;
	return 0;
}

static int runtime_cap_read_file(const char *path, uint8_t *buffer, size_t buffer_len,
				 size_t *out_len)
{
	struct fs_dirent entry;
	struct fs_file_t file;
	int result;

	if (path == NULL || buffer == NULL || out_len == NULL) {
		return -EINVAL;
	}
	*out_len = 0;
	result = fs_stat(path, &entry);
	if (result != 0) {
		return result;
	}
	if (entry.type != FS_DIR_ENTRY_FILE || entry.size > buffer_len) {
		return -EINVAL;
	}

	fs_file_t_init(&file);
	result = fs_open(&file, path, FS_O_READ);
	if (result != 0) {
		return result;
	}
	ssize_t bytes_read = fs_read(&file, buffer, entry.size);
	result = fs_close(&file);
	if (bytes_read < 0) {
		return (int)bytes_read;
	}
	if ((size_t)bytes_read != entry.size) {
		return -EIO;
	}
	*out_len = bytes_read;
	return result;
}

static int runtime_cap_write_file(const char *path, const uint8_t *bytes, size_t len)
{
	struct fs_file_t file;
	int result;

	if (path == NULL || bytes == NULL) {
		return -EINVAL;
	}

	fs_file_t_init(&file);
	result = fs_open(&file, path, FS_O_CREATE | FS_O_WRITE | FS_O_TRUNC);
	if (result != 0) {
		return result;
	}
	ssize_t written = fs_write(&file, bytes, len);
	result = fs_close(&file);
	if (written < 0) {
		return (int)written;
	}
	if ((size_t)written != len) {
		return -EIO;
	}
	return result;
}

static int runtime_cap_apply_record(struct sq_vm_runtime *runtime, const SqdcRecord *record)
{
	char key[SQDC_CONFIG_KEY_CAP];

	if (runtime == NULL || record == NULL || !record->present ||
	    record->key_len == 0 || record->key_len >= sizeof(key) ||
	    record->value.kind != SQDC_VALUE_I32 || record->value.i32_value <= 0 ||
	    record->value.i32_value > UINT16_MAX) {
		return -EINVAL;
	}
	memcpy(key, record->key, record->key_len);
	key[record->key_len] = '\0';
	return sq_vm_runtime_cap_set(runtime, key, (uint16_t)record->value.i32_value);
}

int sq_vm_runtime_cap_load(struct sq_vm_runtime *runtime)
{
	char path[SQ_APP_STORE_RUNTIME_CONFIG_PATH_MAX];
	SqdcConfig config = {0};
	size_t bytes_len = 0;
	SqdcStatus status;
	int result;

	if (runtime == NULL) {
		return -EINVAL;
	}
	runtime_active_caps_set_hard_max(runtime);
	if (runtime->store_mount_point == NULL) {
		return 0;
	}
	result = sq_app_store_runtime_config_path(runtime->store_mount_point, path, sizeof(path));
	if (result != 0) {
		return result;
	}
	result = sq_vm_runtime_transfer_acquire(runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION);
	if (result != 0) {
		return result;
	}
	result = runtime_cap_read_file(path, runtime->transfer.completion.bytes,
				       sizeof(runtime->transfer.completion.bytes), &bytes_len);
	if (result == -ENOENT) {
		(void)sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION);
		return 0;
	}
	if (result != 0) {
		(void)sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION);
		return result;
	}
	status = sqdc_decode_sqdc(runtime->transfer.completion.bytes, bytes_len, &config);
	result = sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION);
	if (result != 0) {
		return result;
	}
	if (status != SQDC_STATUS_OK) {
		return -EINVAL;
	}
	for (size_t i = 0; i < config.count; i++) {
		result = runtime_cap_apply_record(runtime, &config.records[i]);
		if (result != 0) {
			runtime_active_caps_set_hard_max(runtime);
			return result;
		}
	}
	return 0;
}

int sq_vm_runtime_cap_save(struct sq_vm_runtime *runtime)
{
	char path[SQ_APP_STORE_RUNTIME_CONFIG_PATH_MAX];
	SqdcConfig config = {0};
	size_t encoded_len = 0;
	SqdcStatus status;
	int result;

	if (runtime == NULL) {
		return -EINVAL;
	}
	if (runtime->store_mount_point == NULL) {
		return -ENODEV;
	}
	result = sq_app_store_prepare_filesystem(runtime->store_mount_point);
	if (result != 0) {
		return result;
	}
	result = sq_app_store_runtime_config_path(runtime->store_mount_point, path, sizeof(path));
	if (result != 0) {
		return result;
	}
	status = sqdc_config_clear(&config);
	if (status != SQDC_STATUS_OK) {
		return -EINVAL;
	}
	for (size_t i = 0; i < ARRAY_SIZE(runtime_cap_defs); i++) {
		uint16_t value = 0;

		result = sq_vm_runtime_cap_get(runtime, runtime_cap_defs[i].key, &value);
		if (result != 0) {
			return result;
		}
		if (value == runtime_cap_defs[i].hard_max) {
			continue;
		}
		status = sqdc_config_set_i32(&config, (const uint8_t *)runtime_cap_defs[i].key,
					     strlen(runtime_cap_defs[i].key), value);
		if (status != SQDC_STATUS_OK) {
			return -EINVAL;
		}
	}
	if (config.count == 0) {
		result = fs_unlink(path);
		return result == -ENOENT ? 0 : result;
	}
	result = sq_vm_runtime_transfer_acquire(runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION);
	if (result != 0) {
		return result;
	}
	status = sqdc_encode_sqdc(&config, runtime->transfer.completion.bytes,
				  sizeof(runtime->transfer.completion.bytes), &encoded_len);
	if (status != SQDC_STATUS_OK) {
		(void)sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION);
		return -EINVAL;
	}
	result = runtime_cap_write_file(path, runtime->transfer.completion.bytes, encoded_len);
	int release_result = sq_vm_runtime_transfer_release(runtime,
							    SQ_VM_RUNTIME_TRANSFER_COMPLETION);
	return result != 0 ? result : release_result;
}

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

static void runtime_display_record_flush_error(struct sq_vm_runtime *runtime, int result)
{
	if (runtime == NULL || result == 0) {
		return;
	}
	char line[SQ_VM_RUNTIME_DEVICE_ERROR_LEN];
	int n = snprintf(line, sizeof(line), "display=flush code=%d (%s)", result,
			 sq_errno_name(result));
	if (n > 0 && (size_t)n < sizeof(line)) {
		(void)sq_vm_runtime_record_device_error(runtime, line);
	}
}

static void runtime_display_copy_flush_job(struct sq_vm_runtime_display_flush_job *job,
					   struct sq_vm_runtime *runtime)
{
	memset(job, 0, sizeof(*job));
	job->runtime = runtime;
	job->op_count = runtime->display_op_count;
	job->refresh_mode = runtime->display_refresh_mode;
	memcpy(job->ops, runtime->display_ops,
	       runtime->display_op_count * sizeof(runtime->display_ops[0]));
}

static void runtime_display_flush_worker(void *arg1, void *arg2, void *arg3)
{
	ARG_UNUSED(arg1);
	ARG_UNUSED(arg2);
	ARG_UNUSED(arg3);

	while (true) {
		int result = sq_display_backend_flush(sq_vm_runtime_display_active_job.ops,
						      sq_vm_runtime_display_active_job.op_count,
						      sq_vm_runtime_display_active_job.refresh_mode);
		runtime_display_record_flush_error(sq_vm_runtime_display_active_job.runtime, result);

		k_mutex_lock(&sq_vm_runtime_display_work_lock, K_FOREVER);
		if (sq_vm_runtime_display_pending) {
			sq_vm_runtime_display_active_job = sq_vm_runtime_display_pending_job;
			memset(&sq_vm_runtime_display_pending_job, 0,
			       sizeof(sq_vm_runtime_display_pending_job));
			sq_vm_runtime_display_pending = false;
			k_mutex_unlock(&sq_vm_runtime_display_work_lock);
			continue;
		}
		memset(&sq_vm_runtime_display_active_job, 0, sizeof(sq_vm_runtime_display_active_job));
		sq_vm_runtime_display_active = false;
		k_mutex_unlock(&sq_vm_runtime_display_work_lock);
		return;
	}
}

static void runtime_display_work_init(void)
{
	if (!sq_vm_runtime_display_work_lock_initialized) {
		k_mutex_init(&sq_vm_runtime_display_work_lock);
		sq_vm_runtime_display_work_lock_initialized = true;
	}
}

static void runtime_flush_display_if_dirty(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL || !runtime->display_dirty || runtime->display_op_count == 0) {
		return;
	}
	runtime_display_work_init();
	k_mutex_lock(&sq_vm_runtime_display_work_lock, K_FOREVER);
	if (sq_vm_runtime_display_active) {
		runtime_display_copy_flush_job(&sq_vm_runtime_display_pending_job, runtime);
		sq_vm_runtime_display_pending = true;
		k_mutex_unlock(&sq_vm_runtime_display_work_lock);
	} else {
		if (sq_vm_runtime_display_work_thread_started) {
			(void)k_thread_join(&sq_vm_runtime_display_work_thread, K_NO_WAIT);
		}
		runtime_display_copy_flush_job(&sq_vm_runtime_display_active_job, runtime);
		sq_vm_runtime_display_active = true;
		k_mutex_unlock(&sq_vm_runtime_display_work_lock);
		k_thread_create(&sq_vm_runtime_display_work_thread, sq_vm_runtime_display_work_stack,
				K_THREAD_STACK_SIZEOF(sq_vm_runtime_display_work_stack),
				runtime_display_flush_worker, NULL, NULL, NULL, 5, 0, K_NO_WAIT);
		k_thread_name_set(&sq_vm_runtime_display_work_thread, "sq_vm_display");
		sq_vm_runtime_display_work_thread_started = true;
	}
	memset(runtime->display_ops, 0, sizeof(runtime->display_ops));
	runtime->display_op_count = 0;
	runtime->display_refresh_mode = SQ_VM_RUNTIME_DISPLAY_REFRESH_AUTO;
	runtime->display_dirty = false;
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
	runtime_active_caps_set_hard_max(runtime);
	(void)sq_vm_runtime_cap_load(runtime);
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
	sq_vm_runtime_wifi_reset_target(runtime);
	sq_vm_runtime_reset_vm_context(runtime);
	memset(&runtime->job_backend, 0, sizeof(runtime->job_backend));
	memset(runtime->event, 0, sizeof(runtime->event));
	memset(runtime->traces, 0, sizeof(runtime->traces));
	runtime->trace_count = 0;
	memset(runtime->current_app, 0, sizeof(runtime->current_app));
	runtime->current_app_temp = false;
	runtime->start_apply_bindings = false;
	runtime->lifecycle_phase = SQ_VM_RUNTIME_LIFECYCLE_IDLE;
	memset(runtime->lifecycle_target_app, 0, sizeof(runtime->lifecycle_target_app));
	runtime->lifecycle_target_temp = false;
	memset(runtime->lifecycle_previous_app, 0, sizeof(runtime->lifecycle_previous_app));
	runtime->lifecycle_previous_app_temp = false;
	runtime->arm_phase = SQ_VM_RUNTIME_ARM_IDLE;
	memset(runtime->arm_target_app, 0, sizeof(runtime->arm_target_app));
	runtime->planned_sleep_ready = false;
	runtime->planned_sleep_wake_after_ms = 0;
	strncpy(runtime->start_reason, "boot", sizeof(runtime->start_reason) - 1);
	runtime->start_reason[sizeof(runtime->start_reason) - 1] = '\0';
	memset(runtime->return_stack, 0, sizeof(runtime->return_stack));
	memset(runtime->return_stack_temp, 0, sizeof(runtime->return_stack_temp));
	runtime->return_stack_count = 0;
	memset(runtime->armed_timers, 0, sizeof(runtime->armed_timers));
	runtime->armed_timer_count = 0;
	runtime_clear_active_bindings(runtime);
	memset(runtime->target_adc_buttons, 0, sizeof(runtime->target_adc_buttons));
	runtime->target_adc_button_next_poll_ms = 0;
	memset(&runtime->input_event_queue, 0, sizeof(runtime->input_event_queue));
	memset(runtime->outputs, 0, sizeof(runtime->outputs));
	runtime->output_count = 0;
	memset(runtime->device_errors, 0, sizeof(runtime->device_errors));
	runtime->device_error_count = 0;
	memset(runtime->drawlog, 0, sizeof(runtime->drawlog));
	runtime->drawlog_count = 0;
	memset(runtime->display_ops, 0, sizeof(runtime->display_ops));
	runtime->display_op_count = 0;
	runtime->display_dirty = false;
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
	runtime->wifi_ap_clients = 0;
	runtime->wifi_ap_sta_connected_events = 0;
	runtime->wifi_ap_sta_disconnected_events = 0;
	runtime->wifi_service_state = SQ_VM_RUNTIME_WIFI_SERVICE_IDLE;
	runtime->wifi_op_kind = SQ_VM_RUNTIME_WIFI_OP_NONE;
	runtime->wifi_op_active = false;
	runtime->wifi_op_done = false;
	runtime->wifi_op_cancelled = false;
	runtime->wifi_op_ok = false;
	runtime->wifi_op_error = NULL;
	runtime->wifi_op_deadline_ms = 0;
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
		if (runtime->work_initialized) {
			(void)sq_vm_runtime_cap_load(runtime);
		}
	}
}

void sq_vm_runtime_set_registry(struct sq_vm_runtime *runtime, const struct sq_app_registry *registry)
{
	if (runtime != NULL) {
		runtime->registry = registry;
	}
}

void sq_vm_runtime_set_mutable_registry(struct sq_vm_runtime *runtime,
					struct sq_app_registry *registry)
{
	if (runtime != NULL) {
		runtime->mutable_registry = registry;
	}
}

int sq_vm_runtime_request_install(struct sq_vm_runtime *runtime, const char *app_id,
				  const char *file_ref)
{
	if (runtime == NULL || app_id == NULL || file_ref == NULL ||
	    strlen(app_id) >= sizeof(runtime->pending_install.app_id) ||
	    strlen(file_ref) >= sizeof(runtime->pending_install.file_ref)) {
		return -EINVAL;
	}
	strcpy(runtime->pending_install.app_id, app_id);
	strcpy(runtime->pending_install.file_ref, file_ref);
	runtime->pending_install.active = true;
	return 0;
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
		if (runtime->pending_event_payload != NULL &&
		    runtime->pending_event_payload_count > 0) {
			status = sqvm_dispatch_start_resumable_with_payload(
				runtime->context_words, runtime, &runtime_callbacks,
				(const uint8_t *)event, strlen(event),
				runtime->pending_event_payload,
				runtime->pending_event_payload_count, &runtime->result);
		} else {
			status = sqvm_dispatch_start_resumable(runtime->context_words, runtime,
							       &runtime_callbacks,
							       (const uint8_t *)event, strlen(event),
							       &runtime->result);
		}
		/* One-shot: the payload applies only to this start dispatch. */
		runtime->pending_event_payload = NULL;
		runtime->pending_event_payload_count = 0;
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
	if (runtime->result.outcome == SQVM_DISPATCH_COMPLETE) {
		runtime_flush_display_if_dirty(runtime);
	}
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

void sq_vm_runtime_set_pending_event_payload(struct sq_vm_runtime *runtime,
					     const SqvmEventPayloadField *fields, size_t count)
{
	if (runtime == NULL) {
		return;
	}
	if (fields == NULL || count == 0) {
		runtime->pending_event_payload = NULL;
		runtime->pending_event_payload_count = 0;
		return;
	}
	runtime->pending_event_payload = fields;
	runtime->pending_event_payload_count = count;
}

int sq_vm_runtime_queue_input_event(struct sq_vm_runtime *runtime, const char *event)
{
	size_t event_len;
	size_t slot;

	if (runtime == NULL || event == NULL) {
		return -EINVAL;
	}
	event_len = strlen(event);
	if (event_len == 0 || event_len >= SQ_VM_RUNTIME_EVENT_LEN) {
		return -EINVAL;
	}
	if (runtime->input_event_queue.count >= SQ_VM_RUNTIME_INPUT_EVENT_QUEUE_MAX) {
		(void)sq_vm_runtime_record_device_error(runtime, "input_queue_overflow");
		return -ENOSPC;
	}
	slot = (runtime->input_event_queue.head + runtime->input_event_queue.count) %
	       SQ_VM_RUNTIME_INPUT_EVENT_QUEUE_MAX;
	memset(runtime->input_event_queue.events[slot], 0, SQ_VM_RUNTIME_EVENT_LEN);
	memcpy(runtime->input_event_queue.events[slot], event, event_len);
	runtime->input_event_queue.count++;
	return 0;
}

int sq_vm_runtime_drain_input_event(struct sq_vm_runtime *runtime, char *out, size_t out_cap)
{
	if (runtime == NULL || out == NULL || out_cap == 0) {
		return -EINVAL;
	}
	if (runtime->input_event_queue.count == 0) {
		return -ENOENT;
	}
	if (out_cap <= strlen(runtime->input_event_queue.events[runtime->input_event_queue.head])) {
		return -ENOSPC;
	}
	strncpy(out, runtime->input_event_queue.events[runtime->input_event_queue.head], out_cap - 1);
	out[out_cap - 1] = '\0';
	memset(runtime->input_event_queue.events[runtime->input_event_queue.head], 0,
	       SQ_VM_RUNTIME_EVENT_LEN);
	runtime->input_event_queue.head =
		(runtime->input_event_queue.head + 1) % SQ_VM_RUNTIME_INPUT_EVENT_QUEUE_MAX;
	runtime->input_event_queue.count--;
	if (runtime->input_event_queue.count == 0) {
		runtime->input_event_queue.head = 0;
	}
	return 0;
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
	size_t active_max = runtime->active_output_max == 0 ? SQ_VM_RUNTIME_OUTPUT_MAX :
							runtime->active_output_max;
	if (slot >= active_max) {
		memmove(runtime->outputs[0], runtime->outputs[1],
			(active_max - 1) * SQ_VM_RUNTIME_OUTPUT_LEN);
		slot = active_max - 1;
		runtime->output_count = active_max - 1;
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
	size_t active_max = runtime->active_drawlog_max == 0 ? SQ_VM_RUNTIME_DRAWLOG_MAX :
							 runtime->active_drawlog_max;
	if (slot >= active_max) {
		memmove(runtime->drawlog[0], runtime->drawlog[1],
			(active_max - 1) * SQ_VM_RUNTIME_DRAWLOG_LEN);
		slot = active_max - 1;
		runtime->drawlog_count = active_max - 1;
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

static int runtime_invariant_line(char *line, size_t line_len, const char *name, int code)
{
	int written;

	if (line == NULL || line_len == 0 || name == NULL) {
		return -EINVAL;
	}
	written = snprintf(line, line_len, "invariant.runtime.%s code=%d (%s)", name, code,
			   sq_errno_name(code));
	if (written < 0 || (size_t)written >= line_len) {
		return -ENOSPC;
	}
	return code;
}

static size_t runtime_active_timer_cap(const struct sq_vm_runtime *runtime)
{
	return runtime->active_timer_max == 0 ? SQ_VM_RUNTIME_TIMER_MAX : runtime->active_timer_max;
}

static size_t runtime_active_armed_timer_cap(const struct sq_vm_runtime *runtime)
{
	return runtime->active_armed_timer_max == 0 ? SQ_VM_RUNTIME_ARMED_TIMER_MAX :
						      runtime->active_armed_timer_max;
}

static size_t runtime_active_binding_cap(const struct sq_vm_runtime *runtime)
{
	return runtime->active_binding_max == 0 ? SQ_VM_RUNTIME_ACTIVE_BINDING_MAX :
						  runtime->active_binding_max;
}

int sq_vm_runtime_validate_invariants(const struct sq_vm_runtime *runtime, char *line,
				      size_t line_len)
{
	size_t active_max;
	size_t active_count;

	if (runtime == NULL) {
		return -EINVAL;
	}
	if (runtime->current_app_temp && runtime->current_app[0] == '\0') {
		return runtime_invariant_line(line, line_len, "current_app", -EINVAL);
	}
	if (runtime->return_stack_count > SQ_VM_RUNTIME_RETURN_STACK_MAX) {
		return runtime_invariant_line(line, line_len, "return_stack", -EINVAL);
	}
	for (uint8_t i = 0; i < runtime->return_stack_count; i++) {
		if (runtime->return_stack[i][0] == '\0') {
			return runtime_invariant_line(line, line_len, "return_stack", -EINVAL);
		}
	}

	active_max = runtime_active_timer_cap(runtime);
	if (active_max > SQ_VM_RUNTIME_TIMER_MAX) {
		return runtime_invariant_line(line, line_len, "timer_cap", -EINVAL);
	}
	for (size_t i = 0; i < active_max; i++) {
		if (!runtime->timers[i].active) {
			continue;
		}
		if (runtime->timers[i].event[0] == '\0') {
			return runtime_invariant_line(line, line_len, "timer", -EINVAL);
		}
		for (size_t j = i + 1; j < active_max; j++) {
			if (runtime->timers[j].active &&
			    strcmp(runtime->timers[i].event, runtime->timers[j].event) == 0) {
				return runtime_invariant_line(line, line_len, "timer_dup",
							      -EEXIST);
			}
		}
	}

	active_max = runtime_active_armed_timer_cap(runtime);
	if (active_max > SQ_VM_RUNTIME_ARMED_TIMER_MAX ||
	    runtime->armed_timer_count > SQ_VM_RUNTIME_ARMED_TIMER_MAX) {
		return runtime_invariant_line(line, line_len, "armed_timer", -EINVAL);
	}
	active_count = 0;
	for (size_t i = 0; i < active_max; i++) {
		const struct sq_vm_runtime_armed_timer *timer = &runtime->armed_timers[i];

		if (!timer->active) {
			continue;
		}
		active_count++;
		if (timer->app_id[0] == '\0' || timer->event[0] == '\0') {
			return runtime_invariant_line(line, line_len, "armed_timer", -EINVAL);
		}
		for (size_t j = i + 1; j < active_max; j++) {
			const struct sq_vm_runtime_armed_timer *other = &runtime->armed_timers[j];

			if (other->active && strcmp(timer->app_id, other->app_id) == 0 &&
			    strcmp(timer->event, other->event) == 0) {
				return runtime_invariant_line(line, line_len, "armed_dup",
							      -EEXIST);
			}
		}
	}
	if (active_count != runtime->armed_timer_count) {
		return runtime_invariant_line(line, line_len, "armed_count", -EINVAL);
	}

	active_max = runtime_active_binding_cap(runtime);
	if (active_max > SQ_VM_RUNTIME_ACTIVE_BINDING_MAX ||
	    runtime->active_binding_count > SQ_VM_RUNTIME_ACTIVE_BINDING_MAX) {
		return runtime_invariant_line(line, line_len, "binding", -EINVAL);
	}
	active_count = 0;
	for (size_t i = 0; i < active_max; i++) {
		const struct sq_vm_runtime_active_binding *binding = &runtime->active_bindings[i];

		if (!binding->active) {
			continue;
		}
		active_count++;
		if (binding->alias[0] == '\0') {
			return runtime_invariant_line(line, line_len, "binding", -EINVAL);
		}
		for (size_t j = i + 1; j < active_max; j++) {
			if (runtime->active_bindings[j].active &&
			    strcmp(binding->alias, runtime->active_bindings[j].alias) == 0) {
				return runtime_invariant_line(line, line_len, "binding_dup",
							      -EEXIST);
			}
		}
	}
	if (active_count != runtime->active_binding_count) {
		return runtime_invariant_line(line, line_len, "binding_count", -EINVAL);
	}

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
	if (sq_x4_button_probe_poll_runtime(runtime) != 0) {
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
	if (sq_vm_runtime_drain_input_event(runtime, event, sizeof(event)) == 0) {
		return sq_vm_runtime_start(runtime, &runtime->job_backend, event);
	}
	if (sq_vm_runtime_next_due_timer(runtime, event, sizeof(event)) != 0) {
		return 0;
	}
	return sq_vm_runtime_start(runtime, &runtime->job_backend, event);
}
