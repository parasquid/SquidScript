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
#include "ble_file_transfer_core.h"
#include "ble_profile_table.h"
#include "http_upload.h"
#include "protocol.h"
#include "sq_errno.h"
#include "squidvm_ffi.h"
#include "vm_runtime_display_backend.h"
#include "xteink_x4_button_probe.h"

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
	SQ_DEVICE_FIELD_TYPE_BYTES = 0,
	SQ_DEVICE_FIELD_TYPE_BOOL = 3,
	SQ_DEVICE_FIELD_TYPE_STRING = 1,
	SQ_DEVICE_FIELD_TYPE_U64 = 5,
	SQ_DEVICE_FIELD_TYPE_U32 = 6,
	SQ_DEVICE_FIELD_TYPE_RECORD = 32,
};

enum sq_resources_request_field {
	SQ_RESOURCES_FIELD_RESET_HEAP_MAX = 1,
};

enum sq_runtime_cap_request_field {
	SQ_RUNTIME_CAP_FIELD_KEY = 1,
	SQ_RUNTIME_CAP_FIELD_VALUE = 2,
};

enum sq_display_window_probe_request_field {
	SQ_DISPLAY_WINDOW_PROBE_FIELD_PATTERN = 1,
};

struct sq_runtime_cap_request {
	char key[40];
	size_t key_len;
	uint32_t value;
	bool has_key;
	bool has_value;
};

struct sq_content_begin_request {
	char name[SQ_DEVICE_CONTENT_NAME_BYTES];
	size_t total_len;
	uint32_t expected_crc;
	bool has_name;
	bool has_total_len;
	bool has_crc;
};

struct sq_content_chunk_request {
	size_t offset;
	const uint8_t *bytes;
	size_t bytes_len;
	bool has_offset;
	bool has_bytes;
};

enum sq_resource_metric_id {
	SQ_RESOURCE_METRIC_RAM_TOTAL_BYTES = 1,
	SQ_RESOURCE_METRIC_RUNTIME_STATIC_BYTES = 2,
	SQ_RESOURCE_METRIC_VM_SQBC_CHUNK_BYTES = 3,
	SQ_RESOURCE_METRIC_HEAP_COUNT = 4,
	SQ_RESOURCE_METRIC_HEAP_FREE_BYTES = 5,
	SQ_RESOURCE_METRIC_HEAP_ALLOC_BYTES = 6,
	SQ_RESOURCE_METRIC_HEAP_MAX_ALLOC_BYTES = 7,
	SQ_RESOURCE_METRIC_HEAP_LARGEST_FREE_SUPPORTED = 8,
	SQ_RESOURCE_METRIC_HEAP_LARGEST_FREE_BYTES = 9,
	SQ_RESOURCE_METRIC_LAST_DISPATCH_US = 10,
	SQ_RESOURCE_METRIC_LAST_DISPATCH_SEQ = 11,
	SQ_RESOURCE_METRIC_LAST_SQBC_READS = 12,
	SQ_RESOURCE_METRIC_LAST_SQBC_BYTES = 13,
	SQ_RESOURCE_METRIC_RUNTIME_STATUS = 14,
	SQ_RESOURCE_METRIC_RUNTIME_DISPATCH_STARTED = 15,
	SQ_RESOURCE_METRIC_RUNTIME_DISPATCH_AGE_US = 16,
	SQ_RESOURCE_METRIC_RUNTIME_WORK_SUBMITTED = 17,
	SQ_RESOURCE_METRIC_RUNTIME_CURRENT_APP_PRESENT = 18,
	SQ_RESOURCE_METRIC_RUNTIME_LIFECYCLE_PHASE = 19,
	SQ_RESOURCE_METRIC_RUNTIME_ARM_PHASE = 20,
	SQ_RESOURCE_METRIC_CAP_STATIC_TIMER = 21,
	SQ_RESOURCE_METRIC_CAP_STATIC_ARMED_TIMER = 22,
	SQ_RESOURCE_METRIC_CAP_STATIC_INPUT_BUTTON = 23,
	SQ_RESOURCE_METRIC_CAP_STATIC_BINDING = 24,
	SQ_RESOURCE_METRIC_CAP_STATIC_OUTPUT = 25,
	SQ_RESOURCE_METRIC_CAP_STATIC_DRAWLOG = 26,
	SQ_RESOURCE_METRIC_CAP_STATIC_DEVICE_ERROR = 27,
	SQ_RESOURCE_METRIC_CAP_ACTIVE_TIMER = 28,
	SQ_RESOURCE_METRIC_CAP_ACTIVE_ARMED_TIMER = 29,
	SQ_RESOURCE_METRIC_CAP_ACTIVE_INPUT_BUTTON = 30,
	SQ_RESOURCE_METRIC_CAP_ACTIVE_BINDING = 31,
	SQ_RESOURCE_METRIC_CAP_ACTIVE_OUTPUT = 32,
	SQ_RESOURCE_METRIC_CAP_ACTIVE_DRAWLOG = 33,
	SQ_RESOURCE_METRIC_PROTO_STACK_SIZE_BYTES = 34,
	SQ_RESOURCE_METRIC_PROTO_STACK_PRE_UNUSED_BYTES = 35,
	SQ_RESOURCE_METRIC_PROTO_STACK_PRE_USED_BYTES = 36,
	SQ_RESOURCE_METRIC_PROTO_STACK_UNUSED_BYTES = 37,
	SQ_RESOURCE_METRIC_PROTO_STACK_USED_BYTES = 38,
	SQ_RESOURCE_METRIC_VM_STACK_SIZE_BYTES = 39,
	SQ_RESOURCE_METRIC_VM_STACK_UNUSED_BYTES = 40,
	SQ_RESOURCE_METRIC_VM_STACK_USED_BYTES = 41,
	SQ_RESOURCE_METRIC_APP_COUNT = 42,
	SQ_RESOURCE_METRIC_INPUT_BUTTON_STATE = 43,
	SQ_RESOURCE_METRIC_X4_INPUT_ADC_GPIO1_RAW = 44,
	SQ_RESOURCE_METRIC_X4_INPUT_ADC_GPIO1_LOGICAL = 45,
	SQ_RESOURCE_METRIC_X4_INPUT_ADC_GPIO1_ERROR = 46,
	SQ_RESOURCE_METRIC_X4_INPUT_ADC_GPIO2_RAW = 47,
	SQ_RESOURCE_METRIC_X4_INPUT_ADC_GPIO2_LOGICAL = 48,
	SQ_RESOURCE_METRIC_X4_INPUT_ADC_GPIO2_ERROR = 49,
	SQ_RESOURCE_METRIC_X4_INPUT_POWER_RAW = 50,
	SQ_RESOURCE_METRIC_X4_INPUT_POWER_PRESSED = 51,
	SQ_RESOURCE_METRIC_X4_INPUT_POWER_ERROR = 52,
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
	if (runtime->current_app_temp) {
		return -ENOTSUP;
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
		strlen(identity->firmware), identity->diagnostic, SQ_SERIAL_MAX_FRAME_LEN,
		response, response_cap, response_len));
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

static uint64_t read_u64_le_device(const uint8_t *bytes)
{
	return (uint64_t)read_u32_le_device(bytes) |
	       ((uint64_t)read_u32_le_device(bytes + 4) << 32);
}

static uint32_t update_crc32(uint32_t crc, const uint8_t *bytes, size_t len)
{
	for (size_t i = 0; i < len; i++) {
		crc ^= bytes[i];
		for (int bit = 0; bit < 8; bit++) {
			uint32_t mask = 0u - (crc & 1u);
			crc = (crc >> 1) ^ (0xedb88320u & mask);
		}
	}
	return crc;
}

static int append_string_field_payload(uint8_t *payload, size_t payload_cap, size_t *payload_len,
				       uint8_t tag, const char *value)
{
	size_t value_len;
	size_t needed;

	if (payload == NULL || payload_len == NULL || value == NULL) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	value_len = strlen(value);
	if (value_len > UINT16_MAX) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	needed = *payload_len + 4u + value_len;
	if (needed > payload_cap) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	payload[*payload_len] = tag;
	payload[*payload_len + 1u] = SQ_DEVICE_FIELD_TYPE_STRING;
	payload[*payload_len + 2u] = value_len & 0xffu;
	payload[*payload_len + 3u] = (value_len >> 8) & 0xffu;
	memcpy(&payload[*payload_len + 4u], value, value_len);
	*payload_len = needed;
	return SQ_PROTOCOL_OK;
}

static int append_u64_field_payload(uint8_t *payload, size_t payload_cap, size_t *payload_len,
				    uint8_t tag, uint64_t value)
{
	size_t needed;

	if (payload == NULL || payload_len == NULL) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	needed = *payload_len + 4u + sizeof(uint64_t);
	if (needed > payload_cap) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	payload[*payload_len] = tag;
	payload[*payload_len + 1u] = SQ_DEVICE_FIELD_TYPE_U64;
	payload[*payload_len + 2u] = sizeof(uint64_t);
	payload[*payload_len + 3u] = 0u;
	for (size_t i = 0; i < sizeof(uint64_t); i++) {
		payload[*payload_len + 4u + i] = (value >> (i * 8u)) & 0xffu;
	}
	*payload_len = needed;
	return SQ_PROTOCOL_OK;
}

static int append_line_payload(uint8_t *payload, size_t payload_cap, size_t *payload_len,
			       const char *line)
{
	return append_string_field_payload(payload, payload_cap, payload_len, 1u, line);
}

static int append_resource_metric(uint8_t *payload, size_t payload_cap, size_t *payload_len,
				  uint32_t id, uint64_t value)
{
	size_t record_len;
	size_t needed;
	uint8_t *record;
	uint8_t *value_field;

	if (payload == NULL || payload_len == NULL || id == 0 || value > UINT32_MAX) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	record_len = 16u;
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
	record[1] = SQ_DEVICE_FIELD_TYPE_U32;
	record[2] = 4u;
	record[3] = 0u;
	for (size_t i = 0; i < 4u; i++) {
		record[4u + i] = (id >> (i * 8u)) & 0xffu;
	}

	value_field = &record[8u];
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

static bool transfer_chunk_ack_requested(const uint8_t *request_bytes, size_t request_len)
{
	const uint8_t *payload;
	uint32_t payload_len;
	size_t offset = 0;

	if (request_bytes == NULL || request_len < SQ_PROTOCOL_HEADER_LEN) {
		return true;
	}
	payload_len = read_u32_le_device(&request_bytes[12]);
	if ((size_t)payload_len > request_len - SQ_PROTOCOL_HEADER_LEN) {
		return true;
	}
	payload = &request_bytes[SQ_PROTOCOL_HEADER_LEN];
	while (offset < payload_len) {
		const uint8_t *field;
		uint8_t tag;
		uint8_t type;
		uint16_t len;
		size_t next_offset;

		if ((size_t)payload_len - offset < 4u) {
			return true;
		}
		field = &payload[offset];
		tag = field[0];
		type = field[1];
		len = (uint16_t)field[2] | ((uint16_t)field[3] << 8);
		next_offset = offset + 4u + len;
		if (next_offset > payload_len) {
			return true;
		}
		if (tag == SQ_DEVICE_CHUNK_FIELD_ACK_REQUESTED) {
			return type != SQ_DEVICE_FIELD_TYPE_BOOL || len != 1u || field[4] != 0u;
		}
		offset = next_offset;
	}
	return true;
}

static int transfer_chunk_response(const struct sq_protocol_request *request,
				   const uint8_t *request_bytes, size_t request_len,
				   uint8_t *response, size_t response_cap, size_t *response_len)
{
	if (!transfer_chunk_ack_requested(request_bytes, request_len)) {
		*response_len = 0;
		return SQ_PROTOCOL_OK;
	}
	return ok_response(request, response, response_cap, response_len);
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
		if (context->runtime != NULL) {
			int result = sq_vm_runtime_wait_idle(context->runtime, 250);

			if (result != 0) {
				return result;
			}
			sq_app_lifecycle_clear_temp_routes(context->runtime);
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
		return transfer_chunk_response(request, request_bytes, request_len, response,
					       response_cap, response_len);
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

	return transfer_chunk_response(request, request_bytes, request_len, response, response_cap,
				       response_len);
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
	return transfer_chunk_response(request, request_bytes, request_len, response, response_cap,
				       response_len);
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

static bool content_install_name_safe(const char *name)
{
	size_t len;

	if (name == NULL || name[0] == '\0' || name[0] == '.') {
		return false;
	}
	len = strlen(name);
	if (len >= SQ_DEVICE_CONTENT_NAME_BYTES) {
		return false;
	}
	for (size_t i = 0; i < len; i++) {
		if (name[i] == '/' || name[i] == '\\') {
			return false;
		}
	}
	return true;
}

static int content_install_paths(const char *name, char *staging, size_t staging_len,
				 char *final, size_t final_len)
{
	int written;

	if (!content_install_name_safe(name) || staging == NULL || final == NULL) {
		return -EINVAL;
	}
	written = snprintf(final, final_len, SQ_VM_RUNTIME_CONTENT_BOOKS_DIR "/%s", name);
	if (written <= 0 || (size_t)written >= final_len) {
		return -ENAMETOOLONG;
	}
	written = snprintf(staging, staging_len, SQ_VM_RUNTIME_CONTENT_BOOKS_DIR "/%s.upload",
			   name);
	return written > 0 && (size_t)written < staging_len ? 0 : -ENAMETOOLONG;
}

static int content_final_path(const char *name, char *final, size_t final_len)
{
	int written;

	if (!content_install_name_safe(name) || final == NULL) {
		return -EINVAL;
	}
	written = snprintf(final, final_len, SQ_VM_RUNTIME_CONTENT_BOOKS_DIR "/%s", name);
	return written > 0 && (size_t)written < final_len ? 0 : -ENAMETOOLONG;
}

static int fs_mkdir_if_missing(const char *path)
{
	struct fs_dirent entry;
	int result;

	if (path == NULL) {
		return -EINVAL;
	}
	result = fs_stat(path, &entry);
	if (result == 0) {
		return entry.type == FS_DIR_ENTRY_DIR ? 0 : -ENOTDIR;
	}
	if (result != -ENOENT) {
		return result;
	}
	result = fs_mkdir(path);
	return result == -EEXIST ? 0 : result;
}

static int fs_unlink_if_exists(const char *path)
{
	struct fs_dirent entry;
	int result;

	if (path == NULL) {
		return -EINVAL;
	}
	result = fs_stat(path, &entry);
	if (result == -ENOENT) {
		return 0;
	}
	if (result != 0) {
		return result;
	}
	return fs_unlink(path);
}

static int parse_content_begin_request(const uint8_t *request_bytes, size_t request_len,
				       struct sq_content_begin_request *out)
{
	const uint8_t *payload;
	uint32_t payload_len;
	size_t offset = 0;

	if (request_bytes == NULL || out == NULL || request_len < SQ_PROTOCOL_HEADER_LEN) {
		return -EINVAL;
	}
	payload_len = read_u32_le_device(&request_bytes[12]);
	if ((size_t)payload_len > request_len - SQ_PROTOCOL_HEADER_LEN) {
		return -EINVAL;
	}
	payload = &request_bytes[SQ_PROTOCOL_HEADER_LEN];
	memset(out, 0, sizeof(*out));

	while (offset < payload_len) {
		const uint8_t *field;
		uint8_t tag;
		uint8_t type;
		uint16_t len;
		size_t next_offset;

		if ((size_t)payload_len - offset < 4U) {
			return -EINVAL;
		}
		field = &payload[offset];
		tag = field[0];
		type = field[1];
		len = (uint16_t)field[2] | ((uint16_t)field[3] << 8);
		next_offset = offset + 4U + len;
		if (next_offset > payload_len) {
			return -EINVAL;
		}
		switch (tag) {
		case 1:
			if (type != SQ_DEVICE_FIELD_TYPE_STRING || len == 0U ||
			    len >= sizeof(out->name)) {
				return -EINVAL;
			}
			memcpy(out->name, &field[4], len);
			out->name[len] = '\0';
			out->has_name = true;
			break;
		case 2: {
			uint64_t total_len;

			if (type != SQ_DEVICE_FIELD_TYPE_U64 || len != sizeof(uint64_t)) {
				return -EINVAL;
			}
			total_len = read_u64_le_device(&field[4]);
			if (total_len == 0U || total_len > SIZE_MAX) {
				return -EINVAL;
			}
			out->total_len = (size_t)total_len;
			out->has_total_len = true;
			break;
		}
		case 3: {
			uint64_t crc;

			if (type != SQ_DEVICE_FIELD_TYPE_U64 || len != sizeof(uint64_t)) {
				return -EINVAL;
			}
			crc = read_u64_le_device(&field[4]);
			if (crc > UINT32_MAX) {
				return -EINVAL;
			}
			out->expected_crc = (uint32_t)crc;
			out->has_crc = true;
			break;
		}
		default:
			return -EINVAL;
		}
		offset = next_offset;
	}
	return out->has_name && out->has_total_len && out->has_crc &&
		       content_install_name_safe(out->name) ?
		       0 :
		       -EINVAL;
}

static int parse_content_chunk_request(const uint8_t *request_bytes, size_t request_len,
				       struct sq_content_chunk_request *out)
{
	const uint8_t *payload;
	uint32_t payload_len;
	size_t offset = 0;

	if (request_bytes == NULL || out == NULL || request_len < SQ_PROTOCOL_HEADER_LEN) {
		return -EINVAL;
	}
	payload_len = read_u32_le_device(&request_bytes[12]);
	if ((size_t)payload_len > request_len - SQ_PROTOCOL_HEADER_LEN) {
		return -EINVAL;
	}
	payload = &request_bytes[SQ_PROTOCOL_HEADER_LEN];
	memset(out, 0, sizeof(*out));

	while (offset < payload_len) {
		const uint8_t *field;
		uint8_t tag;
		uint8_t type;
		uint16_t len;
		size_t next_offset;

		if ((size_t)payload_len - offset < 4U) {
			return -EINVAL;
		}
		field = &payload[offset];
		tag = field[0];
		type = field[1];
		len = (uint16_t)field[2] | ((uint16_t)field[3] << 8);
		next_offset = offset + 4U + len;
		if (next_offset > payload_len) {
			return -EINVAL;
		}
		switch (tag) {
		case SQ_DEVICE_CHUNK_FIELD_OFFSET: {
			uint64_t value;

			if (type != SQ_DEVICE_FIELD_TYPE_U64 || len != sizeof(uint64_t)) {
				return -EINVAL;
			}
			value = read_u64_le_device(&field[4]);
			if (value > SIZE_MAX) {
				return -EINVAL;
			}
			out->offset = (size_t)value;
			out->has_offset = true;
			break;
		}
		case SQ_DEVICE_CHUNK_FIELD_BYTES:
			if (type != SQ_DEVICE_FIELD_TYPE_BYTES || len == 0U) {
				return -EINVAL;
			}
			out->bytes = &field[4];
			out->bytes_len = len;
			out->has_bytes = true;
			break;
		case SQ_DEVICE_CHUNK_FIELD_ACK_REQUESTED:
			if (type != SQ_DEVICE_FIELD_TYPE_BOOL || len != 1U) {
				return -EINVAL;
			}
			break;
		default:
			return -EINVAL;
		}
		offset = next_offset;
	}
	return out->has_offset && out->has_bytes ? 0 : -EINVAL;
}

static void content_session_close_staging(struct sq_device_content_session *session)
{
	if (session != NULL && session->staging_file_open) {
		(void)fs_close(&session->staging_file);
		session->staging_file_open = false;
	}
}

static int content_session_prepare_staging(struct sq_device_content_session *session)
{
	int result;

	if (session == NULL || session->staging_path[0] == '\0') {
		return -EINVAL;
	}
	if (session->staging_file_open) {
		return 0;
	}
	result = fs_mkdir_if_missing(SQ_VM_RUNTIME_CONTENT_BOOKS_DIR);
	if (result != 0) {
		return result;
	}
	result = fs_unlink_if_exists(session->staging_path);
	if (result != 0) {
		return result;
	}
	fs_file_t_init(&session->staging_file);
	result = fs_open(&session->staging_file, session->staging_path,
			 FS_O_CREATE | FS_O_WRITE | FS_O_TRUNC);
	if (result != 0) {
		return result;
	}
	session->staging_file_open = true;
	return 0;
}

static int __noinline begin_content_install(const struct sq_protocol_request *request,
				 const uint8_t *request_bytes, size_t request_len,
				 const struct sq_device_protocol_context *context,
				 uint8_t *response, size_t response_cap, size_t *response_len)
{
	struct sq_device_content_session *session = context->content_session;
	struct sq_content_begin_request begin;
	int result;

	if (session == NULL) {
		return -ENODEV;
	}
	if (session->active) {
		return -EBUSY;
	}
	result = parse_content_begin_request(request_bytes, request_len, &begin);
	if (result != 0) {
		return result;
	}
	memset(session, 0, sizeof(*session));
	strncpy(session->name, begin.name, sizeof(session->name) - 1U);
	session->total_len = begin.total_len;
	session->expected_crc = begin.expected_crc;
	session->running_crc = 0xffffffffU;
	result = content_install_paths(session->name, session->staging_path,
				       sizeof(session->staging_path), session->final_path,
				       sizeof(session->final_path));
	if (result != 0) {
		memset(session, 0, sizeof(*session));
		return result;
	}
	transfer_session_begin_receiving(session);
	return ok_response(request, response, response_cap, response_len);
}

static int __noinline append_content_chunk(const struct sq_protocol_request *request,
				const uint8_t *request_bytes, size_t request_len,
				const struct sq_device_protocol_context *context,
				uint8_t *response, size_t response_cap, size_t *response_len)
{
	struct sq_device_content_session *session = context->content_session;
	struct sq_content_chunk_request chunk;
	ssize_t written;
	int result;

	if (session == NULL || !session->active ||
	    session->phase != SQ_DEVICE_TRANSFER_RECEIVING) {
		return -EINVAL;
	}
	result = parse_content_chunk_request(request_bytes, request_len, &chunk);
	if (result != 0) {
		return result;
	}
	if (chunk.offset != session->received ||
	    chunk.bytes_len > session->total_len - session->received) {
		return -EINVAL;
	}
	if (!session->staging_file_open) {
		result = content_session_prepare_staging(session);
		if (result != 0) {
			return result;
		}
	}
	written = fs_write(&session->staging_file, chunk.bytes, chunk.bytes_len);
	if (written < 0) {
		return (int)written;
	}
	if ((size_t)written != chunk.bytes_len) {
		return -EIO;
	}
	session->running_crc = update_crc32(session->running_crc, chunk.bytes, chunk.bytes_len);
	session->received += chunk.bytes_len;
	return transfer_chunk_response(request, request_bytes, request_len, response, response_cap,
				       response_len);
}

static int __noinline commit_content_install(const struct sq_protocol_request *request,
				  const struct sq_device_protocol_context *context,
				  uint8_t *response, size_t response_cap, size_t *response_len)
{
	struct sq_device_content_session *session = context->content_session;
	int result;

	if (session == NULL || !session->active ||
	    transfer_session_begin_committing(session) != 0) {
		return -EINVAL;
	}
	content_session_close_staging(session);
	if (session->received != session->total_len ||
	    ~session->running_crc != session->expected_crc) {
		(void)fs_unlink(session->staging_path);
		memset(session, 0, sizeof(*session));
		return -EIO;
	}
	result = fs_unlink(session->final_path);
	if (result != 0 && result != -ENOENT) {
		return result;
	}
	result = fs_rename(session->staging_path, session->final_path);
	if (result != 0) {
		return result;
	}
	memset(session, 0, sizeof(*session));
	return ok_response(request, response, response_cap, response_len);
}

static int parse_content_check_request(const uint8_t *request_bytes, size_t request_len,
				       char *name_out, size_t name_cap)
{
	const uint8_t *payload;
	uint32_t payload_len;
	size_t offset = 0;

	if (request_bytes == NULL || name_out == NULL || name_cap == 0 ||
	    request_len < SQ_PROTOCOL_HEADER_LEN) {
		return -EINVAL;
	}
	payload_len = read_u32_le_device(&request_bytes[12]);
	if ((size_t)payload_len > request_len - SQ_PROTOCOL_HEADER_LEN) {
		return -EINVAL;
	}
	payload = &request_bytes[SQ_PROTOCOL_HEADER_LEN];
	name_out[0] = '\0';
	while (offset < payload_len) {
		const uint8_t *field;
		uint8_t tag;
		uint8_t type;
		uint16_t len;
		size_t next_offset;

		if ((size_t)payload_len - offset < 4U) {
			return -EINVAL;
		}
		field = &payload[offset];
		tag = field[0];
		type = field[1];
		len = (uint16_t)field[2] | ((uint16_t)field[3] << 8);
		next_offset = offset + 4U + len;
		if (next_offset > payload_len) {
			return -EINVAL;
		}
		if (tag == 1) {
			if (type != SQ_DEVICE_FIELD_TYPE_STRING || len == 0U || len >= name_cap) {
				return -EINVAL;
			}
			memcpy(name_out, &field[4], len);
			name_out[len] = '\0';
		} else {
			return -EINVAL;
		}
		offset = next_offset;
	}
	return content_install_name_safe(name_out) ? 0 : -EINVAL;
}

static int content_check_response(const struct sq_protocol_request *request,
				  const uint8_t *request_bytes, size_t request_len,
				  uint8_t *response, size_t response_cap, size_t *response_len)
{
	char name[SQ_DEVICE_CONTENT_NAME_BYTES];
	char path[SQ_DEVICE_CONTENT_PATH_BYTES];
	struct fs_dirent entry;
	struct fs_file_t file;
	uint8_t buf[256];
	uint32_t running_crc = 0xffffffffU;
	size_t payload_len = 0;
	ssize_t read;
	int result;

	result = parse_content_check_request(request_bytes, request_len, name, sizeof(name));
	if (result != 0) {
		return result;
	}
	result = content_final_path(name, path, sizeof(path));
	if (result != 0) {
		return result;
	}
	result = fs_stat(path, &entry);
	if (result != 0) {
		return result;
	}
	fs_file_t_init(&file);
	result = fs_open(&file, path, FS_O_READ);
	if (result != 0) {
		return result;
	}
	while ((read = fs_read(&file, buf, sizeof(buf))) > 0) {
		running_crc = update_crc32(running_crc, buf, (size_t)read);
	}
	(void)fs_close(&file);
	if (read < 0) {
		return (int)read;
	}
	uint8_t *payload = &response[SQ_PROTOCOL_HEADER_LEN];
	result = append_string_field_payload(payload, response_cap - SQ_PROTOCOL_HEADER_LEN,
					     &payload_len, 1u, name);
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	result = append_u64_field_payload(payload, response_cap - SQ_PROTOCOL_HEADER_LEN,
					  &payload_len, 2u, (uint64_t)entry.size);
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	result = append_u64_field_payload(payload, response_cap - SQ_PROTOCOL_HEADER_LEN,
					  &payload_len, 3u, (uint64_t)(~running_crc));
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	memcpy(response, "SQDP", 4);
	response[4] = SQ_FRAME_RESPONSE;
	response[5] = SQ_OPCODE_CONTENT_CHECK;
	response[6] = SQ_STATUS_OK;
	response[7] = 0;
	write_u32_le_device(&response[8], request->sequence);
	write_u32_le_device(&response[12], (uint32_t)payload_len);
	write_u32_le_device(&response[16], sq_protocol_crc32(payload, payload_len));
	*response_len = SQ_PROTOCOL_HEADER_LEN + payload_len;
	return SQ_PROTOCOL_OK;
}

struct temp_storage_backend {
	struct sq_vm_fs_storage fs_storage;
	char state_path[SQ_DEVICE_STAGING_PATH_BYTES];
};

static struct temp_storage_backend temp_foreground_storage;

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
	int result;

	if (session == NULL || context->runtime == NULL || context->store_mount_point == NULL ||
	    sqdp_prepare_transfer_commit(request_bytes, request_len, session, NULL) !=
		    SQDP_STATUS_OK) {
		return -EINVAL;
	}
	if (transfer_session_begin_committing(session) != 0) {
		return -EINVAL;
	}

	memset(&temp_foreground_storage, 0, sizeof(temp_foreground_storage));
	result = temp_state_path_for_mount(context->store_mount_point,
					   temp_foreground_storage.state_path,
					   sizeof(temp_foreground_storage.state_path));
	if (result != 0) {
		return result;
	}
	temp_foreground_storage.fs_storage.sqbc_path = session->staging_path;
	temp_foreground_storage.fs_storage.state_path = temp_foreground_storage.state_path;
	context->runtime->job_backend =
		sq_vm_fs_storage_backend(&temp_foreground_storage.fs_storage);
	result = context->runtime->job_backend.reset_state(context->runtime->job_backend.user_data);
	if (result != 0) {
		return result;
	}
	sq_app_lifecycle_clear_temp_routes(context->runtime);
	result = sq_app_lifecycle_request_temp_launch(context->runtime, (const uint8_t *)session->app_id,
						      strlen(session->app_id));
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
static int start_temp_app_bytes(const struct sq_device_protocol_context *context,
				const uint8_t *app_id, size_t app_id_len,
				const uint8_t *event, size_t event_len, bool set_current);
static int start_resolved_app(const struct sq_device_protocol_context *context,
			      const char *app_id, const uint8_t *event, size_t event_len,
			      bool set_current, bool temp_app);
static int start_resolved_app_bytes(const struct sq_device_protocol_context *context,
				    const uint8_t *app_id, size_t app_id_len,
				    const uint8_t *event, size_t event_len, bool set_current,
				    bool temp_app);
static bool is_main_app_id(const uint8_t *app_id, size_t app_id_len);
static void clear_foreground_timers(struct sq_vm_runtime *runtime);
static void clear_foreground_ble_profile(const struct sq_device_protocol_context *context);

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
			      (context->runtime->current_app_temp ||
			       strlen(context->runtime->current_app) != app_id_len ||
			       memcmp(context->runtime->current_app, app_id, app_id_len) != 0);
	if (current_app_changed) {
		clear_foreground_timers(context->runtime);
		clear_foreground_ble_profile(context);
	}
	if (set_current || context->runtime->current_app_temp ||
	    strlen(context->runtime->current_app) != app_id_len ||
	    memcmp(context->runtime->current_app, app_id, app_id_len) != 0) {
		result = sq_vm_runtime_wait_idle(context->runtime, 250);
		if (result != 0) {
			char l[SQ_VM_RUNTIME_DEVICE_ERROR_LEN];
			(void)snprintf(l, sizeof(l), "launch waitidle %d (%s)", result,
				       sq_errno_name(result));
			(void)sq_vm_runtime_record_device_error(context->runtime, l);
			return result;
		}
		sq_vm_runtime_reset_vm_context(context->runtime);
	}

	result = sq_app_store_vm_storage_for_app_bytes(context->store_mount_point, app_id,
						       app_id_len, context->launch_storage);
	if (result != 0) {
		char l[SQ_VM_RUNTIME_DEVICE_ERROR_LEN];
		(void)snprintf(l, sizeof(l), "launch storage %d (%s)", result,
			       sq_errno_name(result));
		(void)sq_vm_runtime_record_device_error(context->runtime, l);
		return result;
	}
	context->runtime->job_backend = sq_app_store_vm_storage_backend(context->launch_storage);
	if (set_current) {
		strncpy(context->runtime->lifecycle_previous_app, context->runtime->current_app,
			sizeof(context->runtime->lifecycle_previous_app) - 1);
		context->runtime
			->lifecycle_previous_app[sizeof(context->runtime->lifecycle_previous_app) -
						 1] = '\0';
		context->runtime->lifecycle_previous_app_temp = context->runtime->current_app_temp;
		memcpy(context->runtime->current_app, app_id, app_id_len);
		context->runtime->current_app[app_id_len] = '\0';
		context->runtime->current_app_temp = false;
	}
	result = sq_vm_runtime_start_event(context->runtime, &context->runtime->job_backend, event,
					   event_len);
	if (result != 0) {
		char l[SQ_VM_RUNTIME_DEVICE_ERROR_LEN];
		(void)snprintf(l, sizeof(l), "launch start_event %d (%s)", result,
			       sq_errno_name(result));
		(void)sq_vm_runtime_record_device_error(context->runtime, l);
		if (set_current) {
			strncpy(context->runtime->current_app,
				context->runtime->lifecycle_previous_app,
				sizeof(context->runtime->current_app) - 1);
			context->runtime->current_app[sizeof(context->runtime->current_app) - 1] =
				'\0';
			context->runtime->current_app_temp =
				context->runtime->lifecycle_previous_app_temp;
			memset(context->runtime->lifecycle_previous_app, 0,
			       sizeof(context->runtime->lifecycle_previous_app));
			context->runtime->lifecycle_previous_app_temp = false;
		}
		return result;
	}
	if (set_current) {
		memset(context->runtime->lifecycle_previous_app, 0,
		       sizeof(context->runtime->lifecycle_previous_app));
		context->runtime->lifecycle_previous_app_temp = false;
	}
	return 0;
}

static int start_temp_app_bytes(const struct sq_device_protocol_context *context,
				const uint8_t *app_id, size_t app_id_len,
				const uint8_t *event, size_t event_len, bool set_current)
{
	struct sq_device_temp_session *session;
	bool current_app_changed;
	int result;

	if (context == NULL || context->runtime == NULL || context->temp_session == NULL ||
	    context->store_mount_point == NULL || app_id == NULL || event == NULL ||
	    app_id_len == 0 || app_id_len >= SQ_APP_STORE_APP_ID_MAX) {
		return -EINVAL;
	}
	session = context->temp_session;
	if (session->app_id[0] == '\0' || session->staging_path[0] == '\0' ||
	    strlen(session->app_id) != app_id_len ||
	    memcmp(session->app_id, app_id, app_id_len) != 0) {
		return -EINVAL;
	}
	if (!set_current && (!context->runtime->current_app_temp ||
			     strlen(context->runtime->current_app) != app_id_len ||
			     memcmp(context->runtime->current_app, app_id, app_id_len) != 0)) {
		return -EINVAL;
	}
	current_app_changed = set_current &&
			      (!context->runtime->current_app_temp ||
			       strlen(context->runtime->current_app) != app_id_len ||
			       memcmp(context->runtime->current_app, app_id, app_id_len) != 0);
	if (current_app_changed) {
		clear_foreground_timers(context->runtime);
		clear_foreground_ble_profile(context);
	}
	if (set_current || !context->runtime->current_app_temp ||
	    strlen(context->runtime->current_app) != app_id_len ||
	    memcmp(context->runtime->current_app, app_id, app_id_len) != 0) {
		result = sq_vm_runtime_wait_idle(context->runtime, 250);
		if (result != 0) {
			return result;
		}
		sq_vm_runtime_reset_vm_context(context->runtime);
	}

	memset(&temp_foreground_storage, 0, sizeof(temp_foreground_storage));
	result = temp_state_path_for_mount(context->store_mount_point,
					   temp_foreground_storage.state_path,
					   sizeof(temp_foreground_storage.state_path));
	if (result != 0) {
		return result;
	}
	temp_foreground_storage.fs_storage.sqbc_path = session->staging_path;
	temp_foreground_storage.fs_storage.state_path = temp_foreground_storage.state_path;
	context->runtime->job_backend =
		sq_vm_fs_storage_backend(&temp_foreground_storage.fs_storage);
	if (set_current) {
		strncpy(context->runtime->lifecycle_previous_app, context->runtime->current_app,
			sizeof(context->runtime->lifecycle_previous_app) - 1);
		context->runtime
			->lifecycle_previous_app[sizeof(context->runtime->lifecycle_previous_app) -
						 1] = '\0';
		context->runtime->lifecycle_previous_app_temp = context->runtime->current_app_temp;
		memcpy(context->runtime->current_app, app_id, app_id_len);
		context->runtime->current_app[app_id_len] = '\0';
		context->runtime->current_app_temp = true;
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
			context->runtime->current_app_temp =
				context->runtime->lifecycle_previous_app_temp;
			memset(context->runtime->lifecycle_previous_app, 0,
			       sizeof(context->runtime->lifecycle_previous_app));
			context->runtime->lifecycle_previous_app_temp = false;
		}
		return result;
	}
	if (set_current) {
		memset(context->runtime->lifecycle_previous_app, 0,
		       sizeof(context->runtime->lifecycle_previous_app));
		context->runtime->lifecycle_previous_app_temp = false;
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
	current_app_changed = set_current &&
			      (context->runtime->current_app_temp ||
			       strcmp(context->runtime->current_app, "main") != 0);
	if (current_app_changed) {
		clear_foreground_timers(context->runtime);
		clear_foreground_ble_profile(context);
	}
	if (set_current || context->runtime->current_app_temp ||
	    strcmp(context->runtime->current_app, "main") != 0) {
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
		context->runtime->lifecycle_previous_app_temp = context->runtime->current_app_temp;
		strncpy(context->runtime->current_app, "main",
			sizeof(context->runtime->current_app) - 1);
		context->runtime->current_app[sizeof(context->runtime->current_app) - 1] = '\0';
		context->runtime->current_app_temp = false;
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
			context->runtime->current_app_temp =
				context->runtime->lifecycle_previous_app_temp;
			memset(context->runtime->lifecycle_previous_app, 0,
			       sizeof(context->runtime->lifecycle_previous_app));
			context->runtime->lifecycle_previous_app_temp = false;
		}
		return result;
	}
	if (set_current) {
		memset(context->runtime->lifecycle_previous_app, 0,
		       sizeof(context->runtime->lifecycle_previous_app));
		context->runtime->lifecycle_previous_app_temp = false;
	}
	return 0;
}

static int start_resolved_app(const struct sq_device_protocol_context *context,
			      const char *app_id, const uint8_t *event, size_t event_len,
			      bool set_current, bool temp_app)
{
	if (app_id == NULL) {
		return -EINVAL;
	}
	return start_resolved_app_bytes(context, (const uint8_t *)app_id, strlen(app_id), event,
					event_len, set_current, temp_app);
}

static int start_resolved_app_bytes(const struct sq_device_protocol_context *context,
				    const uint8_t *app_id, size_t app_id_len,
				    const uint8_t *event, size_t event_len, bool set_current,
				    bool temp_app)
{
	if (temp_app) {
		return start_temp_app_bytes(context, app_id, app_id_len, event, event_len,
					    set_current);
	}
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

static void clear_foreground_ble_profile(const struct sq_device_protocol_context *context)
{
	uint8_t app_slot = SQ_APP_REGISTRY_SLOT_INVALID;

	if (context == NULL || context->runtime == NULL || context->runtime->current_app_temp ||
	    context->runtime->current_app[0] == '\0') {
		return;
	}
	if (sq_app_registry_slot_for_app(context->registry, context->runtime->current_app,
					 &app_slot) == 0) {
		sq_ble_profile_table_remove_app_slot(app_slot);
	} else if (strcmp(context->runtime->current_app, "main") == 0) {
		sq_ble_profile_table_remove_app_slot(SQ_APP_REGISTRY_SLOT_FALLBACK);
	}
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
				  sizeof("app.start") - 1, true, false);
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
				    (const uint8_t *)"app.start", sizeof("app.start") - 1, true,
				    false);
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

/* Backing storage for the BLE file-transfer completion event payload. Lives
 * at file scope (not on the poll stack) because the dispatch that reads it runs
 * on the VM worker thread after sq_device_protocol_poll returns. Single in-flight
 * transfer + main-thread-only writes make file-static storage safe.
 */
static SqvmEventPayloadField ble_payload_fields[5];
static char ble_payload_bytes_buf[12];
static char ble_payload_total_buf[12];
static SqvmEventPayloadField http_payload_fields[5];
static char http_payload_bytes_buf[12];
static char http_payload_total_buf[12];

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

	if (runtime->pending_install.active) {
		/* A handler queued app.install. Perform it here, between dispatches
		 * (VM idle), and crucially by RENAMING the already-received staging
		 * file into place rather than copying it. The staging bytes were
		 * written earlier (during the BLE/serial transfer), so a rename only
		 * touches directory metadata and leaves the app's data blocks
		 * untouched; a subsequent launch reads them coherently. A fresh copy
		 * here would rewrite the exact blocks the launch reads moments later,
		 * which read back stale from the ESP32 flash read cache and fault the
		 * VM (-EIO). This runs before any pending launch, so app.install +
		 * app.launch in one handler installs first, then launches cleanly. */
		runtime->pending_install.active = false;
		result = sq_app_store_commit_external_file(context->store_mount_point,
							   runtime->pending_install.app_id,
							   runtime->pending_install.file_ref);
		if (result == 0 && context->mutable_registry != NULL) {
			char path[SQ_APP_STORE_PATH_MAX];
			(void)sq_app_store_update_registry_entry_with_path(
				context->store_mount_point, context->mutable_registry,
				runtime->pending_install.app_id, path, sizeof(path));
		}
		if (result != 0) {
			char line[SQ_VM_RUNTIME_DEVICE_ERROR_LEN];
			(void)snprintf(line, sizeof(line), "app.install code=%d (%s)", result,
				       sq_errno_name(result));
			(void)sq_vm_runtime_record_device_error(runtime, line);
		}
		return result;
	}

	if (runtime->lifecycle_phase == SQ_VM_RUNTIME_LIFECYCLE_IDLE &&
	    runtime->arm_phase == SQ_VM_RUNTIME_ARM_IDLE) {
		if (sq_vm_runtime_next_due_armed_timer(runtime, due_app, sizeof(due_app), due_event,
						       sizeof(due_event)) == 0) {
			due_app_ptr = due_app;
			due_event_ptr = due_event;
		} else if (!runtime->dispatch_exited && sq_ble_file_transfer_pending_is_complete() &&
			   sq_ble_file_transfer_drain_pending_event(due_app, sizeof(due_app), due_event,
							  sizeof(due_event)) == 0) {
			/* A completed BLE file transfer dispatches its event to
			 * the target app exactly like an armed timer. drain is
			 * consume-once so this fires a single time. The
			 * !dispatch_exited guard ensures next_step will start this
			 * event (not divert to a pending return) so the consumed
			 * event is not lost.
			 *
			 * Attach the event payload (the staged file path + sizes).
			 * The dispatch runs on the VM worker thread, so the field
			 * storage must outlive this poll: the values live in the BLE
			 * module's static pending state and the file-static buffers
			 * below; both persist until the next transfer / disconnect,
			 * well after the worker consumes them at dispatch start.
			 */
			const char *upload_path = sq_ble_file_transfer_pending_staging_path();
			const char *profile_id = sq_ble_file_transfer_pending_profile_id();
			const char *name = sq_ble_file_transfer_pending_file_name();

			(void)snprintf(ble_payload_bytes_buf, sizeof(ble_payload_bytes_buf), "%zu",
				       sq_ble_file_transfer_pending_bytes_received());
			(void)snprintf(ble_payload_total_buf, sizeof(ble_payload_total_buf), "%zu",
				       sq_ble_file_transfer_pending_total_bytes());
			ble_payload_fields[0] = (SqvmEventPayloadField){
				(const uint8_t *)"upload", 6, (const uint8_t *)upload_path,
				strlen(upload_path)};
			ble_payload_fields[1] = (SqvmEventPayloadField){
				(const uint8_t *)"name", 4, (const uint8_t *)name, strlen(name)};
			ble_payload_fields[2] = (SqvmEventPayloadField){
				(const uint8_t *)"bytesReceived", 13,
				(const uint8_t *)ble_payload_bytes_buf,
				strlen(ble_payload_bytes_buf)};
			ble_payload_fields[3] = (SqvmEventPayloadField){
				(const uint8_t *)"totalBytes", 10,
				(const uint8_t *)ble_payload_total_buf,
				strlen(ble_payload_total_buf)};
			ble_payload_fields[4] = (SqvmEventPayloadField){
				(const uint8_t *)"id", 2, (const uint8_t *)profile_id,
				strlen(profile_id)};
			sq_vm_runtime_set_pending_event_payload(runtime, ble_payload_fields, 5);
			due_app_ptr = due_app;
			due_event_ptr = due_event;
		} else if (!runtime->dispatch_exited && sq_http_upload_pending_is_complete() &&
			   sq_http_upload_drain_pending_event(due_app, sizeof(due_app), due_event,
							      sizeof(due_event)) == 0) {
			const char *upload_path = sq_http_upload_pending_staging_path();
			const char *profile_id = sq_http_upload_pending_profile_id();
			const char *name = sq_http_upload_pending_name();

			(void)snprintf(http_payload_bytes_buf, sizeof(http_payload_bytes_buf), "%zu",
				       sq_http_upload_pending_bytes_received());
			(void)snprintf(http_payload_total_buf, sizeof(http_payload_total_buf), "%zu",
				       sq_http_upload_pending_total_bytes());
			http_payload_fields[0] = (SqvmEventPayloadField){
				(const uint8_t *)"upload", 6, (const uint8_t *)upload_path,
				strlen(upload_path)};
			http_payload_fields[1] = (SqvmEventPayloadField){
				(const uint8_t *)"name", 4, (const uint8_t *)name, strlen(name)};
			http_payload_fields[2] = (SqvmEventPayloadField){
				(const uint8_t *)"bytesReceived", 13,
				(const uint8_t *)http_payload_bytes_buf,
				strlen(http_payload_bytes_buf)};
			http_payload_fields[3] = (SqvmEventPayloadField){
				(const uint8_t *)"totalBytes", 10,
				(const uint8_t *)http_payload_total_buf,
				strlen(http_payload_total_buf)};
			http_payload_fields[4] = (SqvmEventPayloadField){
				(const uint8_t *)"id", 2, (const uint8_t *)profile_id,
				strlen(profile_id)};
			sq_vm_runtime_set_pending_event_payload(runtime, http_payload_fields, 5);
			due_app_ptr = due_app;
			due_event_ptr = due_event;
		}
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
					    strlen(step.event), step.set_current, step.temp_app);
		if (result != 0) {
			sq_app_lifecycle_cancel_pending_after_start_failure(runtime, result);
		}
		return result;

	case SQ_APP_LIFECYCLE_STEP_REGISTER_ARMED_APP:
		/* BLE file-transfer profiles are no longer registered at arm time;
		 * an app registers its profile imperatively via service.ble.start
		 * (runtime_ble_start) while running foreground.
		 */
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
	SqdpLineSlice extra_slices[2 + SQ_VM_RUNTIME_DEVICE_ERROR_MAX];
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
	struct sq_x4_button_probe x4_probe;
	bool x4_probe_present = false;
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
	if (sq_x4_button_probe_read(&x4_probe) == 0) {
		x4_probe_present = true;
	}

#define SQ_RESOURCE_METRIC(metric_id, metric_value) \
	do { \
		int metric_result = append_resource_metric(payload, payload_cap, &payload_len, \
							   (metric_id), (metric_value)); \
		if (metric_result != SQ_PROTOCOL_OK) { \
			return metric_result; \
		} \
	} while (false)
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_RAM_TOTAL_BYTES, CONFIG_SRAM_SIZE * 1024u);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_RUNTIME_STATIC_BYTES,
			   context->runtime == NULL ? 0 : sizeof(*context->runtime));
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_VM_SQBC_CHUNK_BYTES, SQVM_STORAGE_TRANSFER_CAPACITY);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_HEAP_COUNT, heap_count);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_HEAP_FREE_BYTES, heap_free_bytes);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_HEAP_ALLOC_BYTES, heap_allocated_bytes);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_HEAP_MAX_ALLOC_BYTES, heap_max_allocated_bytes);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_HEAP_LARGEST_FREE_SUPPORTED,
			   heap_largest_free_supported);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_HEAP_LARGEST_FREE_BYTES, heap_largest_free_bytes);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_LAST_DISPATCH_US,
			   context->runtime == NULL ? 0 : context->runtime->last_dispatch_elapsed_us);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_LAST_DISPATCH_SEQ,
			   context->runtime == NULL ? 0 : context->runtime->last_dispatch_sequence);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_LAST_SQBC_READS,
			   context->runtime == NULL ? 0 :
						      context->runtime->last_dispatch_sqbc_read_count);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_LAST_SQBC_BYTES,
			   context->runtime == NULL ? 0 :
						      context->runtime->last_dispatch_sqbc_read_bytes);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_RUNTIME_STATUS, runtime_status);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_RUNTIME_DISPATCH_STARTED, runtime_dispatch_started);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_RUNTIME_DISPATCH_AGE_US, runtime_dispatch_age_us);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_RUNTIME_WORK_SUBMITTED, runtime_work_submitted);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_RUNTIME_CURRENT_APP_PRESENT,
			   runtime_current_app_present);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_RUNTIME_LIFECYCLE_PHASE, runtime_lifecycle_phase);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_RUNTIME_ARM_PHASE, runtime_arm_phase);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_CAP_STATIC_TIMER, SQ_VM_RUNTIME_TIMER_MAX);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_CAP_STATIC_ARMED_TIMER,
			   SQ_VM_RUNTIME_ARMED_TIMER_MAX);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_CAP_STATIC_INPUT_BUTTON,
			   SQ_VM_RUNTIME_INPUT_BUTTON_MAX);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_CAP_STATIC_BINDING,
			   SQ_VM_RUNTIME_ACTIVE_BINDING_MAX);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_CAP_STATIC_OUTPUT, SQ_VM_RUNTIME_OUTPUT_MAX);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_CAP_STATIC_DRAWLOG, SQ_VM_RUNTIME_DRAWLOG_MAX);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_CAP_STATIC_DEVICE_ERROR,
			   SQ_VM_RUNTIME_DEVICE_ERROR_MAX);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_CAP_ACTIVE_TIMER,
			   context->runtime == NULL ? SQ_VM_RUNTIME_TIMER_MAX :
						      context->runtime->active_timer_max);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_CAP_ACTIVE_ARMED_TIMER,
			   context->runtime == NULL ? SQ_VM_RUNTIME_ARMED_TIMER_MAX :
						      context->runtime->active_armed_timer_max);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_CAP_ACTIVE_INPUT_BUTTON,
			   context->runtime == NULL ? SQ_VM_RUNTIME_INPUT_BUTTON_MAX :
						      context->runtime->active_input_button_max);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_CAP_ACTIVE_BINDING,
			   context->runtime == NULL ? SQ_VM_RUNTIME_ACTIVE_BINDING_MAX :
						      context->runtime->active_binding_max);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_CAP_ACTIVE_OUTPUT,
			   context->runtime == NULL ? SQ_VM_RUNTIME_OUTPUT_MAX :
						      context->runtime->active_output_max);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_CAP_ACTIVE_DRAWLOG,
			   context->runtime == NULL ? SQ_VM_RUNTIME_DRAWLOG_MAX :
						      context->runtime->active_drawlog_max);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_PROTO_STACK_SIZE_BYTES, protocol_stack_size);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_PROTO_STACK_PRE_UNUSED_BYTES,
			   protocol_stack_pre_resources_unused);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_PROTO_STACK_PRE_USED_BYTES,
			   protocol_stack_pre_resources_used);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_PROTO_STACK_UNUSED_BYTES, protocol_stack_unused);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_PROTO_STACK_USED_BYTES, protocol_stack_used);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_VM_STACK_SIZE_BYTES, vm_worker_stack_size);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_VM_STACK_UNUSED_BYTES, vm_worker_stack_unused);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_VM_STACK_USED_BYTES, vm_worker_stack_used);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_APP_COUNT,
			   context->registry == NULL ? 0 : context->registry->count);
	SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_INPUT_BUTTON_STATE, input_button_state);
	if (x4_probe_present) {
		SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_X4_INPUT_ADC_GPIO1_RAW,
				   x4_probe.adc_gpio1_raw);
		SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_X4_INPUT_ADC_GPIO1_LOGICAL,
				   x4_probe.adc_gpio1_logical);
		SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_X4_INPUT_ADC_GPIO1_ERROR,
				   x4_probe.adc_gpio1_error);
		SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_X4_INPUT_ADC_GPIO2_RAW,
				   x4_probe.adc_gpio2_raw);
		SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_X4_INPUT_ADC_GPIO2_LOGICAL,
				   x4_probe.adc_gpio2_logical);
		SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_X4_INPUT_ADC_GPIO2_ERROR,
				   x4_probe.adc_gpio2_error);
		SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_X4_INPUT_POWER_RAW, x4_probe.power_raw);
		SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_X4_INPUT_POWER_PRESSED,
				   x4_probe.power_pressed);
		SQ_RESOURCE_METRIC(SQ_RESOURCE_METRIC_X4_INPUT_POWER_ERROR,
				   x4_probe.power_error);
	}
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
	if (context->content_session != NULL) {
		content_session_close_staging(context->content_session);
		if (context->content_session->active) {
			(void)fs_unlink(context->content_session->staging_path);
		}
		memset(context->content_session, 0, sizeof(*context->content_session));
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
	bool use_temp_backend = false;

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
			use_temp_backend = context->runtime->current_app_temp;
		} else {
			sq_vm_runtime_reset_vm_context(context->runtime);
		}
		if (!use_temp_backend) {
			int result = sq_app_store_vm_storage_for_app_bytes(
				context->store_mount_point, app_id, app_id_len,
				context->launch_storage);
			if (result != 0) {
				return result;
			}
		}
	} else if (context->runtime->current_app[0] != '\0') {
		use_temp_backend = context->runtime->current_app_temp;
		if (!use_temp_backend) {
			int result = sq_app_store_vm_storage_for_app(
				context->store_mount_point, context->runtime->current_app,
				context->launch_storage);
			if (result != 0) {
				return result;
			}
		}
	}
	if (use_temp_backend) {
		if (context->runtime->job_backend.read_sqbc == NULL) {
			return -ENODEV;
		}
		int result = sq_vm_runtime_start_event(context->runtime,
						       &context->runtime->job_backend, event,
						       event_len);
		if (result != 0) {
			return result;
		}
		return ok_response(request, response, response_cap, response_len);
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
	bool use_temp_backend = false;
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
		use_temp_backend = context->runtime->current_app_temp;
	} else {
		sq_vm_runtime_reset_vm_context(context->runtime);
	}
	if (!use_temp_backend) {
		result = sq_app_store_vm_storage_for_app_bytes(context->store_mount_point,
							       event.app_id, event.app_id_len,
							       context->launch_storage);
		if (result != 0) {
			return result;
		}
	}
	if (use_temp_backend) {
		if (context->runtime->job_backend.read_sqbc == NULL) {
			return -ENODEV;
		}
		result = sq_vm_runtime_start_event(context->runtime, &context->runtime->job_backend,
						   event.event, event.event_len);
		if (result != 0) {
			return result;
		}
		return ok_response(request, response, response_cap, response_len);
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

static int parse_runtime_cap_request(const uint8_t *request_bytes, size_t request_len,
				     struct sq_runtime_cap_request *out)
{
	const uint8_t *payload;
	uint32_t payload_len;
	size_t offset = 0;

	if (request_bytes == NULL || out == NULL || request_len < SQ_PROTOCOL_HEADER_LEN) {
		return -EINVAL;
	}
	payload_len = read_u32_le_device(&request_bytes[12]);
	if ((size_t)payload_len > request_len - SQ_PROTOCOL_HEADER_LEN) {
		return -EINVAL;
	}
	payload = &request_bytes[SQ_PROTOCOL_HEADER_LEN];
	memset(out, 0, sizeof(*out));

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
		switch (tag) {
		case SQ_RUNTIME_CAP_FIELD_KEY:
			if (type != SQ_DEVICE_FIELD_TYPE_STRING || len == 0u ||
			    len >= sizeof(out->key)) {
				return -EINVAL;
			}
			memcpy(out->key, &field[4], len);
			out->key[len] = '\0';
			out->key_len = len;
			out->has_key = true;
			break;
		case SQ_RUNTIME_CAP_FIELD_VALUE:
			if (type != SQ_DEVICE_FIELD_TYPE_U32 || len != sizeof(uint32_t)) {
				return -EINVAL;
			}
			out->value = read_u32_le_device(&field[4]);
			out->has_value = true;
			break;
		default:
			return -EINVAL;
		}
		offset = next_offset;
	}
	return 0;
}

static int __noinline runtime_cap_set(const struct sq_protocol_request *request,
				      const uint8_t *request_bytes, size_t request_len,
				      const struct sq_device_protocol_context *context,
				      uint8_t *response, size_t response_cap,
				      size_t *response_len)
{
	struct sq_runtime_cap_request cap_request;

	if (context == NULL || context->runtime == NULL) {
		return -ENODEV;
	}
	if (parse_runtime_cap_request(request_bytes, request_len, &cap_request) != 0 ||
	    !cap_request.has_key || !cap_request.has_value || cap_request.value > UINT16_MAX) {
		return -EINVAL;
	}
	int result = sq_vm_runtime_cap_set(context->runtime, cap_request.key,
					   (uint16_t)cap_request.value);
	if (result != 0) {
		return result;
	}
	if (context->runtime->store_mount_point != NULL) {
		result = sq_vm_runtime_cap_save(context->runtime);
		if (result != 0) {
			return result;
		}
	}
	return ok_response(request, response, response_cap, response_len);
}

static int __noinline runtime_cap_clear(const struct sq_protocol_request *request,
					const uint8_t *request_bytes, size_t request_len,
					const struct sq_device_protocol_context *context,
					uint8_t *response, size_t response_cap,
					size_t *response_len)
{
	struct sq_runtime_cap_request cap_request;

	if (context == NULL || context->runtime == NULL) {
		return -ENODEV;
	}
	if (parse_runtime_cap_request(request_bytes, request_len, &cap_request) != 0 ||
	    cap_request.has_value) {
		return -EINVAL;
	}
	int result = sq_vm_runtime_cap_clear(context->runtime,
					     cap_request.has_key ? cap_request.key : NULL);
	if (result != 0) {
		return result;
	}
	if (context->runtime->store_mount_point != NULL) {
		result = sq_vm_runtime_cap_save(context->runtime);
		if (result != 0) {
			return result;
		}
	}
	return ok_response(request, response, response_cap, response_len);
}

static int runtime_cap_format_line(const struct sq_vm_runtime *runtime, const char *key,
				   char *out, size_t out_len)
{
	uint16_t value;
	int result = sq_vm_runtime_cap_get(runtime, key, &value);

	if (result != 0) {
		return result;
	}
	int written = snprintf(out, out_len, "%s=%u", key, value);
	if (written < 0 || (size_t)written >= out_len) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	return 0;
}

static int __noinline runtime_cap_get(const struct sq_protocol_request *request,
				      const uint8_t *request_bytes, size_t request_len,
				      const struct sq_device_protocol_context *context,
				      uint8_t *response, size_t response_cap,
				      size_t *response_len)
{
	static const char *const runtime_cap_keys[] = {
		"vm_runtime.timer_max",
		"vm_runtime.armed_timer_max",
		"vm_runtime.input_button_max",
		"vm_runtime.active_binding_max",
		"vm_runtime.output_max",
		"vm_runtime.drawlog_max",
	};
	struct sq_runtime_cap_request cap_request;
	const char *lines[ARRAY_SIZE(runtime_cap_keys)];
	char line_storage[ARRAY_SIZE(runtime_cap_keys)][48];
	size_t line_count = 0;

	if (context == NULL || context->runtime == NULL) {
		return -ENODEV;
	}
	if (parse_runtime_cap_request(request_bytes, request_len, &cap_request) != 0 ||
	    cap_request.has_value) {
		return -EINVAL;
	}
	if (cap_request.has_key) {
		int result = runtime_cap_format_line(context->runtime, cap_request.key,
						     line_storage[0], sizeof(line_storage[0]));
		if (result != 0) {
			return result;
		}
		lines[line_count++] = line_storage[0];
	} else {
		for (size_t i = 0; i < ARRAY_SIZE(runtime_cap_keys); i++) {
			int result = runtime_cap_format_line(context->runtime, runtime_cap_keys[i],
							     line_storage[i],
							     sizeof(line_storage[i]));
			if (result != 0) {
				return result;
			}
			lines[line_count++] = line_storage[i];
		}
	}
	return repeated_runtime_lines_response(request, context->runtime, lines, line_count,
					      response, response_cap, response_len);
}

static bool runtime_has_device_error_line(const struct sq_vm_runtime *runtime, const char *line)
{
	if (runtime == NULL || line == NULL) {
		return false;
	}
	for (size_t i = 0; i < runtime->device_error_count; i++) {
		if (strcmp(runtime->device_errors[i], line) == 0) {
			return true;
		}
	}
	return false;
}

static int protocol_session_invariant_line(char *line, size_t line_len, const char *name, int code)
{
	int written;

	if (line == NULL || line_len == 0 || name == NULL) {
		return -EINVAL;
	}
	written = snprintf(line, line_len, "invariant.protocol.%s code=%d (%s)", name, code,
			   sq_errno_name(code));
	if (written < 0 || (size_t)written >= line_len) {
		return -ENOSPC;
	}
	return code;
}

static int validate_transfer_session(bool active, const char *app_id, size_t total_len,
				     size_t received, enum sq_device_transfer_phase phase,
				     char *line, size_t line_len)
{
	if (phase > SQ_DEVICE_TRANSFER_COMMITTING) {
		return protocol_session_invariant_line(line, line_len, "session", -EINVAL);
	}
	if (!active) {
		return phase == SQ_DEVICE_TRANSFER_IDLE ? 0 :
						 protocol_session_invariant_line(line, line_len,
										 "session", -EINVAL);
	}
	if (!sq_app_store_is_safe_app_id(app_id) || total_len == 0 || received > total_len ||
	    phase == SQ_DEVICE_TRANSFER_IDLE) {
		return protocol_session_invariant_line(line, line_len, "session", -EINVAL);
	}
	return 0;
}

static int validate_context_protocol_invariants(
	const struct sq_device_protocol_context *context, char *line, size_t line_len)
{
	int result;

	if (context == NULL) {
		return 0;
	}
	if (context->scratch != NULL &&
	    context->scratch->owner > SQ_DEVICE_PROTOCOL_SCRATCH_STORAGE_FORMAT) {
		return protocol_session_invariant_line(line, line_len, "scratch", -EINVAL);
	}
	if (context->install_session != NULL) {
		result = validate_transfer_session(
			context->install_session->active, context->install_session->app_id,
			context->install_session->total_len, context->install_session->received,
			context->install_session->phase, line, line_len);
		if (result != 0) {
			return result;
		}
	}
	if (context->temp_session != NULL) {
		result = validate_transfer_session(
			context->temp_session->active, context->temp_session->app_id,
			context->temp_session->total_len, context->temp_session->received,
			context->temp_session->phase, line, line_len);
		if (result != 0) {
			return result;
		}
	}
	if (context->resource_session != NULL) {
		result = validate_transfer_session(
			context->resource_session->active, context->resource_session->app_id,
			context->resource_session->total_len,
			context->resource_session->received, context->resource_session->phase,
			line, line_len);
		if (result != 0) {
			return result;
		}
		if (context->resource_session->active &&
		    context->resource_session->resource_path[0] == '\0') {
			return protocol_session_invariant_line(line, line_len, "session",
							       -EINVAL);
		}
	}
	return 0;
}

static void record_context_invariant(const struct sq_device_protocol_context *context)
{
	char line[SQ_VM_RUNTIME_DEVICE_ERROR_LEN];
	int result;

	if (context == NULL || context->runtime == NULL) {
		return;
	}
	if (context->registry != NULL) {
		result = sq_app_registry_validate(context->registry, line, sizeof(line));
		if (result != 0) {
			if (!runtime_has_device_error_line(context->runtime, line)) {
				(void)sq_vm_runtime_record_device_error(context->runtime, line);
			}
			return;
		}
	}
	result = sq_vm_runtime_validate_invariants(context->runtime, line, sizeof(line));
	if (result != 0) {
		if (!runtime_has_device_error_line(context->runtime, line)) {
			(void)sq_vm_runtime_record_device_error(context->runtime, line);
		}
		return;
	}
	result = validate_context_protocol_invariants(context, line, sizeof(line));
	if (result != 0 && !runtime_has_device_error_line(context->runtime, line)) {
		(void)sq_vm_runtime_record_device_error(context->runtime, line);
	}
}

static int __noinline errors_response(const struct sq_protocol_request *request,
				      const struct sq_device_protocol_context *context,
				      uint8_t *response, size_t response_cap, size_t *response_len)
{
	const struct sq_vm_runtime *runtime = context == NULL ? NULL : context->runtime;
	const char *available[1 + SQ_VM_RUNTIME_DEVICE_ERROR_MAX];
	const char *lines[2 + SQ_VM_RUNTIME_DEVICE_ERROR_MAX];
	size_t available_count = 0;
	size_t line_count = 0;
	char error_line[64];
	char truncated_line[32];
	size_t omitted_count = 0;

	record_context_invariant(context);
	if (runtime != NULL) {
		for (size_t i = 0;
		     i < runtime->device_error_count && available_count < ARRAY_SIZE(available);
		     i++) {
			available[available_count++] = runtime->device_errors[i];
		}
	}
	if (runtime != NULL && runtime->status == SQ_VM_RUNTIME_ERROR) {
		const char *status_name = runtime->result.status == SQVM_STATUS_OK ?
						  "host_error" :
						  sq_vm_runtime_status_name(runtime->result.status);
		int written = snprintf(error_line, sizeof(error_line), "runtime=%s code=%d (%s)",
				       status_name, runtime->result_code,
				       sq_errno_name(runtime->result_code));
		if (written > 0 && (size_t)written < sizeof(error_line)) {
			available[available_count++] = error_line;
		}
	}
	if (response_cap < SQ_PROTOCOL_HEADER_LEN) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	size_t payload_cap = response_cap - SQ_PROTOCOL_HEADER_LEN;
	for (omitted_count = 0; omitted_count <= available_count; omitted_count++) {
		size_t required = 0;
		size_t retained_start = omitted_count;

		if (omitted_count > 0) {
			int written = snprintf(truncated_line, sizeof(truncated_line),
					       "errors_truncated=%zu", omitted_count);
			if (written < 0 || (size_t)written >= sizeof(truncated_line)) {
				return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
			}
			required += 4u + strlen(truncated_line);
		}
		for (size_t i = retained_start; i < available_count; i++) {
			required += 4u + strlen(available[i]);
		}
		if (required <= payload_cap) {
			break;
		}
	}
	if (omitted_count > available_count) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	if (omitted_count > 0) {
		lines[line_count++] = truncated_line;
	}
	for (size_t i = omitted_count; i < available_count; i++) {
		lines[line_count++] = available[i];
	}
	return repeated_runtime_lines_response(request, runtime, lines, line_count, response,
					      response_cap, response_len);
}

static int parse_display_window_probe_pattern(const uint8_t *request_bytes, size_t request_len,
					      char *pattern_out, size_t pattern_cap)
{
	const uint8_t *payload;
	uint32_t payload_len;
	size_t offset = 0;

	if (request_bytes == NULL || pattern_out == NULL || pattern_cap == 0 ||
	    request_len < SQ_PROTOCOL_HEADER_LEN) {
		return -EINVAL;
	}
	payload_len = read_u32_le_device(&request_bytes[12]);
	if ((size_t)payload_len > request_len - SQ_PROTOCOL_HEADER_LEN) {
		return -EINVAL;
	}
	payload = &request_bytes[SQ_PROTOCOL_HEADER_LEN];
	pattern_out[0] = '\0';
	while (offset < payload_len) {
		const uint8_t *field;
		uint8_t tag;
		uint8_t type;
		uint16_t len;
		size_t next_offset;

		if ((size_t)payload_len - offset < 4U) {
			return -EINVAL;
		}
		field = &payload[offset];
		tag = field[0];
		type = field[1];
		len = (uint16_t)field[2] | ((uint16_t)field[3] << 8);
		next_offset = offset + 4U + len;
		if (next_offset > payload_len) {
			return -EINVAL;
		}
		if (tag != SQ_DISPLAY_WINDOW_PROBE_FIELD_PATTERN ||
		    type != SQ_DEVICE_FIELD_TYPE_STRING || len == 0U || len >= pattern_cap) {
			return -EINVAL;
		}
		memcpy(pattern_out, &field[4], len);
		pattern_out[len] = '\0';
		offset = next_offset;
	}
	return pattern_out[0] == '\0' ? -EINVAL : 0;
}

static int display_window_probe(const struct sq_protocol_request *request,
				const uint8_t *request_bytes, size_t request_len, uint8_t *response,
				size_t response_cap, size_t *response_len)
{
	char pattern[32];
	int result;

	result = parse_display_window_probe_pattern(request_bytes, request_len, pattern,
						   sizeof(pattern));
	if (result != 0) {
		return result;
	}
	result = sq_display_backend_window_probe(pattern);
	if (result != 0) {
		return result;
	}
	return ok_response(request, response, response_cap, response_len);
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
	case SQ_OPCODE_CONTENT_INSTALL_BEGIN:
		result = begin_content_install(&frame, request, request_len, context, response,
					       response_cap, response_len);
		break;
	case SQ_OPCODE_CONTENT_INSTALL_CHUNK:
		result = append_content_chunk(&frame, request, request_len, context, response,
					      response_cap, response_len);
		break;
	case SQ_OPCODE_CONTENT_INSTALL_COMMIT:
		result = commit_content_install(&frame, context, response, response_cap,
						response_len);
		break;
	case SQ_OPCODE_CONTENT_CHECK:
		result = content_check_response(&frame, request, request_len, response, response_cap,
						response_len);
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
		result = errors_response(&frame, context, response, response_cap, response_len);
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
	case SQ_OPCODE_RUNTIME_CAP_GET:
		result = runtime_cap_get(&frame, request, request_len, context, response,
					 response_cap, response_len);
		break;
	case SQ_OPCODE_RUNTIME_CAP_SET:
		result = runtime_cap_set(&frame, request, request_len, context, response,
					 response_cap, response_len);
		break;
	case SQ_OPCODE_RUNTIME_CAP_CLEAR:
		result = runtime_cap_clear(&frame, request, request_len, context, response,
					   response_cap, response_len);
		break;
	case SQ_OPCODE_DISPLAY_WINDOW_PROBE:
		result = display_window_probe(&frame, request, request_len, response, response_cap,
					      response_len);
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
