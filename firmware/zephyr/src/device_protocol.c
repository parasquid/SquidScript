#include "device_protocol.h"

#include <errno.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>

#include <zephyr/fs/fs.h>
#include <zephyr/kernel.h>
#include <zephyr/sys/mem_stats.h>
#include <zephyr/sys/sys_heap.h>
#include <zephyr/sys/util.h>

#include "protocol.h"
#include "squidvm_ffi.h"

BUILD_ASSERT(sizeof(struct sq_app_registry_entry) == sizeof(SqdpAppListEntry));
BUILD_ASSERT(offsetof(struct sq_app_registry_entry, app_id) == offsetof(SqdpAppListEntry, app_id));
BUILD_ASSERT(offsetof(struct sq_app_registry_entry, sqbc_len) ==
	     offsetof(SqdpAppListEntry, sqbc_len));
BUILD_ASSERT(SQ_DEVICE_WIFI_PROFILE_NAME_BYTES == SQ_VM_RUNTIME_WIFI_PROFILE_NAME_BYTES);
BUILD_ASSERT(SQ_DEVICE_WIFI_PROFILE_SSID_BYTES == SQ_VM_RUNTIME_WIFI_PROFILE_SSID_BYTES);
BUILD_ASSERT(SQ_DEVICE_WIFI_PROFILE_PASSWORD_BYTES == SQ_VM_RUNTIME_WIFI_PROFILE_PASSWORD_BYTES);

enum sq_device_field_type {
	SQ_DEVICE_FIELD_TYPE_STRING = 1,
	SQ_DEVICE_FIELD_TYPE_U64 = 5,
	SQ_DEVICE_FIELD_TYPE_RECORD = 32,
};

static int sqdp_status_to_protocol_result(SqdpStatus status)
{
	switch (status) {
	case SQDP_STATUS_OK:
		return SQ_PROTOCOL_OK;
	case SQDP_STATUS_BUFFER_TOO_SMALL:
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	case SQDP_STATUS_INVALID_ARGUMENT:
	case SQDP_STATUS_ENCODE_ERROR:
	default:
		return SQ_PROTOCOL_ERR_TRUNCATED_FIELD;
	}
}

static int hello_response(const struct sq_protocol_frame *request,
			  const struct sq_device_identity *identity, uint8_t *response,
			  size_t response_cap, size_t *response_len)
{
	return sqdp_status_to_protocol_result(sqdp_encode_hello_response(
		SQ_OPCODE_HELLO, request->sequence, (const uint8_t *)identity->target,
		strlen(identity->target), (const uint8_t *)identity->firmware,
		strlen(identity->firmware), identity->diagnostic, response, response_cap,
		response_len));
}

static int app_list_response(const struct sq_protocol_frame *request,
			     const struct sq_app_registry *registry, uint8_t *response,
			     size_t response_cap, size_t *response_len)
{
	const SqdpAppListEntry *entries =
		registry == NULL ? NULL : (const SqdpAppListEntry *)registry->apps;
	size_t entry_count = registry == NULL ? 0 : registry->count;

	return sqdp_status_to_protocol_result(sqdp_encode_app_list_response(
		request->sequence, entries, entry_count, response, response_cap, response_len));
}

static int ok_response(const struct sq_protocol_frame *request, uint8_t *response,
		       size_t response_cap, size_t *response_len)
{
	return sqdp_status_to_protocol_result(sqdp_encode_empty_response(
		request->opcode, SQ_STATUS_OK, request->sequence, response, response_cap,
		response_len));
}

static int error_response(const struct sq_protocol_frame *request, int code, uint8_t *response,
			  size_t response_cap, size_t *response_len)
{
	if (code == -ENOTSUP) {
		code = -95;
	}
	return sqdp_status_to_protocol_result(sqdp_encode_error_response_for_code(
		request->opcode, request->sequence, code, response, response_cap, response_len));
}

static void write_u32_le_device(uint8_t *out, uint32_t value)
{
	out[0] = value & 0xffu;
	out[1] = (value >> 8) & 0xffu;
	out[2] = (value >> 16) & 0xffu;
	out[3] = (value >> 24) & 0xffu;
}

static int append_resource_metric(uint8_t *payload, size_t payload_cap, size_t *payload_len,
				  const char *key, uint64_t value)
{
	size_t key_len;
	size_t record_len;
	size_t needed;
	uint8_t *record;
	uint8_t *value_field;

	if (payload == NULL || payload_len == NULL || key == NULL) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	key_len = strlen(key);
	if (key_len > UINT16_MAX || key_len > UINT16_MAX - 16u) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	record_len = 16u + key_len;
	needed = *payload_len + 4u + record_len;
	if (needed > payload_cap) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	payload[*payload_len] = SQ_DEVICE_RECORD_FIELD_ENTRY;
	payload[*payload_len + 1u] = SQ_DEVICE_FIELD_TYPE_RECORD;
	payload[*payload_len + 2u] = record_len & 0xffu;
	payload[*payload_len + 3u] = (record_len >> 8) & 0xffu;

	record = &payload[*payload_len + 4u];
	record[0] = SQ_DEVICE_RECORD_FIELD_KEY;
	record[1] = SQ_DEVICE_FIELD_TYPE_STRING;
	record[2] = key_len & 0xffu;
	record[3] = (key_len >> 8) & 0xffu;
	memcpy(&record[4], key, key_len);

	value_field = &record[4u + key_len];
	value_field[0] = SQ_DEVICE_RECORD_FIELD_VALUE;
	value_field[1] = SQ_DEVICE_FIELD_TYPE_U64;
	value_field[2] = 8u;
	value_field[3] = 0u;
	for (size_t i = 0; i < 8u; i++) {
		value_field[4u + i] = (value >> (i * 8u)) & 0xffu;
	}

	*payload_len = needed;
	return SQ_PROTOCOL_OK;
}

static int encode_resource_metrics_header(uint32_t sequence, uint8_t *response,
					  size_t response_cap, size_t payload_len,
					  size_t *response_len)
{
	uint8_t *payload;

	if (response == NULL || response_len == NULL || response_cap < SQ_PROTOCOL_HEADER_LEN ||
	    payload_len > response_cap - SQ_PROTOCOL_HEADER_LEN || payload_len > UINT32_MAX) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	payload = &response[SQ_PROTOCOL_HEADER_LEN];
	memcpy(response, "SQDP", 4);
	response[4] = SQ_FRAME_RESPONSE;
	response[5] = SQ_OPCODE_RESOURCES_GET;
	response[6] = SQ_STATUS_OK;
	response[7] = 0;
	write_u32_le_device(&response[8], sequence);
	write_u32_le_device(&response[12], (uint32_t)payload_len);
	write_u32_le_device(&response[16], sq_protocol_crc32(payload, payload_len));
	*response_len = SQ_PROTOCOL_HEADER_LEN + payload_len;
	return SQ_PROTOCOL_OK;
}

static int begin_install(const struct sq_protocol_frame *request,
			 const uint8_t *request_bytes, size_t request_len,
			 const struct sq_device_protocol_context *context, uint8_t *response,
			 size_t response_cap, size_t *response_len)
{
	SqdpAction action = {0};

	if (request->opcode == SQ_OPCODE_TEMP_RUN_BEGIN) {
		struct sq_device_temp_session *session = context->temp_session;

		if (session == NULL || context->store_mount_point == NULL) {
			return -ENODEV;
		}
		if (sqdp_prepare_transfer_begin(request_bytes, request_len, session, &action) !=
		    SQDP_STATUS_OK) {
			return -EINVAL;
		}
		int result = sq_app_store_begin_temp_run(context->store_mount_point,
							 session->staging_path,
							 sizeof(session->staging_path));
		if (result != 0) {
			memset(session, 0, sizeof(*session));
			return result;
		}
		session->active = true;
		return ok_response(request, response, response_cap, response_len);
	}

	struct sq_device_install_session *session = context->install_session;

	if (session == NULL || context->store_mount_point == NULL) {
		return -ENODEV;
	}
	if (sqdp_prepare_transfer_begin(request_bytes, request_len, session, &action) !=
	    SQDP_STATUS_OK) {
		return -EINVAL;
	}

	int result = sq_app_store_begin_staged_install(context->store_mount_point, session->app_id,
						      session->staging_path,
						      sizeof(session->staging_path));
	if (result != 0) {
		memset(session, 0, sizeof(*session));
		return result;
	}
	session->active = true;

	return ok_response(request, response, response_cap, response_len);
}

static int append_install_chunk(const struct sq_protocol_frame *request,
				const uint8_t *request_bytes, size_t request_len,
				const struct sq_device_protocol_context *context,
				uint8_t *response, size_t response_cap, size_t *response_len)
{
	SqdpAction action = {0};

	if (request->opcode == SQ_OPCODE_TEMP_RUN_CHUNK) {
		struct sq_device_temp_session *session = context->temp_session;

		if (session == NULL ||
		    sqdp_prepare_transfer_chunk(request_bytes, request_len, session, &action) !=
			    SQDP_STATUS_OK) {
			return -EINVAL;
		}

		int result = sq_app_store_write_staged_chunk(session->staging_path, action.offset,
							     action.bytes, action.bytes_len);
		if (result != 0) {
			return result;
		}
		if (sqdp_complete_transfer_chunk(session, action.bytes, action.bytes_len) !=
		    SQDP_STATUS_OK) {
			return -EINVAL;
		}
		return ok_response(request, response, response_cap, response_len);
	}

	struct sq_device_install_session *session = context->install_session;

	if (session == NULL ||
	    sqdp_prepare_transfer_chunk(request_bytes, request_len, session, &action) !=
		    SQDP_STATUS_OK) {
		return -EINVAL;
	}

	int result = sq_app_store_write_staged_chunk(session->staging_path, action.offset,
						    action.bytes, action.bytes_len);
	if (result != 0) {
		return result;
	}
	if (sqdp_complete_transfer_chunk(session, action.bytes, action.bytes_len) != SQDP_STATUS_OK) {
		return -EINVAL;
	}

	return ok_response(request, response, response_cap, response_len);
}

static int begin_resource_install(const struct sq_protocol_frame *request,
				  const uint8_t *request_bytes, size_t request_len,
				  const struct sq_device_protocol_context *context,
				  uint8_t *response, size_t response_cap, size_t *response_len)
{
	struct sq_device_resource_session *session = context->resource_session;
	SqdpAction action = {0};
	if (session == NULL || context->store_mount_point == NULL) {
		return -ENODEV;
	}
	if (sqdp_prepare_resource_begin(request_bytes, request_len, session, &action) !=
	    SQDP_STATUS_OK) {
		return -EINVAL;
	}

	int result = sq_app_store_begin_staged_resource(context->store_mount_point,
						       session->staging_path,
						       sizeof(session->staging_path));
	if (result != 0) {
		memset(session, 0, sizeof(*session));
		return result;
	}
	session->active = true;
	return ok_response(request, response, response_cap, response_len);
}

static int append_resource_chunk(const struct sq_protocol_frame *request,
				 const uint8_t *request_bytes, size_t request_len,
				 const struct sq_device_protocol_context *context,
				 uint8_t *response, size_t response_cap, size_t *response_len)
{
	struct sq_device_resource_session *session = context->resource_session;
	SqdpAction action = {0};
	if (session == NULL ||
	    sqdp_prepare_resource_chunk(request_bytes, request_len, session, &action) !=
		    SQDP_STATUS_OK) {
		return -EINVAL;
	}

	int result = sq_app_store_write_staged_chunk(session->staging_path, action.offset,
						    action.bytes, action.bytes_len);
	if (result != 0) {
		return result;
	}
	if (sqdp_complete_resource_chunk(session, action.bytes, action.bytes_len) !=
	    SQDP_STATUS_OK) {
		return -EINVAL;
	}
	return ok_response(request, response, response_cap, response_len);
}

static int commit_resource_install(const struct sq_protocol_frame *request,
				   const uint8_t *request_bytes, size_t request_len,
				   const struct sq_device_protocol_context *context,
				   uint8_t *response, size_t response_cap, size_t *response_len)
{
	struct sq_device_resource_session *session = context->resource_session;
	SqdpAction action = {0};
	if (session == NULL || context->store_mount_point == NULL ||
	    sqdp_prepare_resource_commit(request_bytes, request_len, session, &action) !=
		    SQDP_STATUS_OK) {
		return -EINVAL;
	}

	int result = sq_app_store_commit_staged_resource(context->store_mount_point,
							session->app_id,
							session->resource_path,
							session->staging_path);
	if (result != 0) {
		return result;
	}
	sqdp_clear_resource_session(session);
	return ok_response(request, response, response_cap, response_len);
}

struct temp_storage_backend {
	struct sq_vm_fs_storage fs_storage;
	char state_path[SQ_DEVICE_STAGING_PATH_BYTES];
};

static int temp_state_path_for_mount(const char *mount_point, char *out, size_t out_len)
{
	int written;

	if (mount_point == NULL || out == NULL) {
		return -EINVAL;
	}
	written = snprintf(out, out_len, "%s/tmp/temp-run.state.tmp", mount_point);
	if (written < 0 || (size_t)written >= out_len) {
		return -ENAMETOOLONG;
	}
	return 0;
}

static int commit_temp_run(const struct sq_protocol_frame *request,
			   const uint8_t *request_bytes, size_t request_len,
			   const struct sq_device_protocol_context *context, uint8_t *response,
			   size_t response_cap, size_t *response_len)
{
	struct sq_device_temp_session *session = context->temp_session;
	static struct temp_storage_backend temp_storage;
	struct sq_vm_storage_backend backend;
	SqdpAction action = {0};
	int result;

	if (session == NULL || context->runtime == NULL || context->store_mount_point == NULL ||
	    sqdp_prepare_transfer_commit(request_bytes, request_len, session, &action) !=
		    SQDP_STATUS_OK) {
		return -EINVAL;
	}

	memset(&temp_storage, 0, sizeof(temp_storage));
	result = temp_state_path_for_mount(context->store_mount_point, temp_storage.state_path,
					   sizeof(temp_storage.state_path));
	if (result != 0) {
		return result;
	}
	temp_storage.fs_storage.sqbc_path = session->staging_path;
	temp_storage.fs_storage.state_path = temp_storage.state_path;
	backend = sq_vm_fs_storage_backend(&temp_storage.fs_storage);
	result = backend.reset_state(backend.user_data);
	if (result != 0) {
		return result;
	}
	result = sq_vm_runtime_start(context->runtime, &backend, "app.start");
	if (result != 0) {
		return result;
	}

	return ok_response(request, response, response_cap, response_len);
}

static int commit_install(const struct sq_protocol_frame *request,
			  const uint8_t *request_bytes, size_t request_len,
			  const struct sq_device_protocol_context *context, uint8_t *response,
			  size_t response_cap, size_t *response_len)
{
	struct sq_device_install_session *session = context->install_session;
	SqdpAction action = {0};
	int result;

	if (session == NULL || context->store_mount_point == NULL ||
	    sqdp_prepare_transfer_commit(request_bytes, request_len, session, &action) !=
		    SQDP_STATUS_OK) {
		return -EINVAL;
	}

	result = sq_app_store_commit_staged_install(context->store_mount_point, session->app_id,
						   session->staging_path);
	if (result != 0) {
		return result;
	}
	if (context->mutable_registry != NULL) {
		result = sq_app_store_scan_registry(context->store_mount_point,
						    context->mutable_registry);
		if (result != 0) {
			return result;
		}
	}

	sqdp_clear_transfer_session(session);
	return ok_response(request, response, response_cap, response_len);
}

static int start_installed_app(const struct sq_device_protocol_context *context,
			       const char *app_id, const char *event, bool set_current);
static void clear_foreground_timers(struct sq_vm_runtime *runtime);

static int launch_app(const struct sq_protocol_frame *request,
		      const uint8_t *request_bytes, size_t request_len,
		      const struct sq_device_protocol_context *context, uint8_t *response,
		      size_t response_cap, size_t *response_len)
{
	SqdpAppLaunch launch = {0};
	int result;

	if (context->runtime == NULL || context->store_mount_point == NULL ||
	    context->launch_storage == NULL) {
		return -ENODEV;
	}

	if (sqdp_parse_app_launch_request(request_bytes, request_len, &launch) != SQDP_STATUS_OK ||
	    launch.app_id_len >= SQ_APP_STORE_APP_ID_MAX) {
		return -EINVAL;
	}

	char app_id_buffer[SQ_APP_STORE_APP_ID_MAX];
	memcpy(app_id_buffer, launch.app_id, launch.app_id_len);
	app_id_buffer[launch.app_id_len] = '\0';

	result = start_installed_app(context, app_id_buffer, "app.start", true);
	if (result != 0) {
		return result;
	}

	return ok_response(request, response, response_cap, response_len);
}

static int start_installed_app(const struct sq_device_protocol_context *context,
			       const char *app_id, const char *event, bool set_current)
{
	struct sq_vm_storage_backend backend;
	char previous_app[SQ_APP_STORE_APP_ID_MAX];
	int result;
	bool current_app_changed;

	if (context == NULL || context->runtime == NULL || context->store_mount_point == NULL ||
	    context->launch_storage == NULL || app_id == NULL || event == NULL) {
		return -EINVAL;
	}
	if (strlen(app_id) >= SQ_APP_STORE_APP_ID_MAX) {
		return -EINVAL;
	}
	current_app_changed = set_current && strcmp(context->runtime->current_app, app_id) != 0;
	if (current_app_changed) {
		clear_foreground_timers(context->runtime);
	}
	if (set_current || strcmp(context->runtime->current_app, app_id) != 0) {
		sq_vm_runtime_reset_vm_context(context->runtime);
	}

	result = sq_app_store_vm_storage_for_app(context->store_mount_point, app_id,
						context->launch_storage);
	if (result != 0) {
		return result;
	}
	backend = sq_app_store_vm_storage_backend(context->launch_storage);
	if (set_current) {
		strncpy(previous_app, context->runtime->current_app, sizeof(previous_app) - 1);
		previous_app[sizeof(previous_app) - 1] = '\0';
		strncpy(context->runtime->current_app, app_id,
			sizeof(context->runtime->current_app) - 1);
		context->runtime->current_app[sizeof(context->runtime->current_app) - 1] = '\0';
	}
	result = sq_vm_runtime_start(context->runtime, &backend, event);
	if (result != 0) {
		if (set_current) {
			strncpy(context->runtime->current_app, previous_app,
				sizeof(context->runtime->current_app) - 1);
			context->runtime->current_app[sizeof(context->runtime->current_app) - 1] =
				'\0';
		}
		return result;
	}
	return 0;
}

static void clear_foreground_timers(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL) {
		return;
	}
	memset(runtime->timers, 0, sizeof(runtime->timers));
}

static int push_return_app(struct sq_vm_runtime *runtime, const char *app_id)
{
	if (runtime == NULL || app_id == NULL || app_id[0] == '\0') {
		return 0;
	}
	if (runtime->return_stack_count >= SQ_VM_RUNTIME_RETURN_STACK_MAX) {
		return -ENOSPC;
	}
	strncpy(runtime->return_stack[runtime->return_stack_count], app_id,
		sizeof(runtime->return_stack[0]) - 1);
	runtime->return_stack[runtime->return_stack_count][sizeof(runtime->return_stack[0]) - 1] =
		'\0';
	runtime->return_stack_count++;
	return 0;
}

static int pop_return_app(struct sq_vm_runtime *runtime, char *out, size_t out_len)
{
	if (runtime == NULL || out == NULL || out_len == 0) {
		return -EINVAL;
	}
	if (runtime->return_stack_count == 0) {
		strncpy(out, "main", out_len - 1);
		out[out_len - 1] = '\0';
		return 0;
	}
	runtime->return_stack_count--;
	strncpy(out, runtime->return_stack[runtime->return_stack_count], out_len - 1);
	out[out_len - 1] = '\0';
	memset(runtime->return_stack[runtime->return_stack_count], 0,
	       sizeof(runtime->return_stack[0]));
	return 0;
}

static size_t c_array_len(const uint8_t *bytes, size_t cap)
{
	size_t len = 0;

	while (len < cap && bytes[len] != 0) {
		len++;
	}
	return len;
}

static int register_app_triggers(const struct sq_device_protocol_context *context,
				 const char *app_id)
{
	char sqbc_path[SQ_APP_STORE_PATH_MAX];
	struct sq_vm_fs_storage trigger_storage = {
		.sqbc_path = sqbc_path,
	};
	struct sq_vm_storage_backend backend;
	size_t trigger_count = 0;
	SqvmStatus status;
	int result;

	if (context == NULL || context->runtime == NULL || context->store_mount_point == NULL ||
	    app_id == NULL) {
		return -EINVAL;
	}
	result = sq_app_store_sqbc_path(context->store_mount_point, app_id, sqbc_path,
					sizeof(sqbc_path));
	if (result != 0) {
		return result;
	}
	backend = sq_vm_fs_storage_backend(&trigger_storage);
	if (backend.read_sqbc == NULL) {
		return -ENODEV;
	}

	result = sq_vm_runtime_clear_armed_app(context->runtime, (const uint8_t *)app_id,
					      strlen(app_id));
	if (result != 0) {
		return result;
	}
	status = sqvm_trigger_timer_count_from_reader(
		backend.user_data, backend.read_sqbc, context->runtime->transfer.init_scratch,
		sizeof(context->runtime->transfer.init_scratch), &trigger_count);
	if (status != SQVM_STATUS_OK || trigger_count > SQ_VM_RUNTIME_ARMED_TIMER_MAX) {
		return -EINVAL;
	}
	for (size_t i = 0; i < trigger_count; i++) {
		SqvmTriggerTimer timer = {0};
		size_t event_len;

		status = sqvm_trigger_timer_read_from_reader(
			backend.user_data, backend.read_sqbc, context->runtime->transfer.init_scratch,
			sizeof(context->runtime->transfer.init_scratch), i, &timer);
		if (status != SQVM_STATUS_OK) {
			return -EINVAL;
		}
		event_len = c_array_len(timer.event, sizeof(timer.event));
		result = sq_vm_runtime_register_armed_timer(context->runtime, app_id, timer.event,
							   event_len, timer.interval_ms,
							   timer.repeating);
		if (result != 0) {
			return result;
		}
	}
	return 0;
}

int sq_device_protocol_poll(const struct sq_device_protocol_context *context)
{
	struct sq_vm_runtime *runtime;
	int result;

	if (context == NULL || context->runtime == NULL) {
		return -EINVAL;
	}
	runtime = context->runtime;

	if (runtime->status == SQ_VM_RUNTIME_RUNNING) {
		return 0;
	}

	if (runtime->lifecycle_launch_after_exit) {
		runtime->lifecycle_launch_after_exit = false;
		result = push_return_app(runtime, runtime->current_app);
		if (result != 0) {
			return result;
		}
		result = start_installed_app(context, runtime->lifecycle_target_app, "app.start",
					     true);
		memset(runtime->lifecycle_target_app, 0, sizeof(runtime->lifecycle_target_app));
		runtime->dispatch_exited = false;
		return result;
	}

	if (runtime->arm_registration_active) {
		runtime->arm_registration_active = false;
		memset(runtime->arm_registration_app, 0, sizeof(runtime->arm_registration_app));
		return 0;
	}

	if (runtime->pending_launch_active) {
		strncpy(runtime->lifecycle_target_app, runtime->pending_launch_app,
			sizeof(runtime->lifecycle_target_app) - 1);
		runtime->lifecycle_target_app[sizeof(runtime->lifecycle_target_app) - 1] = '\0';
		memset(runtime->pending_launch_app, 0, sizeof(runtime->pending_launch_app));
		runtime->pending_launch_active = false;

		if (runtime->current_app[0] == '\0') {
			return start_installed_app(context, runtime->lifecycle_target_app, "app.start",
						   true);
		}
		runtime->lifecycle_launch_after_exit = true;
		runtime->dispatch_exited = false;
		return start_installed_app(context, runtime->current_app, "app.exit", false);
	}

	if (runtime->pending_arm_active) {
		result = register_app_triggers(context, runtime->pending_arm_app);
		memset(runtime->pending_arm_app, 0, sizeof(runtime->pending_arm_app));
		runtime->pending_arm_active = false;
		return result;
	}

	if (runtime->dispatch_exited) {
		runtime->dispatch_exited = false;
		result = pop_return_app(runtime, runtime->lifecycle_target_app,
					sizeof(runtime->lifecycle_target_app));
		if (result != 0) {
			return result;
		}
		result = start_installed_app(context, runtime->lifecycle_target_app, "app.start",
					     true);
		memset(runtime->lifecycle_target_app, 0, sizeof(runtime->lifecycle_target_app));
		return result;
	}

	if (sq_vm_runtime_next_due_armed_timer(runtime, runtime->lifecycle_target_app,
					       sizeof(runtime->lifecycle_target_app),
					       runtime->event, sizeof(runtime->event)) == 0) {
		result = push_return_app(runtime, runtime->current_app);
		if (result != 0) {
			return result;
		}
		result = start_installed_app(context, runtime->lifecycle_target_app,
					     runtime->event, true);
		memset(runtime->lifecycle_target_app, 0, sizeof(runtime->lifecycle_target_app));
		return result;
	}

	return sq_vm_runtime_poll(runtime);
}

static int repeated_runtime_lines_response(const struct sq_protocol_frame *request,
					   const struct sq_vm_runtime *runtime,
					   const char *const *extra_lines, size_t extra_count,
					   uint8_t *response, size_t response_cap,
					   size_t *response_len)
{
	const uint8_t *fixed_lines = NULL;
	size_t fixed_count = 0;
	size_t fixed_stride = 0;
	SqdpLineSlice extra_slices[1];
	size_t extra_slice_count = 0;

	if (runtime != NULL && request->opcode == SQ_OPCODE_TRACE_GET) {
		fixed_lines = (const uint8_t *)runtime->traces;
		fixed_count = runtime->trace_count;
		fixed_stride = SQ_VM_RUNTIME_TRACE_LEN;
	}
	if (runtime != NULL && request->opcode == SQ_OPCODE_OUTPUT_GET) {
		fixed_lines = (const uint8_t *)runtime->outputs;
		fixed_count = runtime->output_count;
		fixed_stride = SQ_VM_RUNTIME_OUTPUT_LEN;
	}
	if (runtime != NULL && request->opcode == SQ_OPCODE_DRAWLOG_GET) {
		fixed_lines = (const uint8_t *)runtime->drawlog;
		fixed_count = runtime->drawlog_count;
		fixed_stride = SQ_VM_RUNTIME_DRAWLOG_LEN;
	}
	if (extra_count > ARRAY_SIZE(extra_slices)) {
		return -EINVAL;
	}
	for (size_t i = 0; i < extra_count; i++) {
		extra_slices[i] = (SqdpLineSlice){
			.bytes = (const uint8_t *)extra_lines[i],
			.len = strlen(extra_lines[i]),
		};
		extra_slice_count++;
	}
	return sqdp_status_to_protocol_result(sqdp_encode_line_response(
		request->opcode, request->sequence, fixed_lines, fixed_count, fixed_stride,
		extra_slice_count == 0 ? NULL : extra_slices, extra_slice_count, response,
		response_cap, response_len));
}

static int lifecycle_response(const struct sq_protocol_frame *request,
			      const struct sq_vm_runtime *runtime, uint8_t *response,
			      size_t response_cap, size_t *response_len)
{
	const uint8_t *active_app = NULL;
	size_t active_app_len = 0;
	const uint8_t *process_stack = NULL;
	size_t process_count = 0;
	SqdpLifecycleTimer armed_timers[SQ_VM_RUNTIME_ARMED_TIMER_MAX];
	size_t armed_count = 0;

	if (runtime != NULL) {
		if (runtime->current_app[0] != '\0') {
			active_app = (const uint8_t *)runtime->current_app;
			active_app_len = strlen(runtime->current_app);
		}
		process_stack = (const uint8_t *)runtime->return_stack;
		process_count = runtime->return_stack_count;
		memset(armed_timers, 0, sizeof(armed_timers));
		for (size_t i = 0; i < SQ_VM_RUNTIME_ARMED_TIMER_MAX; i++) {
			const struct sq_vm_runtime_armed_timer *timer = &runtime->armed_timers[i];
			if (!timer->active) {
				continue;
			}
			strncpy((char *)armed_timers[armed_count].app_id, timer->app_id,
				sizeof(armed_timers[armed_count].app_id) - 1);
			strncpy((char *)armed_timers[armed_count].event, timer->event,
				sizeof(armed_timers[armed_count].event) - 1);
			armed_count++;
		}
	}

	return sqdp_status_to_protocol_result(sqdp_encode_lifecycle_response(
		request->sequence, active_app, active_app_len, process_stack, process_count,
		SQ_APP_STORE_APP_ID_MAX, armed_count == 0 ? NULL : armed_timers, armed_count,
		response, response_cap, response_len));
}

static int state_get_response(const struct sq_protocol_frame *request,
			      const struct sq_device_protocol_context *context, uint8_t *response,
			      size_t response_cap, size_t *response_len)
{
	size_t bytes_len = 0;
	uint8_t *state_bytes;
	size_t state_cap;
	int result;

	if (context->launch_storage == NULL) {
		return sqdp_status_to_protocol_result(sqdp_encode_state_response(
			request->sequence, NULL, 0, response, response_cap, response_len));
	}
	if (context->runtime == NULL) {
		return -ENODEV;
	}

	struct sq_vm_storage_backend backend =
		sq_app_store_vm_storage_backend(context->launch_storage);
	if (backend.load_state == NULL) {
		return -ENODEV;
	}

	state_bytes = context->runtime->transfer.completion.bytes;
	state_cap = sizeof(context->runtime->transfer.completion.bytes);
	result = backend.load_state(backend.user_data, state_bytes, state_cap, &bytes_len);
	if (result != 0 && result != -ENOENT) {
		return result;
	}
	if (result == -ENOENT) {
		bytes_len = 0;
	}
	if (bytes_len > state_cap) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	return sqdp_status_to_protocol_result(sqdp_encode_state_response(
		request->sequence, state_bytes, bytes_len, response, response_cap, response_len));
}

static int state_import(const struct sq_protocol_frame *request,
			const uint8_t *request_bytes, size_t request_len,
			const struct sq_device_protocol_context *context, uint8_t *response,
			size_t response_cap, size_t *response_len)
{
	SqdpStateImport import = {0};

	if (sqdp_parse_state_import_request(request_bytes, request_len, &import) != SQDP_STATUS_OK ||
	    context->launch_storage == NULL) {
		return -EINVAL;
	}
	struct sq_vm_storage_backend backend =
		sq_app_store_vm_storage_backend(context->launch_storage);
	if (backend.save_state == NULL) {
		return -ENODEV;
	}
	int result = backend.save_state(backend.user_data, import.bytes, import.bytes_len);
	if (result != 0) {
		return result;
	}
	return ok_response(request, response, response_cap, response_len);
}

static int resources_response(const struct sq_protocol_frame *request,
			      const struct sq_device_protocol_context *context, uint8_t *response,
			      size_t response_cap, size_t *response_len)
{
	uint8_t *payload;
	size_t payload_cap;
	size_t payload_len = 0;
	size_t vm_worker_stack_unused = 0;
	size_t vm_worker_stack_size = context->runtime == NULL ? 0 : sq_vm_runtime_work_stack_size();
	size_t vm_worker_stack_used = 0;
	size_t protocol_stack_size = CONFIG_MAIN_STACK_SIZE;
	size_t protocol_stack_pre_resources_unused = 0;
	size_t protocol_stack_pre_resources_used = 0;
	size_t protocol_stack_unused = 0;
	size_t protocol_stack_used = 0;
	size_t heap_count = 0;
	size_t heap_free_bytes = 0;
	size_t heap_allocated_bytes = 0;
	size_t heap_max_allocated_bytes = 0;
	size_t input_button_pressed_count = 0;
	size_t input_button_state = 0;

	if (response == NULL || response_len == NULL || response_cap < SQ_PROTOCOL_HEADER_LEN) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	*response_len = 0;
	payload = &response[SQ_PROTOCOL_HEADER_LEN];
	payload_cap = response_cap - SQ_PROTOCOL_HEADER_LEN;

	if (k_thread_stack_space_get(k_current_get(), &protocol_stack_pre_resources_unused) == 0 &&
	    protocol_stack_pre_resources_unused <= protocol_stack_size) {
		protocol_stack_pre_resources_used =
			protocol_stack_size - protocol_stack_pre_resources_unused;
	} else {
		protocol_stack_pre_resources_unused = 0;
		protocol_stack_pre_resources_used = 0;
	}

	if (context->runtime != NULL && sq_vm_runtime_work_stack_unused(&vm_worker_stack_unused) == 0 &&
	    vm_worker_stack_unused <= vm_worker_stack_size) {
		vm_worker_stack_used = vm_worker_stack_size - vm_worker_stack_unused;
	} else {
		vm_worker_stack_unused = 0;
		vm_worker_stack_used = 0;
	}
	if (k_thread_stack_space_get(k_current_get(), &protocol_stack_unused) == 0 &&
	    protocol_stack_unused <= protocol_stack_size) {
		protocol_stack_used = protocol_stack_size - protocol_stack_unused;
	} else {
		protocol_stack_unused = 0;
		protocol_stack_used = 0;
	}

#ifdef CONFIG_SYS_HEAP_RUNTIME_STATS
	struct k_heap *heaps = NULL;
	int heap_array_count = k_heap_array_get(&heaps);
	if (heap_array_count > 0 && heaps != NULL) {
		heap_count = (size_t)heap_array_count;
		for (int i = 0; i < heap_array_count; i++) {
			struct sys_memory_stats stats;

			if (sys_heap_runtime_stats_get(&heaps[i].heap, &stats) == 0) {
				heap_free_bytes += stats.free_bytes;
				heap_allocated_bytes += stats.allocated_bytes;
				heap_max_allocated_bytes += stats.max_allocated_bytes;
			}
		}
	}
#endif

	if (context->runtime != NULL) {
		for (size_t i = 0; i < SQ_VM_RUNTIME_INPUT_BUTTON_MAX; i++) {
			if (context->runtime->input_buttons[i].active &&
			    context->runtime->input_buttons[i].pressed) {
				input_button_pressed_count++;
			}
		}
		input_button_state = context->runtime->input_button_count |
				     (input_button_pressed_count << 8);
	}

#define SQ_RESOURCE_METRIC(key_literal, metric_value) \
	do { \
		int metric_result = append_resource_metric(payload, payload_cap, &payload_len, \
							   (key_literal), (metric_value)); \
		if (metric_result != SQ_PROTOCOL_OK) { \
			return metric_result; \
		} \
	} while (false)
	SQ_RESOURCE_METRIC("ram_total_bytes", CONFIG_SRAM_SIZE * 1024u);
	SQ_RESOURCE_METRIC("runtime_static_bytes",
			   context->runtime == NULL ? 0 : sizeof(*context->runtime));
	SQ_RESOURCE_METRIC("vm_sqbc_chunk_bytes", SQVM_STORAGE_TRANSFER_CAPACITY);
	SQ_RESOURCE_METRIC("heap_count", heap_count);
	SQ_RESOURCE_METRIC("heap_free_bytes", heap_free_bytes);
	SQ_RESOURCE_METRIC("heap_alloc_bytes", heap_allocated_bytes);
	SQ_RESOURCE_METRIC("heap_max_alloc_bytes", heap_max_allocated_bytes);
	SQ_RESOURCE_METRIC("last_dispatch_us",
			   context->runtime == NULL ? 0 : context->runtime->last_dispatch_elapsed_us);
	SQ_RESOURCE_METRIC("last_dispatch_seq",
			   context->runtime == NULL ? 0 : context->runtime->last_dispatch_sequence);
	SQ_RESOURCE_METRIC("last_sqbc_reads",
			   context->runtime == NULL ? 0 :
						      context->runtime->last_dispatch_sqbc_read_count);
	SQ_RESOURCE_METRIC("last_sqbc_bytes",
			   context->runtime == NULL ? 0 :
						      context->runtime->last_dispatch_sqbc_read_bytes);
	SQ_RESOURCE_METRIC("proto_stack_size_bytes", protocol_stack_size);
	SQ_RESOURCE_METRIC("proto_stack_pre_res_unused_bytes",
			   protocol_stack_pre_resources_unused);
	SQ_RESOURCE_METRIC("proto_stack_pre_res_used_bytes",
			   protocol_stack_pre_resources_used);
	SQ_RESOURCE_METRIC("proto_stack_unused_bytes", protocol_stack_unused);
	SQ_RESOURCE_METRIC("proto_stack_used_bytes", protocol_stack_used);
	SQ_RESOURCE_METRIC("vm_stack_size_bytes", vm_worker_stack_size);
	SQ_RESOURCE_METRIC("vm_stack_unused_bytes", vm_worker_stack_unused);
	SQ_RESOURCE_METRIC("vm_stack_used_bytes", vm_worker_stack_used);
	SQ_RESOURCE_METRIC("app_count", context->registry == NULL ? 0 : context->registry->count);
	SQ_RESOURCE_METRIC("input_button_state", input_button_state);
#undef SQ_RESOURCE_METRIC

	return encode_resource_metrics_header(request->sequence, response, response_cap,
					      payload_len, response_len);
}

static void clear_runtime_context(const struct sq_device_protocol_context *context)
{
	if (context->runtime != NULL) {
		sq_vm_runtime_reset(context->runtime);
	}
	if (context->install_session != NULL) {
		memset(context->install_session, 0, sizeof(*context->install_session));
	}
	if (context->temp_session != NULL) {
		memset(context->temp_session, 0, sizeof(*context->temp_session));
	}
	if (context->resource_session != NULL) {
		memset(context->resource_session, 0, sizeof(*context->resource_session));
	}
	if (context->launch_storage != NULL) {
		memset(context->launch_storage, 0, sizeof(*context->launch_storage));
	}
}

static int reset_runtime(const struct sq_protocol_frame *request,
			 const struct sq_device_protocol_context *context, uint8_t *response,
			 size_t response_cap, size_t *response_len)
{
	clear_runtime_context(context);
	return ok_response(request, response, response_cap, response_len);
}

static int storage_format(const struct sq_protocol_frame *request,
			  const struct sq_device_protocol_context *context, uint8_t *response,
			  size_t response_cap, size_t *response_len)
{
	if (context->store_mount_point == NULL) {
		return -ENODEV;
	}
	clear_runtime_context(context);
	int result = sq_app_store_format_filesystem(context->store_mount_point);
	if (result != 0) {
		return result;
	}
	if (context->mutable_registry != NULL) {
		memset(context->mutable_registry, 0, sizeof(*context->mutable_registry));
	}
	return ok_response(request, response, response_cap, response_len);
}

static int dispatch_event_from_parts(const struct sq_protocol_frame *request,
				     const struct sq_device_protocol_context *context,
				     const uint8_t *app_id, size_t app_id_len,
				     const uint8_t *event, size_t event_len, uint8_t *response,
				     size_t response_cap, size_t *response_len)
{
	if (event == NULL || event_len == 0 || event_len >= SQ_VM_RUNTIME_EVENT_LEN ||
	    context->runtime == NULL || context->store_mount_point == NULL ||
	    context->launch_storage == NULL) {
		return -EINVAL;
	}
	if (app_id != NULL) {
		if (app_id_len == 0 || app_id_len >= SQ_APP_STORE_APP_ID_MAX) {
			return -EINVAL;
		}
		char app_id_buffer[SQ_APP_STORE_APP_ID_MAX];
		memcpy(app_id_buffer, app_id, app_id_len);
		app_id_buffer[app_id_len] = '\0';
		if (context->runtime->current_app[0] != '\0' &&
		    strcmp(context->runtime->current_app, app_id_buffer) != 0) {
			return -EINVAL;
		}
		if (context->runtime->current_app[0] == '\0') {
			sq_vm_runtime_reset_vm_context(context->runtime);
		}
		int result = sq_app_store_vm_storage_for_app(context->store_mount_point,
							     app_id_buffer,
							     context->launch_storage);
		if (result != 0) {
			return result;
		}
	} else if (context->runtime->current_app[0] != '\0') {
		int result = sq_app_store_vm_storage_for_app(context->store_mount_point,
							     context->runtime->current_app,
							     context->launch_storage);
		if (result != 0) {
			return result;
		}
	}
	if (context->launch_storage->fs_storage.sqbc_path == NULL) {
		return -ENODEV;
	}

	char event_buffer[SQ_VM_RUNTIME_EVENT_LEN];
	memcpy(event_buffer, event, event_len);
	event_buffer[event_len] = '\0';
	struct sq_vm_storage_backend backend =
		sq_app_store_vm_storage_backend(context->launch_storage);
	int result = sq_vm_runtime_start(context->runtime, &backend, event_buffer);
	if (result != 0) {
		return result;
	}
	return ok_response(request, response, response_cap, response_len);
}

static int dispatch_event_request(const struct sq_protocol_frame *request,
				  const uint8_t *request_bytes, size_t request_len,
				  const struct sq_device_protocol_context *context,
				  uint8_t *response, size_t response_cap, size_t *response_len)
{
	SqdpEventDispatch event = {0};

	if (sqdp_parse_event_dispatch_request(request_bytes, request_len, &event) != SQDP_STATUS_OK) {
		return -EINVAL;
	}
	return dispatch_event_from_parts(request, context, event.app_id, event.app_id_len,
					 event.event, event.event_len, response, response_cap,
					 response_len);
}

static int dispatch_key(const struct sq_protocol_frame *request,
			const uint8_t *request_bytes, size_t request_len,
			const struct sq_device_protocol_context *context, uint8_t *response,
			size_t response_cap, size_t *response_len)
{
	uint8_t event[SQ_VM_RUNTIME_EVENT_LEN];
	size_t event_len = 0;

	if (sqdp_prepare_key_event(request_bytes, request_len, event, sizeof(event), &event_len) !=
	    SQDP_STATUS_OK) {
		return -EINVAL;
	}
	return dispatch_event_from_parts(request, context, NULL, 0, event, event_len, response,
					 response_cap, response_len);
}

static int wifi_profile_set(const struct sq_protocol_frame *request,
			    const uint8_t *request_bytes, size_t request_len,
			    const struct sq_device_protocol_context *context, uint8_t *response,
			    size_t response_cap, size_t *response_len)
{
	SqdpWifiProfile profile = {0};

	if (context == NULL || context->runtime == NULL) {
		return -ENODEV;
	}
	if (sqdp_parse_wifi_profile_set_request(request_bytes, request_len, &profile) !=
	    SQDP_STATUS_OK) {
		return -EINVAL;
	}
	int result = sq_vm_runtime_set_wifi_profile(context->runtime, profile.profile,
						    profile.profile_len, profile.ssid,
						    profile.ssid_len, profile.password,
						    profile.password_len);
	if (result != 0) {
		return result;
	}
	return ok_response(request, response, response_cap, response_len);
}

int sq_device_protocol_handle_frame(const uint8_t *request, size_t request_len,
				    const struct sq_device_protocol_context *context, uint8_t *response,
				    size_t response_cap, size_t *response_len)
{
	struct sq_protocol_frame frame;
	int result;

	*response_len = 0;
	if (context == NULL || context->identity == NULL) {
		return SQ_PROTOCOL_ERR_BAD_MAGIC;
	}

	result = sq_protocol_decode_frame(request, request_len, &frame);
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}

	if (frame.kind != SQ_FRAME_REQUEST) {
		return SQ_PROTOCOL_ERR_BAD_MAGIC;
	}

	switch (frame.opcode) {
	case SQ_OPCODE_HELLO:
		result = hello_response(&frame, context->identity, response, response_cap,
					response_len);
		break;
	case SQ_OPCODE_APP_LIST:
		result = app_list_response(&frame, context->registry, response, response_cap,
					   response_len);
		break;
	case SQ_OPCODE_APP_INSTALL_BEGIN:
	case SQ_OPCODE_TEMP_RUN_BEGIN:
		result = begin_install(&frame, request, request_len, context, response, response_cap,
				       response_len);
		break;
	case SQ_OPCODE_APP_INSTALL_CHUNK:
	case SQ_OPCODE_TEMP_RUN_CHUNK:
		result = append_install_chunk(&frame, request, request_len, context, response,
					      response_cap, response_len);
		break;
	case SQ_OPCODE_APP_INSTALL_COMMIT:
		result = commit_install(&frame, request, request_len, context, response, response_cap,
					response_len);
		break;
	case SQ_OPCODE_RESOURCE_INSTALL_BEGIN:
		result = begin_resource_install(&frame, request, request_len, context, response,
						response_cap, response_len);
		break;
	case SQ_OPCODE_RESOURCE_INSTALL_CHUNK:
		result = append_resource_chunk(&frame, request, request_len, context, response,
					       response_cap, response_len);
		break;
	case SQ_OPCODE_RESOURCE_INSTALL_COMMIT:
		result = commit_resource_install(&frame, request, request_len, context, response,
						 response_cap, response_len);
		break;
	case SQ_OPCODE_TEMP_RUN_COMMIT:
		result = commit_temp_run(&frame, request, request_len, context, response,
					 response_cap, response_len);
		break;
	case SQ_OPCODE_APP_LAUNCH:
		result = launch_app(&frame, request, request_len, context, response, response_cap,
				    response_len);
		break;
	case SQ_OPCODE_OUTPUT_GET:
		result = repeated_runtime_lines_response(&frame, context->runtime, NULL, 0,
							 response, response_cap, response_len);
		break;
	case SQ_OPCODE_TRACE_GET:
		result = repeated_runtime_lines_response(&frame, context->runtime, NULL, 0,
							 response, response_cap, response_len);
		break;
	case SQ_OPCODE_DRAWLOG_GET:
		result = repeated_runtime_lines_response(&frame, context->runtime, NULL, 0,
							 response, response_cap, response_len);
		break;
	case SQ_OPCODE_ERRORS_GET: {
		const char *lines[1];
		size_t line_count = 0;
		char error_line[48];
		if (context->runtime != NULL && context->runtime->status == SQ_VM_RUNTIME_ERROR) {
			const char *status_name =
				context->runtime->result.status == SQVM_STATUS_OK ?
					"host_error" :
					sq_vm_runtime_status_name(context->runtime->result.status);
			int written = snprintf(error_line, sizeof(error_line), "runtime=%s code=%d",
					       status_name, context->runtime->result_code);
			if (written > 0 && (size_t)written < sizeof(error_line)) {
				lines[line_count++] = error_line;
			}
		}
		result = repeated_runtime_lines_response(&frame, context->runtime, lines, line_count,
							 response, response_cap, response_len);
		break;
	}
	case SQ_OPCODE_STATE_GET:
		result = state_get_response(&frame, context, response, response_cap, response_len);
		break;
	case SQ_OPCODE_STATE_IMPORT:
		result = state_import(&frame, request, request_len, context, response, response_cap,
				      response_len);
		break;
	case SQ_OPCODE_RESOURCES_GET:
		result = resources_response(&frame, context, response, response_cap, response_len);
		break;
	case SQ_OPCODE_LIFECYCLE_GET:
		result = lifecycle_response(&frame, context->runtime, response, response_cap,
					    response_len);
		break;
	case SQ_OPCODE_RESET:
		result = reset_runtime(&frame, context, response, response_cap, response_len);
		break;
	case SQ_OPCODE_STORAGE_FORMAT:
		result = storage_format(&frame, context, response, response_cap, response_len);
		break;
	case SQ_OPCODE_EVENT_DISPATCH:
		result = dispatch_event_request(&frame, request, request_len, context, response, response_cap,
						response_len);
		break;
	case SQ_OPCODE_KEY:
		result = dispatch_key(&frame, request, request_len, context, response, response_cap,
				      response_len);
		break;
	case SQ_OPCODE_WIFI_PROFILE_SET:
		result = wifi_profile_set(&frame, request, request_len, context, response,
					  response_cap, response_len);
		break;
	default:
		return SQ_PROTOCOL_ERR_BAD_MAGIC;
	}
	if (result != SQ_PROTOCOL_OK) {
		return error_response(&frame, result, response, response_cap, response_len);
	}
	return SQ_PROTOCOL_OK;
}
