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

#include "app_lifecycle.h"
#include "protocol.h"
#include "squidvm_ffi.h"

BUILD_ASSERT(sizeof(struct sq_app_registry_entry) == sizeof(SqdpAppListEntry));
BUILD_ASSERT(offsetof(struct sq_app_registry_entry, app_id) == offsetof(SqdpAppListEntry, app_id));
BUILD_ASSERT(offsetof(struct sq_app_registry_entry, sqbc_len) ==
	     offsetof(SqdpAppListEntry, sqbc_len));
#if SIZE_MAX == UINT64_MAX
BUILD_ASSERT(sizeof(struct sq_device_install_session) == 160);
BUILD_ASSERT(sizeof(struct sq_device_temp_session) == 160);
BUILD_ASSERT(sizeof(struct sq_device_resource_session) == 240);
#else
BUILD_ASSERT(sizeof(struct sq_device_install_session) == 144);
BUILD_ASSERT(sizeof(struct sq_device_temp_session) == 144);
BUILD_ASSERT(sizeof(struct sq_device_resource_session) == 224);
#endif
BUILD_ASSERT(SQ_DEVICE_WIFI_PROFILE_NAME_BYTES == SQ_VM_RUNTIME_WIFI_PROFILE_NAME_BYTES);
BUILD_ASSERT(SQ_DEVICE_WIFI_PROFILE_SSID_BYTES == SQ_VM_RUNTIME_WIFI_PROFILE_SSID_BYTES);
BUILD_ASSERT(SQ_DEVICE_WIFI_PROFILE_PASSWORD_BYTES == SQ_VM_RUNTIME_WIFI_PROFILE_PASSWORD_BYTES);

enum sq_device_field_type {
	SQ_DEVICE_FIELD_TYPE_BOOL = 3,
	SQ_DEVICE_FIELD_TYPE_STRING = 1,
	SQ_DEVICE_FIELD_TYPE_U64 = 5,
	SQ_DEVICE_FIELD_TYPE_U32 = 6,
	SQ_DEVICE_FIELD_TYPE_RECORD = 32,
};

enum sq_resources_request_field {
	SQ_RESOURCES_FIELD_RESET_HEAP_MAX = 1,
};

static int copy_app_id(char *out, size_t out_cap, const char *app_id)
{
	size_t len = 0;

	if (out == NULL || out_cap == 0 || app_id == NULL) {
		return -EINVAL;
	}
	while (len < out_cap && app_id[len] != '\0') {
		len++;
	}
	if (len == 0 || len >= out_cap) {
		return -EINVAL;
	}
	memset(out, 0, out_cap);
	memcpy(out, app_id, len);
	return 0;
}

static int append_fixed_app_id(uint8_t *out, size_t out_cap, size_t *offset, const char *app_id)
{
	if (out == NULL || offset == NULL || app_id == NULL || *offset > out_cap ||
	    out_cap - *offset < SQ_APP_STORE_APP_ID_MAX) {
		return -ENOSPC;
	}
	memcpy(&out[*offset], app_id, SQ_APP_STORE_APP_ID_MAX);
	*offset += SQ_APP_STORE_APP_ID_MAX;
	return 0;
}

static int read_fixed_app_id(const uint8_t *bytes, size_t len, size_t *offset, char *out,
			     size_t out_cap)
{
	if (bytes == NULL || offset == NULL || out == NULL || out_cap != SQ_APP_STORE_APP_ID_MAX ||
	    *offset > len || len - *offset < SQ_APP_STORE_APP_ID_MAX) {
		return -EINVAL;
	}
	memcpy(out, &bytes[*offset], SQ_APP_STORE_APP_ID_MAX);
	out[out_cap - 1] = '\0';
	*offset += SQ_APP_STORE_APP_ID_MAX;
	return 0;
}

int sq_device_protocol_encode_planned_resume(
	const struct sq_device_planned_resume_record *record, uint8_t *out, size_t out_cap,
	size_t *out_len)
{
	size_t offset = 0;

	if (record == NULL || out == NULL || out_len == NULL) {
		return -EINVAL;
	}
	if (out_cap < SQ_DEVICE_PLANNED_RESUME_LEN ||
	    record->return_stack_count > SQ_VM_RUNTIME_RETURN_STACK_MAX ||
	    record->armed_app_count > SQ_VM_RUNTIME_ARMED_TIMER_MAX ||
	    record->current_app[0] == '\0') {
		return -EINVAL;
	}
	memcpy(out, SQ_DEVICE_PLANNED_RESUME_MAGIC, 4);
	offset += 4;
	out[offset++] = SQ_DEVICE_PLANNED_RESUME_VERSION;
	if (append_fixed_app_id(out, out_cap, &offset, record->current_app) != 0) {
		return -EINVAL;
	}
	out[offset++] = record->return_stack_count;
	for (size_t i = 0; i < SQ_VM_RUNTIME_RETURN_STACK_MAX; i++) {
		if (append_fixed_app_id(out, out_cap, &offset, record->return_stack[i]) != 0) {
			return -EINVAL;
		}
	}
	out[offset++] = record->armed_app_count;
	for (size_t i = 0; i < SQ_VM_RUNTIME_ARMED_TIMER_MAX; i++) {
		if (append_fixed_app_id(out, out_cap, &offset, record->armed_apps[i]) != 0) {
			return -EINVAL;
		}
	}
	*out_len = offset;
	return 0;
}

int sq_device_protocol_decode_planned_resume(const uint8_t *bytes, size_t len,
					     struct sq_device_planned_resume_record *out)
{
	size_t offset = 0;

	if (bytes == NULL || out == NULL || len != SQ_DEVICE_PLANNED_RESUME_LEN) {
		return -EINVAL;
	}
	if (memcmp(bytes, SQ_DEVICE_PLANNED_RESUME_MAGIC, 4) != 0 ||
	    bytes[4] != SQ_DEVICE_PLANNED_RESUME_VERSION) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
	offset = 5;
	if (read_fixed_app_id(bytes, len, &offset, out->current_app, sizeof(out->current_app)) !=
	    0 ||
	    out->current_app[0] == '\0') {
		return -EINVAL;
	}
	out->return_stack_count = bytes[offset++];
	if (out->return_stack_count > SQ_VM_RUNTIME_RETURN_STACK_MAX) {
		return -EINVAL;
	}
	for (size_t i = 0; i < SQ_VM_RUNTIME_RETURN_STACK_MAX; i++) {
		if (read_fixed_app_id(bytes, len, &offset, out->return_stack[i],
				      sizeof(out->return_stack[i])) != 0) {
			return -EINVAL;
		}
		if (i < out->return_stack_count && out->return_stack[i][0] == '\0') {
			return -EINVAL;
		}
	}
	out->armed_app_count = bytes[offset++];
	if (out->armed_app_count > SQ_VM_RUNTIME_ARMED_TIMER_MAX) {
		return -EINVAL;
	}
	for (size_t i = 0; i < SQ_VM_RUNTIME_ARMED_TIMER_MAX; i++) {
		if (read_fixed_app_id(bytes, len, &offset, out->armed_apps[i],
				      sizeof(out->armed_apps[i])) != 0) {
			return -EINVAL;
		}
		if (i < out->armed_app_count && out->armed_apps[i][0] == '\0') {
			return -EINVAL;
		}
	}
	return offset == len ? 0 : -EINVAL;
}

int sq_device_protocol_planned_resume_from_runtime(
	const struct sq_vm_runtime *runtime, struct sq_device_planned_resume_record *out)
{
	if (runtime == NULL || out == NULL || runtime->current_app[0] == '\0') {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
	if (copy_app_id(out->current_app, sizeof(out->current_app), runtime->current_app) != 0) {
		return -EINVAL;
	}
	out->return_stack_count = runtime->return_stack_count;
	for (size_t i = 0; i < runtime->return_stack_count; i++) {
		if (copy_app_id(out->return_stack[i], sizeof(out->return_stack[i]),
				runtime->return_stack[i]) != 0) {
			return -EINVAL;
		}
	}
	for (size_t i = 0; i < SQ_VM_RUNTIME_ARMED_TIMER_MAX; i++) {
		const struct sq_vm_runtime_armed_timer *timer = &runtime->armed_timers[i];
		bool duplicate = false;

		if (!timer->active || timer->app_id[0] == '\0') {
			continue;
		}
		for (size_t j = 0; j < out->armed_app_count; j++) {
			if (strncmp(out->armed_apps[j], timer->app_id, SQ_APP_STORE_APP_ID_MAX) ==
			    0) {
				duplicate = true;
				break;
			}
		}
		if (duplicate) {
			continue;
		}
		if (out->armed_app_count >= SQ_VM_RUNTIME_ARMED_TIMER_MAX ||
		    copy_app_id(out->armed_apps[out->armed_app_count],
				sizeof(out->armed_apps[out->armed_app_count]), timer->app_id) !=
			    0) {
			return -EINVAL;
		}
		out->armed_app_count++;
	}
	return 0;
}

__weak int sq_device_protocol_enter_planned_sleep(int32_t wake_after_ms)
{
	ARG_UNUSED(wake_after_ms);
	return 0;
}

static int protocol_scratch_acquire(const struct sq_device_protocol_context *context,
				    enum sq_device_protocol_scratch_owner owner)
{
	if (context == NULL || context->scratch == NULL ||
	    owner == SQ_DEVICE_PROTOCOL_SCRATCH_FREE) {
		return -EINVAL;
	}
	if (context->scratch->owner != SQ_DEVICE_PROTOCOL_SCRATCH_FREE) {
		return -EBUSY;
	}
	memset(context->scratch, 0, sizeof(*context->scratch));
	context->scratch->owner = owner;
	return 0;
}

static int protocol_scratch_release(const struct sq_device_protocol_context *context,
				    enum sq_device_protocol_scratch_owner owner)
{
	if (context == NULL || context->scratch == NULL ||
	    owner == SQ_DEVICE_PROTOCOL_SCRATCH_FREE) {
		return -EINVAL;
	}
	if (context->scratch->owner != owner) {
		return -EBUSY;
	}
	memset(context->scratch, 0, sizeof(*context->scratch));
	return 0;
}

static int write_planned_resume_file(const struct sq_device_protocol_context *context)
{
	struct sq_device_protocol_scratch *scratch;
	ssize_t written;
	int result;

	if (context == NULL || context->runtime == NULL || context->store_mount_point == NULL) {
		return -EINVAL;
	}
	result = protocol_scratch_acquire(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);
	if (result != 0) {
		return result;
	}
	scratch = context->scratch;
	result = sq_device_protocol_planned_resume_from_runtime(context->runtime,
							       &scratch->planned_resume_record);
	if (result != 0) {
		(void)protocol_scratch_release(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);
		return result;
	}
	result = sq_device_protocol_encode_planned_resume(
		&scratch->planned_resume_record, scratch->planned_resume_bytes,
		sizeof(scratch->planned_resume_bytes), &scratch->planned_resume_len);
	if (result != 0) {
		(void)protocol_scratch_release(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);
		return result;
	}
	result = sq_app_store_planned_resume_temp_path(
		context->store_mount_point, scratch->planned_resume_temp_path,
		sizeof(scratch->planned_resume_temp_path));
	if (result != 0) {
		(void)protocol_scratch_release(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);
		return result;
	}
	result = sq_app_store_planned_resume_path(
		context->store_mount_point, scratch->planned_resume_final_path,
		sizeof(scratch->planned_resume_final_path));
	if (result != 0) {
		(void)protocol_scratch_release(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);
		return result;
	}
	fs_file_t_init(&scratch->planned_resume_file);
	result = fs_open(&scratch->planned_resume_file, scratch->planned_resume_temp_path,
			 FS_O_CREATE | FS_O_WRITE | FS_O_TRUNC);
	if (result != 0) {
		(void)protocol_scratch_release(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);
		return result;
	}
	written = fs_write(&scratch->planned_resume_file, scratch->planned_resume_bytes,
			   scratch->planned_resume_len);
	result = fs_close(&scratch->planned_resume_file);
	if (written < 0) {
		(void)protocol_scratch_release(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);
		return (int)written;
	}
	if (result != 0) {
		(void)protocol_scratch_release(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);
		return result;
	}
	if ((size_t)written != scratch->planned_resume_len) {
		(void)protocol_scratch_release(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);
		return -EIO;
	}
	result = fs_unlink(scratch->planned_resume_final_path);
	if (result != 0 && result != -ENOENT) {
		(void)protocol_scratch_release(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);
		return result;
	}
	result = fs_rename(scratch->planned_resume_temp_path,
			   scratch->planned_resume_final_path);
	(void)protocol_scratch_release(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);
	return result;
}

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

static void write_u32_le_device(uint8_t *out, uint32_t value);

static int hello_response(const struct sq_protocol_request *request,
			  const struct sq_device_identity *identity, uint8_t *response,
			  size_t response_cap, size_t *response_len)
{
	return sqdp_status_to_protocol_result(sqdp_encode_hello_response(
		SQ_OPCODE_HELLO, request->sequence, (const uint8_t *)identity->target,
		strlen(identity->target), (const uint8_t *)identity->firmware,
		strlen(identity->firmware), identity->diagnostic, response, response_cap,
		response_len));
}

static int app_list_response(const struct sq_protocol_request *request,
			     const struct sq_app_registry *registry, uint8_t *response,
			     size_t response_cap, size_t *response_len)
{
	const SqdpAppListEntry *entries =
		registry == NULL ? NULL : (const SqdpAppListEntry *)registry->apps;
	size_t entry_count = registry == NULL ? 0 : registry->count;

	return sqdp_status_to_protocol_result(sqdp_encode_app_list_response(
		request->sequence, entries, entry_count, response, response_cap, response_len));
}

static int append_line_payload(uint8_t *payload, size_t payload_cap, size_t *payload_len,
			       const char *line);

static int ok_response(const struct sq_protocol_request *request, uint8_t *response,
		       size_t response_cap, size_t *response_len)
{
	if (request == NULL || response == NULL || response_len == NULL ||
	    response_cap < SQ_PROTOCOL_HEADER_LEN) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	memcpy(response, "SQDP", 4);
	response[4] = SQ_FRAME_RESPONSE;
	response[5] = request->opcode;
	response[6] = SQ_STATUS_OK;
	response[7] = 0;
	write_u32_le_device(&response[8], request->sequence);
	write_u32_le_device(&response[12], 0);
	write_u32_le_device(&response[16], sq_protocol_crc32(NULL, 0));
	*response_len = SQ_PROTOCOL_HEADER_LEN;
	return SQ_PROTOCOL_OK;
}

static int pending_line_response(const struct sq_protocol_request *request, const char *line,
				 uint8_t *response, size_t response_cap, size_t *response_len)
{
	size_t payload_len = 0;
	uint8_t *payload;

	if (request == NULL || response == NULL || response_len == NULL ||
	    response_cap < SQ_PROTOCOL_HEADER_LEN) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	payload = &response[SQ_PROTOCOL_HEADER_LEN];
	if (append_line_payload(payload, response_cap - SQ_PROTOCOL_HEADER_LEN, &payload_len,
				line) != SQ_PROTOCOL_OK) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	memcpy(response, "SQDP", 4);
	response[4] = SQ_FRAME_RESPONSE;
	response[5] = request->opcode;
	response[6] = SQ_STATUS_PENDING;
	response[7] = 0;
	write_u32_le_device(&response[8], request->sequence);
	write_u32_le_device(&response[12], (uint32_t)payload_len);
	write_u32_le_device(&response[16], sq_protocol_crc32(payload, payload_len));
	*response_len = SQ_PROTOCOL_HEADER_LEN + payload_len;
	return SQ_PROTOCOL_OK;
}

static int error_response(const struct sq_protocol_request *request, int code, uint8_t *response,
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

static uint32_t read_u32_le_device(const uint8_t *bytes)
{
	return (uint32_t)bytes[0] | ((uint32_t)bytes[1] << 8) |
	       ((uint32_t)bytes[2] << 16) | ((uint32_t)bytes[3] << 24);
}

static int append_line_payload(uint8_t *payload, size_t payload_cap, size_t *payload_len,
			       const char *line)
{
	size_t line_len;
	size_t needed;

	if (payload == NULL || payload_len == NULL || line == NULL) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	line_len = strlen(line);
	if (line_len > UINT16_MAX) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	needed = *payload_len + 4u + line_len;
	if (needed > payload_cap) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	payload[*payload_len] = 1u;
	payload[*payload_len + 1u] = SQ_DEVICE_FIELD_TYPE_STRING;
	payload[*payload_len + 2u] = line_len & 0xffu;
	payload[*payload_len + 3u] = (line_len >> 8) & 0xffu;
	memcpy(&payload[*payload_len + 4u], line, line_len);
	*payload_len = needed;
	return SQ_PROTOCOL_OK;
}

static int append_resource_metric(uint8_t *payload, size_t payload_cap, size_t *payload_len,
				  const char *key, uint64_t value)
{
	size_t key_len;
	size_t record_len;
	size_t needed;
	uint8_t *record;
	uint8_t *value_field;

	if (payload == NULL || payload_len == NULL || key == NULL || value > UINT32_MAX) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	key_len = strlen(key);
	if (key_len > UINT16_MAX || key_len > UINT16_MAX - 12u) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	record_len = 12u + key_len;
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
	value_field[1] = SQ_DEVICE_FIELD_TYPE_U32;
	value_field[2] = 4u;
	value_field[3] = 0u;
	for (size_t i = 0; i < 4u; i++) {
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

static int encode_lifecycle_header(uint32_t sequence, uint8_t *response, size_t response_cap,
				   size_t payload_len, size_t *response_len)
{
	uint8_t *payload;

	if (response == NULL || response_len == NULL || response_cap < SQ_PROTOCOL_HEADER_LEN ||
	    payload_len > response_cap - SQ_PROTOCOL_HEADER_LEN || payload_len > UINT32_MAX) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	payload = &response[SQ_PROTOCOL_HEADER_LEN];
	memcpy(response, "SQDP", 4);
	response[4] = SQ_FRAME_RESPONSE;
	response[5] = SQ_OPCODE_LIFECYCLE_GET;
	response[6] = SQ_STATUS_OK;
	response[7] = 0;
	write_u32_le_device(&response[8], sequence);
	write_u32_le_device(&response[12], (uint32_t)payload_len);
	write_u32_le_device(&response[16], sq_protocol_crc32(payload, payload_len));
	*response_len = SQ_PROTOCOL_HEADER_LEN + payload_len;
	return SQ_PROTOCOL_OK;
}

static int __noinline begin_install(const struct sq_protocol_request *request,
			 const uint8_t *request_bytes, size_t request_len,
			 const struct sq_device_protocol_context *context, uint8_t *response,
			 size_t response_cap, size_t *response_len)
{
	if (request->opcode == SQ_OPCODE_TEMP_RUN_BEGIN) {
		struct sq_device_temp_session *session = context->temp_session;

		if (session == NULL || context->store_mount_point == NULL) {
			return -ENODEV;
		}
		if (sqdp_prepare_transfer_begin(request_bytes, request_len, session, NULL) !=
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
		transfer_session_begin_receiving(session);
		return ok_response(request, response, response_cap, response_len);
	}

	struct sq_device_install_session *session = context->install_session;

	if (session == NULL || context->store_mount_point == NULL) {
		return -ENODEV;
	}
	if (sqdp_prepare_transfer_begin(request_bytes, request_len, session, NULL) !=
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
	transfer_session_begin_receiving(session);

	return ok_response(request, response, response_cap, response_len);
}

static int __noinline append_install_chunk(const struct sq_protocol_request *request,
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

static int __noinline begin_resource_install(const struct sq_protocol_request *request,
				  const uint8_t *request_bytes, size_t request_len,
				  const struct sq_device_protocol_context *context,
				  uint8_t *response, size_t response_cap, size_t *response_len)
{
	struct sq_device_resource_session *session = context->resource_session;
	if (session == NULL || context->store_mount_point == NULL) {
		return -ENODEV;
	}
	if (sqdp_prepare_resource_begin(request_bytes, request_len, session, NULL) !=
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
	transfer_session_begin_receiving(session);
	return ok_response(request, response, response_cap, response_len);
}

static int __noinline append_resource_chunk(const struct sq_protocol_request *request,
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

static int __noinline commit_resource_install(const struct sq_protocol_request *request,
				   const uint8_t *request_bytes, size_t request_len,
				   const struct sq_device_protocol_context *context,
				   uint8_t *response, size_t response_cap, size_t *response_len)
{
	struct sq_device_resource_session *session = context->resource_session;
	if (session == NULL || context->store_mount_point == NULL ||
	    sqdp_prepare_resource_commit(request_bytes, request_len, session, NULL) !=
		    SQDP_STATUS_OK) {
		return -EINVAL;
	}
	if (transfer_session_begin_committing(session) != 0) {
		return -EINVAL;
	}

	int result = sq_app_store_commit_staged_resource_with_path(
		context->store_mount_point, session->app_id, session->resource_path,
		session->staging_path, (char *)response, response_cap);
	if (result != 0) {
		return result;
	}
	sqdp_clear_resource_session(session);
	transfer_session_finish_idle(session);
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

static int __noinline commit_temp_run(const struct sq_protocol_request *request,
			   const uint8_t *request_bytes, size_t request_len,
			   const struct sq_device_protocol_context *context, uint8_t *response,
			   size_t response_cap, size_t *response_len)
{
	struct sq_device_temp_session *session = context->temp_session;
	static struct temp_storage_backend temp_storage;
	int result;

	if (session == NULL || context->runtime == NULL || context->store_mount_point == NULL ||
	    sqdp_prepare_transfer_commit(request_bytes, request_len, session, NULL) !=
		    SQDP_STATUS_OK) {
		return -EINVAL;
	}
	if (transfer_session_begin_committing(session) != 0) {
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
	context->runtime->job_backend = sq_vm_fs_storage_backend(&temp_storage.fs_storage);
	result = context->runtime->job_backend.reset_state(context->runtime->job_backend.user_data);
	if (result != 0) {
		return result;
	}
	result = sq_vm_runtime_start_event(context->runtime, &context->runtime->job_backend,
					   (const uint8_t *)"app.start", sizeof("app.start") - 1);
	if (result != 0) {
		return result;
	}

	transfer_session_finish_idle(session);
	return ok_response(request, response, response_cap, response_len);
}

static int __noinline commit_install(const struct sq_protocol_request *request,
			  const uint8_t *request_bytes, size_t request_len,
			  const struct sq_device_protocol_context *context, uint8_t *response,
			  size_t response_cap, size_t *response_len)
{
	struct sq_device_install_session *session = context->install_session;
	int result;

	if (session == NULL || context->store_mount_point == NULL ||
	    sqdp_prepare_transfer_commit(request_bytes, request_len, session, NULL) !=
		    SQDP_STATUS_OK) {
		return -EINVAL;
	}
	if (transfer_session_begin_committing(session) != 0) {
		return -EINVAL;
	}

	result = sq_app_store_commit_staged_install(context->store_mount_point, session->app_id,
						   session->staging_path);
	if (result != 0) {
		return result;
	}
	if (context->mutable_registry != NULL) {
		result = sq_app_store_update_registry_entry_with_path(
			context->store_mount_point, context->mutable_registry, session->app_id,
			session->staging_path, sizeof(session->staging_path));
		if (result != 0) {
			return result;
		}
	}

	sqdp_clear_transfer_session(session);
	transfer_session_finish_idle(session);
	return ok_response(request, response, response_cap, response_len);
}

static int start_installed_app_bytes(const struct sq_device_protocol_context *context,
				     const uint8_t *app_id, size_t app_id_len,
				     const uint8_t *event, size_t event_len, bool set_current);
static int start_resolved_app(const struct sq_device_protocol_context *context,
			      const char *app_id, const uint8_t *event, size_t event_len,
			      bool set_current);
static int start_resolved_app_bytes(const struct sq_device_protocol_context *context,
				    const uint8_t *app_id, size_t app_id_len,
				    const uint8_t *event, size_t event_len, bool set_current);
static bool is_main_app_id(const uint8_t *app_id, size_t app_id_len);
static void clear_foreground_timers(struct sq_vm_runtime *runtime);

static int __noinline launch_app(const struct sq_protocol_request *request,
		      const uint8_t *request_bytes, size_t request_len,
		      const struct sq_device_protocol_context *context, uint8_t *response,
		      size_t response_cap, size_t *response_len)
{
	SqdpAppLaunch launch = {0};

	if (context->runtime == NULL || context->store_mount_point == NULL ||
	    context->launch_storage == NULL) {
		return -ENODEV;
	}

	if (sqdp_parse_app_launch_request(request_bytes, request_len, &launch) != SQDP_STATUS_OK ||
	    launch.app_id_len >= SQ_APP_STORE_APP_ID_MAX) {
		return -EINVAL;
	}

	int result = sq_app_lifecycle_request_launch(context->runtime, launch.app_id,
						     launch.app_id_len);
	if (result != 0) {
		return result;
	}

	return ok_response(request, response, response_cap, response_len);
}

static int start_installed_app_bytes(const struct sq_device_protocol_context *context,
				     const uint8_t *app_id, size_t app_id_len,
				     const uint8_t *event, size_t event_len, bool set_current)
{
	int result;
	bool current_app_changed;

	if (context == NULL || context->runtime == NULL || context->store_mount_point == NULL ||
	    context->launch_storage == NULL || app_id == NULL || event == NULL) {
		return -EINVAL;
	}
	if (app_id_len >= SQ_APP_STORE_APP_ID_MAX) {
		return -EINVAL;
	}
	current_app_changed = set_current &&
			      (strlen(context->runtime->current_app) != app_id_len ||
			       memcmp(context->runtime->current_app, app_id, app_id_len) != 0);
	if (current_app_changed) {
		clear_foreground_timers(context->runtime);
	}
	if (set_current || strlen(context->runtime->current_app) != app_id_len ||
	    memcmp(context->runtime->current_app, app_id, app_id_len) != 0) {
		result = sq_vm_runtime_wait_idle(context->runtime, 250);
		if (result != 0) {
			return result;
		}
		sq_vm_runtime_reset_vm_context(context->runtime);
	}

	result = sq_app_store_vm_storage_for_app_bytes(context->store_mount_point, app_id,
						       app_id_len, context->launch_storage);
	if (result != 0) {
		return result;
	}
	context->runtime->job_backend = sq_app_store_vm_storage_backend(context->launch_storage);
	if (set_current) {
		strncpy(context->runtime->lifecycle_previous_app, context->runtime->current_app,
			sizeof(context->runtime->lifecycle_previous_app) - 1);
		context->runtime
			->lifecycle_previous_app[sizeof(context->runtime->lifecycle_previous_app) -
						 1] = '\0';
		memcpy(context->runtime->current_app, app_id, app_id_len);
		context->runtime->current_app[app_id_len] = '\0';
	}
	result = sq_vm_runtime_start_event(context->runtime, &context->runtime->job_backend, event,
					   event_len);
	if (result != 0) {
		if (set_current) {
			strncpy(context->runtime->current_app,
				context->runtime->lifecycle_previous_app,
				sizeof(context->runtime->current_app) - 1);
			context->runtime->current_app[sizeof(context->runtime->current_app) - 1] =
				'\0';
			memset(context->runtime->lifecycle_previous_app, 0,
			       sizeof(context->runtime->lifecycle_previous_app));
		}
		return result;
	}
	if (set_current) {
		memset(context->runtime->lifecycle_previous_app, 0,
		       sizeof(context->runtime->lifecycle_previous_app));
	}
	return 0;
}

static bool is_main_app_id(const uint8_t *app_id, size_t app_id_len)
{
	return app_id != NULL && app_id_len == 4u && memcmp(app_id, "main", 4u) == 0;
}

static bool installed_main_exists(const struct sq_device_protocol_context *context)
{
	return context != NULL && context->registry != NULL &&
	       sq_app_registry_find(context->registry, "main") != NULL;
}

static int start_fallback_app(const struct sq_device_protocol_context *context,
			      const uint8_t *event, size_t event_len, bool set_current)
{
	struct sq_vm_storage_backend backend;
	bool current_app_changed;
	int result;

	if (context == NULL || context->runtime == NULL || context->fallback_app == NULL ||
	    context->fallback_app->app_id == NULL ||
	    strcmp(context->fallback_app->app_id, "main") != 0 || event == NULL) {
		return -EINVAL;
	}
	current_app_changed = set_current && strcmp(context->runtime->current_app, "main") != 0;
	if (current_app_changed) {
		clear_foreground_timers(context->runtime);
	}
	if (set_current || strcmp(context->runtime->current_app, "main") != 0) {
		result = sq_vm_runtime_wait_idle(context->runtime, 250);
		if (result != 0) {
			return result;
		}
		sq_vm_runtime_reset_vm_context(context->runtime);
	}

	backend = sq_firmware_fallback_app_backend(context->fallback_app);
	context->runtime->job_backend = backend;
	if (set_current) {
		strncpy(context->runtime->lifecycle_previous_app, context->runtime->current_app,
			sizeof(context->runtime->lifecycle_previous_app) - 1);
		context->runtime
			->lifecycle_previous_app[sizeof(context->runtime->lifecycle_previous_app) -
						 1] = '\0';
		strncpy(context->runtime->current_app, "main",
			sizeof(context->runtime->current_app) - 1);
		context->runtime->current_app[sizeof(context->runtime->current_app) - 1] = '\0';
	}
	result = sq_vm_runtime_start_event(context->runtime, &context->runtime->job_backend, event,
					   event_len);
	if (result != 0) {
		if (set_current) {
			strncpy(context->runtime->current_app,
				context->runtime->lifecycle_previous_app,
				sizeof(context->runtime->current_app) - 1);
			context->runtime->current_app[sizeof(context->runtime->current_app) - 1] =
				'\0';
			memset(context->runtime->lifecycle_previous_app, 0,
			       sizeof(context->runtime->lifecycle_previous_app));
		}
		return result;
	}
	if (set_current) {
		memset(context->runtime->lifecycle_previous_app, 0,
		       sizeof(context->runtime->lifecycle_previous_app));
	}
	return 0;
}

static int start_resolved_app(const struct sq_device_protocol_context *context,
			      const char *app_id, const uint8_t *event, size_t event_len,
			      bool set_current)
{
	if (app_id == NULL) {
		return -EINVAL;
	}
	return start_resolved_app_bytes(context, (const uint8_t *)app_id, strlen(app_id), event,
					event_len, set_current);
}

static int start_resolved_app_bytes(const struct sq_device_protocol_context *context,
				    const uint8_t *app_id, size_t app_id_len,
				    const uint8_t *event, size_t event_len, bool set_current)
{
	if (is_main_app_id(app_id, app_id_len) && !installed_main_exists(context)) {
		return start_fallback_app(context, event, event_len, set_current);
	}
	return start_installed_app_bytes(context, app_id, app_id_len, event, event_len,
					 set_current);
}

static void clear_foreground_timers(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL) {
		return;
	}
	memset(runtime->timers, 0, sizeof(runtime->timers));
}

static size_t c_array_len(const uint8_t *bytes, size_t cap)
{
	size_t len = 0;

	while (len < cap && bytes[len] != 0) {
		len++;
	}
	return len;
}

static int __noinline register_app_trigger_timer(struct sq_vm_runtime *runtime,
						 const struct sq_vm_storage_backend *backend,
						 const char *app_id, size_t index)
{
	SqvmTriggerTimer timer = {0};
	SqvmStatus status;
	size_t event_len;

	if (runtime == NULL || backend == NULL || backend->read_sqbc == NULL || app_id == NULL) {
		return -EINVAL;
	}
	int transfer_result =
		sq_vm_runtime_transfer_acquire(runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH);
	if (transfer_result != 0) {
		return transfer_result;
	}
	status = sqvm_trigger_timer_read_from_reader(
		backend->user_data, backend->read_sqbc, runtime->transfer.init_scratch,
		sizeof(runtime->transfer.init_scratch), index, &timer);
	transfer_result =
		sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH);
	if (transfer_result != 0) {
		return transfer_result;
	}
	if (status != SQVM_STATUS_OK) {
		return -EINVAL;
	}
	event_len = c_array_len(timer.event, sizeof(timer.event));
	return sq_vm_runtime_register_armed_timer(runtime, app_id, timer.event, event_len,
						  timer.interval_ms, timer.repeating);
}

static int __noinline register_app_triggers(const struct sq_device_protocol_context *context,
					    const char *app_id)
{
	struct sq_vm_storage_backend backend;
	struct sq_app_store_vm_storage *trigger_storage;
	size_t trigger_count = 0;
	SqvmStatus status;
	int result;

	if (context == NULL || context->runtime == NULL || context->store_mount_point == NULL ||
	    context->trigger_storage == NULL || app_id == NULL) {
		return -EINVAL;
	}
	trigger_storage = context->trigger_storage;
	result = sq_app_store_vm_storage_for_app(context->store_mount_point, app_id,
						 trigger_storage);
	if (result != 0) {
		return result;
	}
	backend = sq_app_store_vm_storage_backend(trigger_storage);
	if (backend.read_sqbc == NULL) {
		return -ENODEV;
	}

	result = sq_vm_runtime_clear_armed_app(context->runtime, (const uint8_t *)app_id,
					      strlen(app_id));
	if (result != 0) {
		return result;
	}
	result = sq_vm_runtime_transfer_acquire(context->runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH);
	if (result != 0) {
		return result;
	}
	status = sqvm_trigger_timer_count_from_reader(
		backend.user_data, backend.read_sqbc, context->runtime->transfer.init_scratch,
		sizeof(context->runtime->transfer.init_scratch), &trigger_count);
	result = sq_vm_runtime_transfer_release(context->runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH);
	if (result != 0) {
		return result;
	}
	if (status != SQVM_STATUS_OK || trigger_count > SQ_VM_RUNTIME_ARMED_TIMER_MAX) {
		return -EINVAL;
	}
	for (size_t i = 0; i < trigger_count; i++) {
		result = register_app_trigger_timer(context->runtime, &backend, app_id, i);
		if (result != 0) {
			return result;
		}
	}
	return 0;
}

int sq_device_protocol_start_root(const struct sq_device_protocol_context *context)
{
	if (context == NULL || context->runtime == NULL || context->store_mount_point == NULL ||
	    context->launch_storage == NULL) {
		return -EINVAL;
	}
	return start_resolved_app(context, "main", (const uint8_t *)"app.start",
				  sizeof("app.start") - 1, true);
}

int sq_device_protocol_restore_planned_resume(const struct sq_device_protocol_context *context)
{
	struct sq_device_protocol_scratch *scratch;
	ssize_t read_len;
	int result;

	if (context == NULL || context->runtime == NULL || context->store_mount_point == NULL) {
		return -EINVAL;
	}
	result = protocol_scratch_acquire(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);
	if (result != 0) {
		return result;
	}
	scratch = context->scratch;
	result = sq_app_store_planned_resume_path(
		context->store_mount_point, scratch->planned_resume_final_path,
		sizeof(scratch->planned_resume_final_path));
	if (result != 0) {
		(void)protocol_scratch_release(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);
		return result;
	}
	fs_file_t_init(&scratch->planned_resume_file);
	result = fs_open(&scratch->planned_resume_file, scratch->planned_resume_final_path,
			 FS_O_READ);
	if (result == -ENOENT) {
		(void)protocol_scratch_release(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);
		return -ENOENT;
	}
	if (result != 0) {
		(void)protocol_scratch_release(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);
		return result;
	}
	read_len = fs_read(&scratch->planned_resume_file, scratch->planned_resume_bytes,
			   sizeof(scratch->planned_resume_bytes));
	result = fs_close(&scratch->planned_resume_file);
	if (read_len < 0) {
		(void)protocol_scratch_release(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);
		return (int)read_len;
	}
	if (result != 0) {
		(void)protocol_scratch_release(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);
		return result;
	}
	result = sq_device_protocol_decode_planned_resume(
		scratch->planned_resume_bytes, (size_t)read_len,
		&scratch->planned_resume_record);
	if (result != 0) {
		(void)fs_unlink(scratch->planned_resume_final_path);
		(void)protocol_scratch_release(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);
		return result;
	}
	(void)fs_unlink(scratch->planned_resume_final_path);
	result = sq_app_lifecycle_restore_planned_route(
		context->runtime, scratch->planned_resume_record.return_stack,
		scratch->planned_resume_record.return_stack_count);
	if (result != 0) {
		(void)protocol_scratch_release(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);
		return result;
	}
	for (size_t i = 0; i < scratch->planned_resume_record.armed_app_count; i++) {
		result = register_app_triggers(context, scratch->planned_resume_record.armed_apps[i]);
		if (result != 0) {
			sq_vm_runtime_record_trace(
				context->runtime,
				(const uint8_t *)"planned resume armed app restore failed",
				sizeof("planned resume armed app restore failed") - 1);
		}
	}
	result = start_resolved_app(context, scratch->planned_resume_record.current_app,
				    (const uint8_t *)"app.start", sizeof("app.start") - 1, true);
	(void)protocol_scratch_release(context, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);
	if (result != 0) {
		sq_vm_runtime_record_trace(context->runtime,
					   (const uint8_t *)"planned resume app missing",
					   sizeof("planned resume app missing") - 1);
		memset(context->runtime->start_reason, 0, sizeof(context->runtime->start_reason));
		strncpy(context->runtime->start_reason, "boot",
			sizeof(context->runtime->start_reason) - 1);
		return result;
	}
	return 0;
}

int sq_device_protocol_poll(const struct sq_device_protocol_context *context)
{
	struct sq_vm_runtime *runtime;
	struct sq_app_lifecycle_step step;
	char due_app[SQ_APP_STORE_APP_ID_MAX] = {0};
	char due_event[SQ_VM_RUNTIME_EVENT_LEN] = {0};
	const char *due_app_ptr = NULL;
	const char *due_event_ptr = NULL;
	int result;

	if (context == NULL || context->runtime == NULL) {
		return -EINVAL;
	}
	runtime = context->runtime;

	if (runtime->status == SQ_VM_RUNTIME_RUNNING) {
		return sq_vm_runtime_poll(runtime);
	}

	if (runtime->lifecycle_phase == SQ_VM_RUNTIME_LIFECYCLE_IDLE &&
	    runtime->arm_phase == SQ_VM_RUNTIME_ARM_IDLE &&
	    sq_vm_runtime_next_due_armed_timer(runtime, due_app, sizeof(due_app), due_event,
					      sizeof(due_event)) == 0) {
		due_app_ptr = due_app;
		due_event_ptr = due_event;
	}

	result = sq_app_lifecycle_next_step(runtime, due_app_ptr, due_event_ptr, &step);
	if (result != 0) {
		return result;
	}

	switch (step.kind) {
	case SQ_APP_LIFECYCLE_STEP_WRITE_SLEEP_CHECKPOINT:
		result = write_planned_resume_file(context);
		if (result != 0) {
			sq_vm_runtime_record_trace(
				runtime, (const uint8_t *)"planned sleep checkpoint failed",
				sizeof("planned sleep checkpoint failed") - 1);
			return result;
		}
		runtime->planned_sleep_ready = true;
		sq_vm_runtime_record_trace(runtime,
					   (const uint8_t *)"planned sleep checkpoint saved",
					   sizeof("planned sleep checkpoint saved") - 1);
		result = sq_device_protocol_enter_planned_sleep(runtime->planned_sleep_wake_after_ms);
		if (result != 0) {
			sq_vm_runtime_record_trace(runtime,
						   (const uint8_t *)"planned sleep enter failed",
						   sizeof("planned sleep enter failed") - 1);
			return result;
		}
		return 0;

	case SQ_APP_LIFECYCLE_STEP_START_APP:
		result = start_resolved_app(context, step.app_id, (const uint8_t *)step.event,
					    strlen(step.event), step.set_current);
		if (result != 0) {
			sq_app_lifecycle_cancel_pending_after_start_failure(runtime, result);
		}
		return result;

	case SQ_APP_LIFECYCLE_STEP_REGISTER_ARMED_APP:
		return register_app_triggers(context, step.app_id);

	case SQ_APP_LIFECYCLE_STEP_POLL_RUNTIME:
		return sq_vm_runtime_poll(runtime);

	case SQ_APP_LIFECYCLE_STEP_NONE:
		return 0;
	}

	return -EINVAL;
}

static int repeated_runtime_lines_response(const struct sq_protocol_request *request,
					   const struct sq_vm_runtime *runtime,
					   const char *const *extra_lines, size_t extra_count,
					   uint8_t *response, size_t response_cap,
					   size_t *response_len)
{
	const uint8_t *fixed_lines = NULL;
	size_t fixed_count = 0;
	size_t fixed_stride = 0;
	SqdpLineSlice extra_slices[1 + SQ_VM_RUNTIME_DEVICE_ERROR_MAX];
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

static int __noinline lifecycle_response(const struct sq_protocol_request *request,
			      const struct sq_vm_runtime *runtime, uint8_t *response,
			      size_t response_cap, size_t *response_len)
{
	uint8_t *payload;
	size_t payload_cap;
	size_t payload_len = 0;
	char line[96];
	int written;
	size_t armed_line_index = 0;
	int result;

	if (request == NULL || response == NULL || response_len == NULL ||
	    response_cap < SQ_PROTOCOL_HEADER_LEN) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	payload = &response[SQ_PROTOCOL_HEADER_LEN];
	payload_cap = response_cap - SQ_PROTOCOL_HEADER_LEN;

	written = snprintf(line, sizeof(line), "active=%s",
			   runtime == NULL ? "" : runtime->current_app);
	if (written < 0 || (size_t)written >= sizeof(line)) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	result = append_line_payload(payload, payload_cap, &payload_len, line);
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	if (runtime != NULL) {
		for (size_t i = 0; i < runtime->return_stack_count; i++) {
			written = snprintf(line, sizeof(line), "process_stack[%zu]=%s", i,
					   runtime->return_stack[i]);
			if (written < 0 || (size_t)written >= sizeof(line)) {
				return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
			}
			result = append_line_payload(payload, payload_cap, &payload_len, line);
			if (result != SQ_PROTOCOL_OK) {
				return result;
			}
		}
	}
	result = append_line_payload(payload, payload_cap, &payload_len, "armed_stack=");
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	if (runtime != NULL) {
		for (size_t i = 0; i < SQ_VM_RUNTIME_ARMED_TIMER_MAX; i++) {
			if (!runtime->armed_timers[i].active) {
				continue;
			}
			written = snprintf(line, sizeof(line), "armed_stack[%zu]=%s %s",
					   armed_line_index, runtime->armed_timers[i].app_id,
					   runtime->armed_timers[i].event);
			if (written < 0 || (size_t)written >= sizeof(line)) {
				return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
			}
			result = append_line_payload(payload, payload_cap, &payload_len, line);
			if (result != SQ_PROTOCOL_OK) {
				return result;
			}
			armed_line_index++;
		}
	}
	if (runtime != NULL) {
		written = snprintf(line, sizeof(line), "lifecycle=%s",
				   sq_app_lifecycle_phase_name(runtime->lifecycle_phase));
		if (written < 0 || (size_t)written >= sizeof(line)) {
			return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
		}
		result = append_line_payload(payload, payload_cap, &payload_len, line);
		if (result != SQ_PROTOCOL_OK) {
			return result;
		}
		written = snprintf(line, sizeof(line), "arm_lifecycle=%s",
				   sq_app_lifecycle_arm_phase_name(runtime->arm_phase));
		if (written < 0 || (size_t)written >= sizeof(line)) {
			return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
		}
		result = append_line_payload(payload, payload_cap, &payload_len, line);
		if (result != SQ_PROTOCOL_OK) {
			return result;
		}
		written = snprintf(line, sizeof(line), "start_reason=%s", runtime->start_reason);
		if (written < 0 || (size_t)written >= sizeof(line)) {
			return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
		}
		result = append_line_payload(payload, payload_cap, &payload_len, line);
		if (result != SQ_PROTOCOL_OK) {
			return result;
		}
	}

	return encode_lifecycle_header(request->sequence, response, response_cap, payload_len,
				       response_len);
}

static int __noinline state_get_response(const struct sq_protocol_request *request,
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

	result = sq_vm_runtime_transfer_acquire(context->runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION);
	if (result != 0) {
		return result;
	}
	state_bytes = context->runtime->transfer.completion.bytes;
	state_cap = sizeof(context->runtime->transfer.completion.bytes);
	result = backend.load_state(backend.user_data, state_bytes, state_cap, &bytes_len);
	if (result != 0 && result != -ENOENT) {
		(void)sq_vm_runtime_transfer_release(context->runtime,
						     SQ_VM_RUNTIME_TRANSFER_COMPLETION);
		return result;
	}
	if (result == -ENOENT) {
		bytes_len = 0;
	}
	if (bytes_len > state_cap) {
		(void)sq_vm_runtime_transfer_release(context->runtime,
						     SQ_VM_RUNTIME_TRANSFER_COMPLETION);
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	result = sqdp_status_to_protocol_result(sqdp_encode_state_response(
		request->sequence, state_bytes, bytes_len, response, response_cap, response_len));
	int release_result = sq_vm_runtime_transfer_release(context->runtime,
							    SQ_VM_RUNTIME_TRANSFER_COMPLETION);
	return result != 0 ? result : release_result;
}

static int __noinline state_import(const struct sq_protocol_request *request,
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

static int resources_request_reset_heap_max(const uint8_t *request_bytes, size_t request_len,
					    bool *reset_heap_max)
{
	const uint8_t *payload;
	uint32_t payload_len;
	size_t offset = 0;

	if (request_bytes == NULL || reset_heap_max == NULL || request_len < SQ_PROTOCOL_HEADER_LEN) {
		return -EINVAL;
	}
	payload_len = read_u32_le_device(&request_bytes[12]);
	if ((size_t)payload_len > request_len - SQ_PROTOCOL_HEADER_LEN) {
		return -EINVAL;
	}
	payload = &request_bytes[SQ_PROTOCOL_HEADER_LEN];
	*reset_heap_max = false;

	while (offset < payload_len) {
		const uint8_t *field;
		uint8_t tag;
		uint8_t type;
		uint16_t len;
		size_t next_offset;

		if ((size_t)payload_len - offset < 4u) {
			return -EINVAL;
		}
		field = &payload[offset];
		tag = field[0];
		type = field[1];
		len = (uint16_t)field[2] | ((uint16_t)field[3] << 8);
		next_offset = offset + 4u + len;
		if (next_offset > payload_len) {
			return -EINVAL;
		}
		if (tag != SQ_RESOURCES_FIELD_RESET_HEAP_MAX ||
		    type != SQ_DEVICE_FIELD_TYPE_BOOL || len != 1u) {
			return -EINVAL;
		}
		if (field[4] > 1u) {
			return -EINVAL;
		}
		*reset_heap_max = field[4] != 0u;
		offset = next_offset;
	}
	return 0;
}

static int __noinline resources_response(const struct sq_protocol_request *request,
			      const uint8_t *request_bytes, size_t request_len,
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
	size_t heap_largest_free_supported = 0;
	size_t heap_largest_free_bytes = 0;
	size_t input_button_pressed_count = 0;
	size_t input_button_state = 0;
	size_t runtime_status = 0;
	size_t runtime_dispatch_started = 0;
	size_t runtime_dispatch_age_us = 0;
	size_t runtime_work_submitted = 0;
	size_t runtime_current_app_present = 0;
	size_t runtime_lifecycle_phase = 0;
	size_t runtime_arm_phase = 0;
	bool reset_heap_max = false;

	if (response == NULL || response_len == NULL || response_cap < SQ_PROTOCOL_HEADER_LEN) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	*response_len = 0;
	payload = &response[SQ_PROTOCOL_HEADER_LEN];
	payload_cap = response_cap - SQ_PROTOCOL_HEADER_LEN;
	if (resources_request_reset_heap_max(request_bytes, request_len, &reset_heap_max) != 0) {
		return -EINVAL;
	}

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

			if (reset_heap_max) {
				(void)sys_heap_runtime_stats_reset_max(&heaps[i].heap);
			}
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
		runtime_status = (size_t)context->runtime->status;
		runtime_dispatch_started = context->runtime->dispatch_started ? 1u : 0u;
		if (context->runtime->dispatch_started) {
			uint64_t dispatch_age_cycles =
				k_cycle_get_64() - context->runtime->dispatch_start_cycles;
			uint64_t dispatch_age_us = k_cyc_to_us_floor64(dispatch_age_cycles);

			runtime_dispatch_age_us =
				dispatch_age_us > UINT32_MAX ? UINT32_MAX : (size_t)dispatch_age_us;
		}
		runtime_work_submitted = context->runtime->work_submitted ? 1u : 0u;
		runtime_current_app_present = context->runtime->current_app[0] == '\0' ? 0u : 1u;
		runtime_lifecycle_phase = (size_t)context->runtime->lifecycle_phase;
		runtime_arm_phase = (size_t)context->runtime->arm_phase;
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
	SQ_RESOURCE_METRIC("heap_largest_free_supported", heap_largest_free_supported);
	SQ_RESOURCE_METRIC("heap_largest_free_bytes", heap_largest_free_bytes);
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
	SQ_RESOURCE_METRIC("runtime_status", runtime_status);
	SQ_RESOURCE_METRIC("runtime_dispatch_started", runtime_dispatch_started);
	SQ_RESOURCE_METRIC("runtime_dispatch_age_us", runtime_dispatch_age_us);
	SQ_RESOURCE_METRIC("runtime_work_submitted", runtime_work_submitted);
	SQ_RESOURCE_METRIC("runtime_current_app_present", runtime_current_app_present);
	SQ_RESOURCE_METRIC("runtime_lifecycle_phase", runtime_lifecycle_phase);
	SQ_RESOURCE_METRIC("runtime_arm_phase", runtime_arm_phase);
	SQ_RESOURCE_METRIC("proto_stack_size_bytes", protocol_stack_size);
	SQ_RESOURCE_METRIC("proto_stack_pre_unused_bytes",
			   protocol_stack_pre_resources_unused);
	SQ_RESOURCE_METRIC("proto_stack_pre_used_bytes",
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

static int clear_runtime_context(const struct sq_device_protocol_context *context)
{
	if (context->runtime != NULL) {
		int result = sq_vm_runtime_wait_idle(context->runtime, 250);

		if (result != 0) {
			return result;
		}
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
	if (context->trigger_storage != NULL) {
		memset(context->trigger_storage, 0, sizeof(*context->trigger_storage));
	}
	return 0;
}

static int __noinline reset_runtime(const struct sq_protocol_request *request,
			 const struct sq_device_protocol_context *context, uint8_t *response,
			 size_t response_cap, size_t *response_len)
{
	int result = clear_runtime_context(context);

	if (result != 0) {
		return result;
	}
	return ok_response(request, response, response_cap, response_len);
}

static int __noinline storage_format(const struct sq_protocol_request *request,
			  const struct sq_device_protocol_context *context, uint8_t *response,
			  size_t response_cap, size_t *response_len)
{
	if (context->store_mount_point == NULL) {
		return -ENODEV;
	}
	if (context->scratch == NULL) {
		return -ENODEV;
	}
	if (context->runtime != NULL && context->runtime->status == SQ_VM_RUNTIME_RUNNING) {
		return -EBUSY;
	}

	if (context->scratch->owner == SQ_DEVICE_PROTOCOL_SCRATCH_FREE) {
		int result = clear_runtime_context(context);
		if (result != 0) {
			return result;
		}
		context->scratch->owner = SQ_DEVICE_PROTOCOL_SCRATCH_STORAGE_FORMAT;
		sq_app_store_format_job_reset(&context->scratch->format_job);
		if (context->mutable_registry != NULL) {
			memset(context->mutable_registry, 0, sizeof(*context->mutable_registry));
		}
	} else if (context->scratch->owner != SQ_DEVICE_PROTOCOL_SCRATCH_STORAGE_FORMAT) {
		return -EBUSY;
	}

	bool done = false;
	int result = sq_app_store_format_job_step(&context->scratch->format_job,
						 context->store_mount_point, &done);
	if (result != 0) {
		sq_app_store_format_job_reset(&context->scratch->format_job);
		context->scratch->owner = SQ_DEVICE_PROTOCOL_SCRATCH_FREE;
		return result;
	}
	if (!done) {
		return pending_line_response(request, "storage-format pending", response, response_cap,
					     response_len);
	}
	context->scratch->owner = SQ_DEVICE_PROTOCOL_SCRATCH_FREE;
	return ok_response(request, response, response_cap, response_len);
}

static int dispatch_event_from_parts(const struct sq_protocol_request *request,
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
		if (context->runtime->current_app[0] != '\0') {
			size_t current_app_len = strlen(context->runtime->current_app);
			if (current_app_len != app_id_len ||
			    memcmp(context->runtime->current_app, app_id, app_id_len) != 0) {
				return -EINVAL;
			}
		} else {
			sq_vm_runtime_reset_vm_context(context->runtime);
		}
		int result = sq_app_store_vm_storage_for_app_bytes(context->store_mount_point,
								   app_id, app_id_len,
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

	struct sq_vm_storage_backend backend =
		sq_app_store_vm_storage_backend(context->launch_storage);
	int result = sq_vm_runtime_start_event(context->runtime, &backend, event, event_len);
	if (result != 0) {
		return result;
	}
	return ok_response(request, response, response_cap, response_len);
}

static int __noinline dispatch_event_request(const struct sq_protocol_request *request,
				  const uint8_t *request_bytes, size_t request_len,
				  const struct sq_device_protocol_context *context,
				  uint8_t *response, size_t response_cap, size_t *response_len)
{
	SqdpEventDispatch event = {0};
	int result;

	if (sqdp_parse_event_dispatch_request(request_bytes, request_len, &event) != SQDP_STATUS_OK) {
		return -EINVAL;
	}
	if (event.event == NULL || event.event_len == 0 ||
	    event.event_len >= SQ_VM_RUNTIME_EVENT_LEN || event.app_id == NULL ||
	    event.app_id_len == 0 || event.app_id_len >= SQ_APP_STORE_APP_ID_MAX ||
	    context->runtime == NULL || context->store_mount_point == NULL ||
	    context->launch_storage == NULL) {
		return -EINVAL;
	}
	if (context->runtime->current_app[0] != '\0') {
		size_t current_app_len = strlen(context->runtime->current_app);
		if (current_app_len != event.app_id_len ||
		    memcmp(context->runtime->current_app, event.app_id, event.app_id_len) != 0) {
			return -EINVAL;
		}
	} else {
		sq_vm_runtime_reset_vm_context(context->runtime);
	}
	result = sq_app_store_vm_storage_for_app_bytes(context->store_mount_point,
						       event.app_id, event.app_id_len,
						       context->launch_storage);
	if (result != 0) {
		return result;
	}
	if (context->launch_storage->fs_storage.sqbc_path == NULL) {
		return -ENODEV;
	}

	struct sq_vm_storage_backend backend =
		sq_app_store_vm_storage_backend(context->launch_storage);
	result = sq_vm_runtime_start_event(context->runtime, &backend, event.event,
					   event.event_len);
	if (result != 0) {
		return result;
	}
	return ok_response(request, response, response_cap, response_len);
}

static int __noinline dispatch_key(const struct sq_protocol_request *request,
			const uint8_t *request_bytes, size_t request_len,
			const struct sq_device_protocol_context *context, uint8_t *response,
			size_t response_cap, size_t *response_len)
{
	size_t event_len = 0;

	if (context == NULL || context->runtime == NULL) {
		return -EINVAL;
	}
	uint8_t *event = (uint8_t *)context->runtime->event;
	if (sqdp_prepare_key_event(request_bytes, request_len, event,
				   sizeof(context->runtime->event), &event_len) !=
	    SQDP_STATUS_OK) {
		return -EINVAL;
	}
	return dispatch_event_from_parts(request, context, NULL, 0, event, event_len, response,
					 response_cap, response_len);
}

static int __noinline wifi_profile_set(const struct sq_protocol_request *request,
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

static int __noinline errors_response(const struct sq_protocol_request *request,
				      const struct sq_vm_runtime *runtime, uint8_t *response,
				      size_t response_cap, size_t *response_len)
{
	const char *lines[1 + SQ_VM_RUNTIME_DEVICE_ERROR_MAX];
	size_t line_count = 0;
	char error_line[48];

	if (runtime != NULL) {
		for (size_t i = 0; i < runtime->device_error_count && line_count < ARRAY_SIZE(lines);
		     i++) {
			lines[line_count++] = runtime->device_errors[i];
		}
	}
	if (runtime != NULL && runtime->status == SQ_VM_RUNTIME_ERROR) {
		const char *status_name = runtime->result.status == SQVM_STATUS_OK ?
						  "host_error" :
						  sq_vm_runtime_status_name(runtime->result.status);
		int written = snprintf(error_line, sizeof(error_line), "runtime=%s code=%d",
				       status_name, runtime->result_code);
		if (written > 0 && (size_t)written < sizeof(error_line)) {
			lines[line_count++] = error_line;
		}
	}
	return repeated_runtime_lines_response(request, runtime, lines, line_count, response,
					      response_cap, response_len);
}

int sq_device_protocol_handle_frame(const uint8_t *request, size_t request_len,
				    const struct sq_device_protocol_context *context, uint8_t *response,
				    size_t response_cap, size_t *response_len)
{
	struct sq_protocol_request frame;
	int result;

	*response_len = 0;
	if (context == NULL || context->identity == NULL) {
		return SQ_PROTOCOL_ERR_BAD_MAGIC;
	}

	result = sq_protocol_decode_request(request, request_len, &frame);
	if (result != SQ_PROTOCOL_OK) {
		return result;
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
	case SQ_OPCODE_ERRORS_GET:
		result = errors_response(&frame, context->runtime, response, response_cap,
					 response_len);
		break;
	case SQ_OPCODE_STATE_GET:
		result = state_get_response(&frame, context, response, response_cap, response_len);
		break;
	case SQ_OPCODE_STATE_IMPORT:
		result = state_import(&frame, request, request_len, context, response, response_cap,
				      response_len);
		break;
	case SQ_OPCODE_RESOURCES_GET:
		result = resources_response(&frame, request, request_len, context, response,
					    response_cap, response_len);
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
