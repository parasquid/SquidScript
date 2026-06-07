#include "vm_runtime_internal.h"

#include "app_store.h"
#include "ble_profile_table.h"
#include "sq_errno.h"

static int32_t runtime_app_lifecycle(void *user_data, const char *action, const uint8_t *app,
				     size_t app_len)
{
	struct sq_vm_runtime *runtime = user_data;
	char line[SQ_VM_RUNTIME_TRACE_LEN];

	if (runtime == NULL || action == NULL || (app == NULL && app_len > 0)) {
		return -EINVAL;
	}
	int written = snprintf(line, sizeof(line), "app.%s %.*s", action, (int)app_len,
			       app == NULL ? (const uint8_t *)"" : app);
	if (written > 0) {
		(void)sq_vm_runtime_record_trace(runtime, (const uint8_t *)line, strlen(line));
	}
	return 0;
}

int32_t runtime_app_launch(void *user_data, const uint8_t *app, size_t app_len)
{
	struct sq_vm_runtime *runtime = user_data;
	int result = runtime_app_lifecycle(user_data, "launch", app, app_len);

	if (result != 0) {
		return result;
	}
	return sq_app_lifecycle_request_launch(runtime, app, app_len);
}

int32_t runtime_app_arm(void *user_data, const uint8_t *app, size_t app_len)
{
	struct sq_vm_runtime *runtime = user_data;
	int result = runtime_app_lifecycle(user_data, "arm", app, app_len);

	if (result != 0) {
		return result;
	}
	return sq_app_lifecycle_request_arm(runtime, app, app_len);
}

int32_t runtime_app_disarm(void *user_data, const uint8_t *app, size_t app_len)
{
	int result = runtime_app_lifecycle(user_data, "disarm", app, app_len);

	if (result != 0) {
		return result;
	}
	struct sq_vm_runtime *runtime = user_data;
	if (runtime != NULL && app != NULL) {
		result = sq_app_lifecycle_cancel_pending_arm(runtime, app, app_len);
		if (result != 0) {
			return result;
		}
	}
	/* Drop this app's BLE profile routes alongside its armed timers. */
	if (app != NULL && app_len < SQ_APP_STORE_APP_ID_MAX) {
		char app_id[SQ_APP_STORE_APP_ID_MAX];

		memcpy(app_id, app, app_len);
		app_id[app_len] = '\0';
		sq_ble_profile_table_remove_app(app_id);
	}
	return sq_vm_runtime_clear_armed_app(user_data, app, app_len);
}

int32_t runtime_app_install_file(void *user_data, const uint8_t *file_ref, size_t file_ref_len,
				 const uint8_t *app_id, size_t app_id_len)
{
	struct sq_vm_runtime *runtime = user_data;
	char app_id_buf[SQ_APP_STORE_APP_ID_MAX];
	char file_path_buf[SQ_APP_STORE_PATH_MAX];
	int trace_result;
	int result;

	if (runtime == NULL || file_ref == NULL || app_id == NULL || file_ref_len == 0 ||
	    app_id_len == 0 || app_id_len >= sizeof(app_id_buf) ||
	    file_ref_len >= sizeof(file_path_buf)) {
		return -EINVAL;
	}

	memcpy(app_id_buf, app_id, app_id_len);
	app_id_buf[app_id_len] = '\0';
	memcpy(file_path_buf, file_ref, file_ref_len);
	file_path_buf[file_ref_len] = '\0';

	trace_result = runtime_app_lifecycle(user_data, "install", app_id, app_id_len);
	if (trace_result != 0) {
		return trace_result;
	}

	/* Defer the actual install (flash write) to sq_device_protocol_poll, which
	 * runs it between dispatches with the VM idle. Writing the app store from
	 * inside this dispatch corrupts the flash read cache, so a subsequent launch
	 * of the freshly-installed app reads stale bytes and faults. Record the
	 * request now; the poll performs it before any pending launch is processed.
	 */
	result = sq_vm_runtime_request_install(runtime, app_id_buf, file_path_buf);
	if (result != 0) {
		char line[SQ_VM_RUNTIME_DEVICE_ERROR_LEN];
		int n = snprintf(line, sizeof(line), "app.install queue %d (%s)", result,
				 sq_errno_name(result));
		if (n > 0) {
			(void)sq_vm_runtime_record_device_error(runtime, line);
		}
	}
	return result;
}

static void runtime_app_registry_entry_from_store(const struct sq_app_registry_entry *source,
						  SqvmAppRegistryEntry *out)
{
	size_t len;

	if (out == NULL) {
		return;
	}
	memset(out, 0, sizeof(*out));
	if (source == NULL) {
		return;
	}
	len = bounded_strlen(source->app_id, sizeof(source->app_id));
	out->id = (const uint8_t *)source->app_id;
	out->id_len = len;
	out->name = (const uint8_t *)source->app_id;
	out->name_len = len;
}

int32_t runtime_app_registry_list(void *user_data, SqvmAppRegistryEntry *out,
					 size_t out_cap, size_t *out_count)
{
	struct sq_vm_runtime *runtime = user_data;
	size_t count;

	if (runtime == NULL || runtime->registry == NULL || out_count == NULL ||
	    (out == NULL && out_cap > 0)) {
		return -EINVAL;
	}
	count = runtime->registry->count;
	if (count > out_cap) {
		count = out_cap;
	}
	for (size_t i = 0; i < count; i++) {
		runtime_app_registry_entry_from_store(&runtime->registry->apps[i], &out[i]);
	}
	*out_count = count;
	return 0;
}

int32_t runtime_app_registry_get(void *user_data, const uint8_t *app, size_t app_len,
					SqvmAppRegistryEntry *out)
{
	struct sq_vm_runtime *runtime = user_data;
	char app_id[SQ_APP_STORE_APP_ID_MAX];
	const struct sq_app_registry_entry *entry;

	if (runtime == NULL || runtime->registry == NULL || out == NULL || app == NULL ||
	    app_len == 0 || app_len >= sizeof(app_id)) {
		return -EINVAL;
	}
	memcpy(app_id, app, app_len);
	app_id[app_len] = '\0';
	entry = sq_app_registry_find(runtime->registry, app_id);
	if (entry == NULL) {
		return -ENOENT;
	}
	runtime_app_registry_entry_from_store(entry, out);
	return 0;
}

int32_t runtime_app_process_stack(void *user_data, SqvmAppStackEntry *out, size_t out_cap,
					 size_t *out_count)
{
	struct sq_vm_runtime *runtime = user_data;
	size_t count;

	if (runtime == NULL || out_count == NULL || (out == NULL && out_cap > 0)) {
		return -EINVAL;
	}
	count = runtime->return_stack_count;
	if (count > out_cap) {
		count = out_cap;
	}
	for (size_t i = 0; i < count; i++) {
		size_t len = bounded_strlen(runtime->return_stack[i], SQ_APP_STORE_APP_ID_MAX);
		out[i].app_id = (const uint8_t *)runtime->return_stack[i];
		out[i].app_id_len = len;
		out[i].event = NULL;
		out[i].event_len = 0;
	}
	*out_count = count;
	return 0;
}

int32_t runtime_app_armed_stack(void *user_data, SqvmAppStackEntry *out, size_t out_cap,
				       size_t *out_count)
{
	struct sq_vm_runtime *runtime = user_data;
	size_t count = 0;

	if (runtime == NULL || out_count == NULL || (out == NULL && out_cap > 0)) {
		return -EINVAL;
	}
	for (size_t i = 0; i < SQ_VM_RUNTIME_ARMED_TIMER_MAX && count < out_cap; i++) {
		const struct sq_vm_runtime_armed_timer *timer = &runtime->armed_timers[i];
		if (!timer->active) {
			continue;
		}
		out[count].app_id = (const uint8_t *)timer->app_id;
		out[count].app_id_len = bounded_strlen(timer->app_id, sizeof(timer->app_id));
		out[count].event = (const uint8_t *)timer->event;
		out[count].event_len = bounded_strlen(timer->event, sizeof(timer->event));
		count++;
	}
	*out_count = count;
	return 0;
}
