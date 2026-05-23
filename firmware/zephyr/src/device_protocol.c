#include "device_protocol.h"

#include <errno.h>
#include <stdio.h>
#include <string.h>

#include <zephyr/fs/fs.h>
#include <zephyr/kernel.h>
#include <zephyr/sys/util.h>

#include "protocol.h"
#include "squidvm_ffi.h"

static int append_field(uint8_t *payload, size_t cap, size_t *len, uint8_t tag, uint8_t type,
			const uint8_t *value, uint16_t value_len)
{
	size_t needed = *len + 4u + value_len;

	if (needed > cap) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	payload[*len] = tag;
	payload[*len + 1u] = type;
	payload[*len + 2u] = value_len & 0xffu;
	payload[*len + 3u] = (value_len >> 8) & 0xffu;
	memcpy(&payload[*len + 4u], value, value_len);
	*len = needed;

	return SQ_PROTOCOL_OK;
}

static int append_string_field(uint8_t *payload, size_t cap, size_t *len, uint8_t tag,
			       const char *value)
{
	size_t value_len = strlen(value);

	if (value_len > UINT16_MAX) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	return append_field(payload, cap, len, tag, SQ_FIELD_STRING, (const uint8_t *)value,
			    (uint16_t)value_len);
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

static int append_u64_field(uint8_t *payload, size_t cap, size_t *len, uint8_t tag,
			    uint64_t value)
{
	uint8_t encoded[8] = {
		value & 0xffu,
		(value >> 8) & 0xffu,
		(value >> 16) & 0xffu,
		(value >> 24) & 0xffu,
		(value >> 32) & 0xffu,
		(value >> 40) & 0xffu,
		(value >> 48) & 0xffu,
		(value >> 56) & 0xffu,
	};

	return append_field(payload, cap, len, tag, SQ_FIELD_U64, encoded, sizeof(encoded));
}

static uint32_t crc32_update(uint32_t crc, const uint8_t *bytes, size_t len)
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

static int append_record_field(uint8_t *payload, size_t cap, size_t *len, uint8_t tag,
			       const uint8_t *record, uint16_t record_len)
{
	return append_field(payload, cap, len, tag, SQ_FIELD_RECORD, record, record_len);
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
	uint8_t payload[512];
	size_t payload_len = 0;
	int result;

	if (registry != NULL) {
		for (size_t i = 0; i < registry->count; i++) {
			uint8_t record[96];
			size_t record_len = 0;

			result = append_string_field(record, sizeof(record), &record_len,
						     SQ_DEVICE_APP_FIELD_ID,
						     registry->apps[i].app_id);
			if (result != SQ_PROTOCOL_OK) {
				return result;
			}
			result = append_u64_field(record, sizeof(record), &record_len,
						  SQ_DEVICE_APP_FIELD_SQBC_LEN,
						  registry->apps[i].sqbc_len);
			if (result != SQ_PROTOCOL_OK) {
				return result;
			}
			result = append_record_field(payload, sizeof(payload), &payload_len,
						     SQ_DEVICE_APP_LIST_FIELD_APP, record,
						     (uint16_t)record_len);
			if (result != SQ_PROTOCOL_OK) {
				return result;
			}
		}
	}

	if (response_cap < SQ_PROTOCOL_HEADER_LEN + payload_len) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	result = sq_protocol_encode_frame_header(SQ_FRAME_RESPONSE, SQ_OPCODE_APP_LIST,
						 SQ_STATUS_OK, request->sequence, payload,
						 payload_len, response, response_cap);
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	memcpy(&response[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	*response_len = SQ_PROTOCOL_HEADER_LEN + payload_len;

	return SQ_PROTOCOL_OK;
}

static int ok_response(const struct sq_protocol_frame *request, uint8_t *response,
		       size_t response_cap, size_t *response_len)
{
	return sqdp_status_to_protocol_result(sqdp_encode_empty_response(
		request->opcode, SQ_STATUS_OK, request->sequence, response, response_cap,
		response_len));
}

static int write_response(const struct sq_protocol_frame *request, uint8_t status,
			  const uint8_t *payload, size_t payload_len, uint8_t *response,
			  size_t response_cap, size_t *response_len)
{
	int result;

	if (response_cap < SQ_PROTOCOL_HEADER_LEN + payload_len) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	result = sq_protocol_encode_frame_header(SQ_FRAME_RESPONSE, request->opcode, status,
						 request->sequence, payload, payload_len,
						 response, response_cap);
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	if (payload_len > 0) {
		memcpy(&response[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	}
	*response_len = SQ_PROTOCOL_HEADER_LEN + payload_len;
	return SQ_PROTOCOL_OK;
}

static int error_response(const struct sq_protocol_frame *request, int code, uint8_t *response,
			  size_t response_cap, size_t *response_len)
{
	const char *message = code == -ENOTSUP ? "unsupported" :
			      code == -ENODEV  ? "device unavailable" :
			      code == -EINVAL  ? "invalid request" :
			      code == -EBUSY   ? "busy" :
						 "command failed";
	return sqdp_status_to_protocol_result(sqdp_encode_error_response(
		request->opcode, request->sequence, code, (const uint8_t *)message, strlen(message),
		response, response_cap, response_len));
}

static int begin_install(const struct sq_protocol_frame *request,
			 const struct sq_device_protocol_context *context, uint8_t *response,
			 size_t response_cap, size_t *response_len)
{
	const char *app_id = NULL;
	size_t app_id_len = 0;
	uint64_t total_len = 0;
	uint64_t crc32 = 0;
	size_t offset = 0;
	struct sq_protocol_field field;

	while (sq_protocol_next_field(request->payload, request->payload_len, &offset, &field) ==
	       SQ_PROTOCOL_OK) {
		if (field.tag == SQ_DEVICE_INSTALL_FIELD_APP_ID && field.type == SQ_FIELD_STRING) {
			app_id = (const char *)field.value;
			app_id_len = field.len;
		} else if (field.tag == SQ_DEVICE_INSTALL_FIELD_TOTAL_LEN &&
			   field.type == SQ_FIELD_U64 && field.len == 8) {
			total_len = sq_protocol_read_u64_le(field.value);
		} else if (field.tag == SQ_DEVICE_INSTALL_FIELD_CRC32 && field.type == SQ_FIELD_U64 &&
			   field.len == 8) {
			crc32 = sq_protocol_read_u64_le(field.value);
		}
	}

	if (app_id == NULL || app_id_len == 0 || crc32 > UINT32_MAX) {
		return -EINVAL;
	}

	if (request->opcode == SQ_OPCODE_TEMP_RUN_BEGIN) {
		struct sq_device_temp_session *session = context->temp_session;

		if (session == NULL || context->store_mount_point == NULL) {
			return -ENODEV;
		}
		if (app_id_len >= sizeof(session->app_id) || total_len == 0 ||
		    total_len > SQ_DEVICE_TEMP_RUN_MAX_BYTES) {
			return -EINVAL;
		}

		memset(session, 0, sizeof(*session));
		memcpy(session->app_id, app_id, app_id_len);
		session->app_id[app_id_len] = '\0';
		session->active = true;
		session->total_len = (size_t)total_len;
		session->expected_crc = (uint32_t)crc32;
		session->running_crc = 0xffffffffu;
		int result = sq_app_store_begin_temp_run(context->store_mount_point,
							 session->staging_path,
							 sizeof(session->staging_path));
		if (result != 0) {
			memset(session, 0, sizeof(*session));
			return result;
		}
		return ok_response(request, response, response_cap, response_len);
	}

	struct sq_device_install_session *session = context->install_session;

	if (session == NULL || context->store_mount_point == NULL) {
		return -ENODEV;
	}
	if (app_id_len >= sizeof(session->app_id) || total_len == 0 ||
	    total_len > SQ_DEVICE_INSTALL_MAX_BYTES) {
		return -EINVAL;
	}

	memset(session, 0, sizeof(*session));
	memcpy(session->app_id, app_id, app_id_len);
	session->app_id[app_id_len] = '\0';
	session->total_len = (size_t)total_len;
	session->expected_crc = (uint32_t)crc32;
	session->running_crc = 0xffffffffu;

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
				const struct sq_device_protocol_context *context,
				uint8_t *response, size_t response_cap, size_t *response_len)
{
	const uint8_t *bytes = NULL;
	size_t bytes_len = 0;
	uint64_t chunk_offset = UINT64_MAX;
	size_t offset = 0;
	struct sq_protocol_field field;

	while (sq_protocol_next_field(request->payload, request->payload_len, &offset, &field) ==
	       SQ_PROTOCOL_OK) {
		if (field.tag == SQ_DEVICE_CHUNK_FIELD_OFFSET && field.type == SQ_FIELD_U64 &&
		    field.len == 8) {
			chunk_offset = sq_protocol_read_u64_le(field.value);
		} else if (field.tag == SQ_DEVICE_CHUNK_FIELD_BYTES && field.type == SQ_FIELD_BYTES) {
			bytes = field.value;
			bytes_len = field.len;
		}
	}

	if (request->opcode == SQ_OPCODE_TEMP_RUN_CHUNK) {
		struct sq_device_temp_session *session = context->temp_session;

		if (session == NULL || !session->active || bytes == NULL ||
		    chunk_offset != session->received ||
		    session->received + bytes_len > session->total_len) {
			return -EINVAL;
		}

		int result = sq_app_store_write_staged_chunk(session->staging_path,
							     session->received, bytes,
							     bytes_len);
		if (result != 0) {
			return result;
		}
		session->running_crc = crc32_update(session->running_crc, bytes, bytes_len);
		session->received += bytes_len;
		return ok_response(request, response, response_cap, response_len);
	}

	struct sq_device_install_session *session = context->install_session;

	if (session == NULL || !session->active || bytes == NULL ||
	    chunk_offset != session->received || session->received + bytes_len > session->total_len) {
		return -EINVAL;
	}

	int result = sq_app_store_write_staged_chunk(session->staging_path, session->received, bytes,
						    bytes_len);
	if (result != 0) {
		return result;
	}
	session->running_crc = crc32_update(session->running_crc, bytes, bytes_len);
	session->received += bytes_len;

	return ok_response(request, response, response_cap, response_len);
}

static int begin_resource_install(const struct sq_protocol_frame *request,
				  const struct sq_device_protocol_context *context,
				  uint8_t *response, size_t response_cap, size_t *response_len)
{
	const char *app_id = NULL;
	const char *resource_path = NULL;
	size_t app_id_len = 0;
	size_t resource_path_len = 0;
	uint64_t total_len = 0;
	uint64_t crc32 = 0;
	size_t offset = 0;
	struct sq_protocol_field field;

	while (sq_protocol_next_field(request->payload, request->payload_len, &offset, &field) ==
	       SQ_PROTOCOL_OK) {
		if (field.tag == SQ_DEVICE_RESOURCE_FIELD_APP_ID && field.type == SQ_FIELD_STRING) {
			app_id = (const char *)field.value;
			app_id_len = field.len;
		} else if (field.tag == SQ_DEVICE_RESOURCE_FIELD_PATH &&
			   field.type == SQ_FIELD_STRING) {
			resource_path = (const char *)field.value;
			resource_path_len = field.len;
		} else if (field.tag == SQ_DEVICE_RESOURCE_FIELD_TOTAL_LEN &&
			   field.type == SQ_FIELD_U64 && field.len == 8) {
			total_len = sq_protocol_read_u64_le(field.value);
		} else if (field.tag == SQ_DEVICE_RESOURCE_FIELD_CRC32 &&
			   field.type == SQ_FIELD_U64 && field.len == 8) {
			crc32 = sq_protocol_read_u64_le(field.value);
		}
	}

	struct sq_device_resource_session *session = context->resource_session;
	if (session == NULL || context->store_mount_point == NULL) {
		return -ENODEV;
	}
	if (app_id == NULL || resource_path == NULL || app_id_len == 0 ||
	    app_id_len >= sizeof(session->app_id) || resource_path_len == 0 ||
	    resource_path_len >= sizeof(session->resource_path) || total_len == 0 ||
	    total_len > SQ_DEVICE_INSTALL_MAX_BYTES || crc32 > UINT32_MAX) {
		return -EINVAL;
	}

	memset(session, 0, sizeof(*session));
	memcpy(session->app_id, app_id, app_id_len);
	session->app_id[app_id_len] = '\0';
	memcpy(session->resource_path, resource_path, resource_path_len);
	session->resource_path[resource_path_len] = '\0';
	session->total_len = (size_t)total_len;
	session->expected_crc = (uint32_t)crc32;
	session->running_crc = 0xffffffffu;

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
				 const struct sq_device_protocol_context *context,
				 uint8_t *response, size_t response_cap, size_t *response_len)
{
	const uint8_t *bytes = NULL;
	size_t bytes_len = 0;
	uint64_t chunk_offset = UINT64_MAX;
	size_t offset = 0;
	struct sq_protocol_field field;

	while (sq_protocol_next_field(request->payload, request->payload_len, &offset, &field) ==
	       SQ_PROTOCOL_OK) {
		if (field.tag == SQ_DEVICE_CHUNK_FIELD_OFFSET && field.type == SQ_FIELD_U64 &&
		    field.len == 8) {
			chunk_offset = sq_protocol_read_u64_le(field.value);
		} else if (field.tag == SQ_DEVICE_CHUNK_FIELD_BYTES && field.type == SQ_FIELD_BYTES) {
			bytes = field.value;
			bytes_len = field.len;
		}
	}

	struct sq_device_resource_session *session = context->resource_session;
	if (session == NULL || !session->active || bytes == NULL ||
	    chunk_offset != session->received || session->received + bytes_len > session->total_len) {
		return -EINVAL;
	}

	int result = sq_app_store_write_staged_chunk(session->staging_path, session->received, bytes,
						    bytes_len);
	if (result != 0) {
		return result;
	}
	session->running_crc = crc32_update(session->running_crc, bytes, bytes_len);
	session->received += bytes_len;
	return ok_response(request, response, response_cap, response_len);
}

static int commit_resource_install(const struct sq_protocol_frame *request,
				   const struct sq_device_protocol_context *context,
				   uint8_t *response, size_t response_cap, size_t *response_len)
{
	struct sq_device_resource_session *session = context->resource_session;
	if (session == NULL || !session->active || context->store_mount_point == NULL ||
	    session->received != session->total_len ||
	    ~session->running_crc != session->expected_crc) {
		return -EINVAL;
	}

	int result = sq_app_store_commit_staged_resource(context->store_mount_point,
							session->app_id,
							session->resource_path,
							session->staging_path);
	if (result != 0) {
		return result;
	}
	memset(session, 0, sizeof(*session));
	return ok_response(request, response, response_cap, response_len);
}

struct temp_storage_backend {
	const char *sqbc_path;
	size_t sqbc_len;
	uint8_t state[SQVM_STORAGE_TRANSFER_CAPACITY];
	size_t state_len;
	bool state_present;
};

static int temp_read_file_exact(const char *path, size_t offset, uint8_t *out, size_t len)
{
	struct fs_file_t file;
	int result;

	if (path == NULL || out == NULL) {
		return -EINVAL;
	}

	fs_file_t_init(&file);
	result = fs_open(&file, path, FS_O_READ);
	if (result != 0) {
		return result;
	}

	result = fs_seek(&file, (off_t)offset, FS_SEEK_SET);
	if (result != 0) {
		(void)fs_close(&file);
		return result;
	}

	ssize_t read = fs_read(&file, out, len);
	result = fs_close(&file);
	if (read < 0) {
		return (int)read;
	}
	if ((size_t)read != len) {
		return -EIO;
	}
	return result;
}

static int temp_read_sqbc(void *user_data, size_t offset, uint8_t *out, size_t len)
{
	struct temp_storage_backend *storage = user_data;

	if (storage == NULL || offset > storage->sqbc_len || len > storage->sqbc_len - offset) {
		return -EINVAL;
	}
	return temp_read_file_exact(storage->sqbc_path, offset, out, len);
}

static int temp_load_state(void *user_data, uint8_t *out, size_t out_len, size_t *len)
{
	struct temp_storage_backend *storage = user_data;

	if (!storage->state_present) {
		*len = 0;
		return 0;
	}
	if (storage->state_len > out_len) {
		return -ENOSPC;
	}
	memcpy(out, storage->state, storage->state_len);
	*len = storage->state_len;
	return 0;
}

static int temp_save_state(void *user_data, const uint8_t *bytes, size_t len)
{
	struct temp_storage_backend *storage = user_data;

	if (len > sizeof(storage->state)) {
		return -ENOSPC;
	}
	memcpy(storage->state, bytes, len);
	storage->state_len = len;
	storage->state_present = true;
	return 0;
}

static int temp_reset_state(void *user_data)
{
	struct temp_storage_backend *storage = user_data;

	storage->state_len = 0;
	storage->state_present = false;
	return 0;
}

static int commit_temp_run(const struct sq_protocol_frame *request,
			   const struct sq_device_protocol_context *context, uint8_t *response,
			   size_t response_cap, size_t *response_len)
{
	struct sq_device_temp_session *session = context->temp_session;
	static struct temp_storage_backend temp_storage;
	struct sq_vm_storage_backend backend;
	int result;

	if (session == NULL || !session->active || context->runtime == NULL ||
	    session->received != session->total_len ||
	    ~session->running_crc != session->expected_crc) {
		return -EINVAL;
	}

	memset(&temp_storage, 0, sizeof(temp_storage));
	temp_storage.sqbc_path = session->staging_path;
	temp_storage.sqbc_len = session->total_len;
	backend = (struct sq_vm_storage_backend){
		.user_data = &temp_storage,
		.read_sqbc = temp_read_sqbc,
		.load_state = temp_load_state,
		.save_state = temp_save_state,
		.reset_state = temp_reset_state,
	};
	result = sq_vm_runtime_start(context->runtime, &backend, "app.start");
	if (result != 0) {
		return result;
	}

	return ok_response(request, response, response_cap, response_len);
}

static int commit_install(const struct sq_protocol_frame *request,
			  const struct sq_device_protocol_context *context, uint8_t *response,
			  size_t response_cap, size_t *response_len)
{
	struct sq_device_install_session *session = context->install_session;
	int result;

	if (session == NULL || !session->active || context->store_mount_point == NULL ||
	    session->received != session->total_len ||
	    ~session->running_crc != session->expected_crc) {
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

	memset(session, 0, sizeof(*session));
	return ok_response(request, response, response_cap, response_len);
}

static int launch_app(const struct sq_protocol_frame *request,
		      const struct sq_device_protocol_context *context, uint8_t *response,
		      size_t response_cap, size_t *response_len)
{
	const char *app_id = NULL;
	size_t app_id_len = 0;
	size_t offset = 0;
	struct sq_protocol_field field;
	struct sq_vm_storage_backend backend;
	int result;

	if (context->runtime == NULL || context->store_mount_point == NULL ||
	    context->launch_storage == NULL) {
		return -ENODEV;
	}

	while (sq_protocol_next_field(request->payload, request->payload_len, &offset, &field) ==
	       SQ_PROTOCOL_OK) {
		if (field.tag == 1 && field.type == SQ_FIELD_STRING) {
			app_id = (const char *)field.value;
			app_id_len = field.len;
		}
	}
	if (app_id == NULL || app_id_len == 0 || app_id_len >= SQ_APP_STORE_APP_ID_MAX) {
		return -EINVAL;
	}

	char app_id_buffer[SQ_APP_STORE_APP_ID_MAX];
	memcpy(app_id_buffer, app_id, app_id_len);
	app_id_buffer[app_id_len] = '\0';

	result = sq_app_store_vm_storage_for_app(context->store_mount_point, app_id_buffer,
						context->launch_storage);
	if (result != 0) {
		return result;
	}
	backend = sq_app_store_vm_storage_backend(context->launch_storage);
	result = sq_vm_runtime_start(context->runtime, &backend, "app.start");
	if (result != 0) {
		return result;
	}

	return ok_response(request, response, response_cap, response_len);
}

static int repeated_runtime_lines_response(const struct sq_protocol_frame *request,
					   const struct sq_vm_runtime *runtime,
					   const char *const *extra_lines, size_t extra_count,
					   uint8_t *response, size_t response_cap,
					   size_t *response_len)
{
	uint8_t payload[512];
	size_t payload_len = 0;
	int result;

	if (runtime != NULL && request->opcode == SQ_OPCODE_TRACE_GET) {
		for (size_t i = 0; i < runtime->trace_count; i++) {
			result = append_string_field(payload, sizeof(payload), &payload_len,
						     SQ_DEVICE_LINE_FIELD_VALUE,
						     runtime->traces[i]);
			if (result != SQ_PROTOCOL_OK) {
				return result;
			}
		}
	}
	for (size_t i = 0; i < extra_count; i++) {
		result = append_string_field(payload, sizeof(payload), &payload_len,
					     SQ_DEVICE_LINE_FIELD_VALUE, extra_lines[i]);
		if (result != SQ_PROTOCOL_OK) {
			return result;
		}
	}
	return write_response(request, SQ_STATUS_OK, payload, payload_len, response, response_cap,
			      response_len);
}

static int state_get_response(const struct sq_protocol_frame *request,
			      const struct sq_device_protocol_context *context, uint8_t *response,
			      size_t response_cap, size_t *response_len)
{
	size_t bytes_len = 0;
	size_t payload_cap;
	uint8_t *payload;
	int result;

	if (context->launch_storage == NULL) {
		return write_response(request, SQ_STATUS_OK, NULL, 0, response, response_cap,
				      response_len);
	}
	if (response_cap < SQ_PROTOCOL_HEADER_LEN + 4) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	struct sq_vm_storage_backend backend =
		sq_app_store_vm_storage_backend(context->launch_storage);
	if (backend.load_state == NULL) {
		return -ENODEV;
	}

	payload = &response[SQ_PROTOCOL_HEADER_LEN];
	payload_cap = MIN(response_cap - SQ_PROTOCOL_HEADER_LEN - 4,
			  (size_t)SQVM_STORAGE_TRANSFER_CAPACITY);
	result = backend.load_state(backend.user_data, &payload[4], payload_cap, &bytes_len);
	if (result != 0 && result != -ENOENT) {
		return result;
	}
	if (result == -ENOENT) {
		bytes_len = 0;
	}
	if (bytes_len > payload_cap || bytes_len > UINT16_MAX) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	payload[0] = SQ_DEVICE_STATE_FIELD_BYTES;
	payload[1] = SQ_FIELD_BYTES;
	payload[2] = (uint8_t)(bytes_len & 0xffu);
	payload[3] = (uint8_t)((bytes_len >> 8) & 0xffu);
	result = sq_protocol_encode_frame_header(SQ_FRAME_RESPONSE, request->opcode, SQ_STATUS_OK,
						 request->sequence, payload, bytes_len + 4,
						 response, response_cap);
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	*response_len = SQ_PROTOCOL_HEADER_LEN + bytes_len + 4;
	return SQ_PROTOCOL_OK;
}

static int state_import(const struct sq_protocol_frame *request,
			const struct sq_device_protocol_context *context, uint8_t *response,
			size_t response_cap, size_t *response_len)
{
	const uint8_t *bytes = NULL;
	size_t bytes_len = 0;
	size_t offset = 0;
	struct sq_protocol_field field;

	while (sq_protocol_next_field(request->payload, request->payload_len, &offset, &field) ==
	       SQ_PROTOCOL_OK) {
		if (field.tag == SQ_DEVICE_STATE_FIELD_BYTES && field.type == SQ_FIELD_BYTES) {
			bytes = field.value;
			bytes_len = field.len;
		}
	}
	if (bytes == NULL || context->launch_storage == NULL) {
		return -EINVAL;
	}
	struct sq_vm_storage_backend backend =
		sq_app_store_vm_storage_backend(context->launch_storage);
	if (backend.save_state == NULL) {
		return -ENODEV;
	}
	int result = backend.save_state(backend.user_data, bytes, bytes_len);
	if (result != 0) {
		return result;
	}
	return ok_response(request, response, response_cap, response_len);
}

static int resources_response(const struct sq_protocol_frame *request,
			      const struct sq_device_protocol_context *context, uint8_t *response,
			      size_t response_cap, size_t *response_len)
{
	uint8_t payload[384];
	size_t payload_len = 0;
	int result;
	struct {
		const char *key;
		uint64_t value;
	} entries[] = {
		{"ram_total_bytes", CONFIG_SRAM_SIZE * 1024u},
		{"runtime_static_bytes", context->runtime == NULL ? 0 : sizeof(*context->runtime)},
		{"install_session_bytes",
		 context->install_session == NULL ? 0 : sizeof(*context->install_session)},
		{"temp_session_bytes",
		 context->temp_session == NULL ? 0 : sizeof(*context->temp_session)},
		{"resource_session_bytes",
		 context->resource_session == NULL ? 0 : sizeof(*context->resource_session)},
		{"app_count", context->registry == NULL ? 0 : context->registry->count},
	};

	for (size_t i = 0; i < ARRAY_SIZE(entries); i++) {
		uint8_t record[96];
		size_t record_len = 0;
		result = append_string_field(record, sizeof(record), &record_len,
					     SQ_DEVICE_RECORD_FIELD_KEY, entries[i].key);
		if (result != SQ_PROTOCOL_OK) {
			return result;
		}
		result = append_u64_field(record, sizeof(record), &record_len,
					  SQ_DEVICE_RECORD_FIELD_VALUE, entries[i].value);
		if (result != SQ_PROTOCOL_OK) {
			return result;
		}
		result = append_record_field(payload, sizeof(payload), &payload_len,
					     SQ_DEVICE_RECORD_FIELD_ENTRY, record,
					     (uint16_t)record_len);
		if (result != SQ_PROTOCOL_OK) {
			return result;
		}
	}
	return write_response(request, SQ_STATUS_OK, payload, payload_len, response, response_cap,
			      response_len);
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

static int dispatch_event_request(const struct sq_protocol_frame *request,
				  const struct sq_device_protocol_context *context,
				  uint8_t *response, size_t response_cap, size_t *response_len)
{
	const char *app_id = NULL;
	const char *event = NULL;
	size_t app_id_len = 0;
	size_t event_len = 0;
	size_t offset = 0;
	struct sq_protocol_field field;

	while (sq_protocol_next_field(request->payload, request->payload_len, &offset, &field) ==
	       SQ_PROTOCOL_OK) {
		if (field.tag == 1 && field.type == SQ_FIELD_STRING) {
			app_id = (const char *)field.value;
			app_id_len = field.len;
		} else if (field.tag == 2 && field.type == SQ_FIELD_STRING) {
			event = (const char *)field.value;
			event_len = field.len;
		}
	}
	if (event == NULL || event_len == 0 || event_len >= SQ_VM_RUNTIME_EVENT_LEN ||
	    context->runtime == NULL || context->store_mount_point == NULL ||
	    context->launch_storage == NULL) {
		return -EINVAL;
	}
	if (request->opcode == SQ_OPCODE_EVENT_DISPATCH) {
		if (app_id == NULL || app_id_len == 0 || app_id_len >= SQ_APP_STORE_APP_ID_MAX) {
			return -EINVAL;
		}
		char app_id_buffer[SQ_APP_STORE_APP_ID_MAX];
		memcpy(app_id_buffer, app_id, app_id_len);
		app_id_buffer[app_id_len] = '\0';
		int result = sq_app_store_vm_storage_for_app(context->store_mount_point,
							     app_id_buffer,
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

static int dispatch_key(const struct sq_protocol_frame *request,
			const struct sq_device_protocol_context *context, uint8_t *response,
			size_t response_cap, size_t *response_len)
{
	const char *key = NULL;
	size_t key_len = 0;
	size_t offset = 0;
	struct sq_protocol_field field;
	while (sq_protocol_next_field(request->payload, request->payload_len, &offset, &field) ==
	       SQ_PROTOCOL_OK) {
		if (field.tag == 1 && field.type == SQ_FIELD_STRING) {
			key = (const char *)field.value;
			key_len = field.len;
		}
	}
	if (key == NULL || key_len == 0 || key_len + 4 >= SQ_VM_RUNTIME_EVENT_LEN) {
		return -EINVAL;
	}
	char event[SQ_VM_RUNTIME_EVENT_LEN];
	memcpy(event, "key.", 4);
	memcpy(&event[4], key, key_len);
	event[4 + key_len] = '\0';
	struct sq_protocol_frame event_request = *request;
	uint8_t event_payload[64];
	size_t event_payload_len = 0;
	int result = append_string_field(event_payload, sizeof(event_payload), &event_payload_len, 2,
					 event);
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	event_request.opcode = SQ_OPCODE_KEY;
	event_request.payload = event_payload;
	event_request.payload_len = event_payload_len;
	return dispatch_event_request(&event_request, context, response, response_cap, response_len);
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
		result = begin_install(&frame, context, response, response_cap, response_len);
		break;
	case SQ_OPCODE_APP_INSTALL_CHUNK:
	case SQ_OPCODE_TEMP_RUN_CHUNK:
		result = append_install_chunk(&frame, context, response, response_cap, response_len);
		break;
	case SQ_OPCODE_APP_INSTALL_COMMIT:
		result = commit_install(&frame, context, response, response_cap, response_len);
		break;
	case SQ_OPCODE_RESOURCE_INSTALL_BEGIN:
		result = begin_resource_install(&frame, context, response, response_cap,
						response_len);
		break;
	case SQ_OPCODE_RESOURCE_INSTALL_CHUNK:
		result = append_resource_chunk(&frame, context, response, response_cap,
					       response_len);
		break;
	case SQ_OPCODE_RESOURCE_INSTALL_COMMIT:
		result = commit_resource_install(&frame, context, response, response_cap,
						 response_len);
		break;
	case SQ_OPCODE_TEMP_RUN_COMMIT:
		result = commit_temp_run(&frame, context, response, response_cap, response_len);
		break;
	case SQ_OPCODE_APP_LAUNCH:
		result = launch_app(&frame, context, response, response_cap, response_len);
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
			int written = snprintf(error_line, sizeof(error_line), "runtime=%d",
					       context->runtime->result_code);
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
		result = state_import(&frame, context, response, response_cap, response_len);
		break;
	case SQ_OPCODE_RESOURCES_GET:
		result = resources_response(&frame, context, response, response_cap, response_len);
		break;
	case SQ_OPCODE_RESET:
		result = reset_runtime(&frame, context, response, response_cap, response_len);
		break;
	case SQ_OPCODE_STORAGE_FORMAT:
		result = storage_format(&frame, context, response, response_cap, response_len);
		break;
	case SQ_OPCODE_EVENT_DISPATCH:
		result = dispatch_event_request(&frame, context, response, response_cap,
						response_len);
		break;
	case SQ_OPCODE_KEY:
		result = dispatch_key(&frame, context, response, response_cap, response_len);
		break;
	case SQ_OPCODE_WIFI_PROFILE_SET:
		result = -ENOTSUP;
		break;
	default:
		return SQ_PROTOCOL_ERR_BAD_MAGIC;
	}
	if (result != SQ_PROTOCOL_OK) {
		return error_response(&frame, result, response, response_cap, response_len);
	}
	return SQ_PROTOCOL_OK;
}
