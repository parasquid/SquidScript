#include "device_protocol.h"

#include <errno.h>
#include <string.h>

#include <zephyr/fs/fs.h>

#include "protocol.h"

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

static int append_bool_field(uint8_t *payload, size_t cap, size_t *len, uint8_t tag, bool value)
{
	uint8_t encoded = value ? 1u : 0u;

	return append_field(payload, cap, len, tag, SQ_FIELD_BOOL, &encoded, 1u);
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
	uint8_t payload[96];
	size_t payload_len = 0;
	int result;

	result = append_string_field(payload, sizeof(payload), &payload_len, SQ_DEVICE_FIELD_TARGET,
				     identity->target);
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	result = append_string_field(payload, sizeof(payload), &payload_len, SQ_DEVICE_FIELD_FIRMWARE,
				     identity->firmware);
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	result = append_bool_field(payload, sizeof(payload), &payload_len, SQ_DEVICE_FIELD_DIAGNOSTIC,
				   identity->diagnostic);
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}

	if (response_cap < SQ_PROTOCOL_HEADER_LEN + payload_len) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	result = sq_protocol_encode_frame_header(SQ_FRAME_RESPONSE, SQ_OPCODE_HELLO, SQ_STATUS_OK,
						 request->sequence, payload, payload_len, response,
						 response_cap);
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	memcpy(&response[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	*response_len = SQ_PROTOCOL_HEADER_LEN + payload_len;

	return SQ_PROTOCOL_OK;
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
	int result;

	if (response_cap < SQ_PROTOCOL_HEADER_LEN) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	result = sq_protocol_encode_frame_header(SQ_FRAME_RESPONSE, request->opcode, SQ_STATUS_OK,
						 request->sequence, NULL, 0, response,
						 response_cap);
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	*response_len = SQ_PROTOCOL_HEADER_LEN;
	return SQ_PROTOCOL_OK;
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
		return hello_response(&frame, context->identity, response, response_cap,
				      response_len);
	case SQ_OPCODE_APP_LIST:
		return app_list_response(&frame, context->registry, response, response_cap,
					 response_len);
	case SQ_OPCODE_APP_INSTALL_BEGIN:
	case SQ_OPCODE_TEMP_RUN_BEGIN:
		return begin_install(&frame, context, response, response_cap, response_len);
	case SQ_OPCODE_APP_INSTALL_CHUNK:
	case SQ_OPCODE_TEMP_RUN_CHUNK:
		return append_install_chunk(&frame, context, response, response_cap, response_len);
	case SQ_OPCODE_APP_INSTALL_COMMIT:
		return commit_install(&frame, context, response, response_cap, response_len);
	case SQ_OPCODE_TEMP_RUN_COMMIT:
		return commit_temp_run(&frame, context, response, response_cap, response_len);
	case SQ_OPCODE_APP_LAUNCH:
		return launch_app(&frame, context, response, response_cap, response_len);
	case SQ_OPCODE_OUTPUT_GET:
		return ok_response(&frame, response, response_cap, response_len);
	default:
		return SQ_PROTOCOL_ERR_BAD_MAGIC;
	}
}
