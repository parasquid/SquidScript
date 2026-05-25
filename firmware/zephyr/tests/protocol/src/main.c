#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include <zephyr/fs/fs.h>
#include <zephyr/ztest.h>

#include "app_store.h"
#include "device_protocol.h"
#include "protocol.h"
#include "serial_transport.h"
#include "squidscript_target_defaults.h"
#include "vm_runtime.h"
#include "vm_fs_storage.h"
#include "squidvm_ffi.h"
#include "vm_storage.h"

#define SQ_PROTOCOL_DONE 1

enum sq_test_field_type {
	SQ_FIELD_BYTES = 0,
	SQ_FIELD_STRING = 1,
	SQ_FIELD_BOOL = 3,
	SQ_FIELD_I64 = 4,
	SQ_FIELD_U64 = 5,
	SQ_FIELD_RECORD = 32,
};

struct sq_protocol_field {
	uint8_t tag;
	uint8_t type;
	const uint8_t *value;
	uint16_t len;
};

static bool runtime_has_active_binding(const struct sq_vm_runtime *runtime, const char *alias)
{
	if (runtime == NULL || alias == NULL) {
		return false;
	}
	for (size_t i = 0; i < runtime->active_binding_count; i++) {
		if (runtime->active_bindings[i].active &&
		    strcmp(runtime->active_bindings[i].alias, alias) == 0) {
			return true;
		}
	}
	return false;
}

static void write_u32_le(uint8_t *bytes, uint32_t value)
{
	bytes[0] = value & 0xff;
	bytes[1] = (value >> 8) & 0xff;
	bytes[2] = (value >> 16) & 0xff;
	bytes[3] = (value >> 24) & 0xff;
}

static int sq_protocol_next_field(const uint8_t *payload, size_t payload_len, size_t *offset,
				  struct sq_protocol_field *out)
{
	if (*offset == payload_len) {
		return SQ_PROTOCOL_DONE;
	}
	if (*offset > payload_len || payload_len - *offset < 4u) {
		return SQ_PROTOCOL_ERR_TRUNCATED_FIELD;
	}

	const uint8_t *field = &payload[*offset];
	uint16_t field_len = (uint16_t)field[2] | ((uint16_t)field[3] << 8);
	size_t next_offset = *offset + 4u + field_len;

	if (next_offset > payload_len) {
		return SQ_PROTOCOL_ERR_TRUNCATED_FIELD;
	}

	out->tag = field[0];
	out->type = field[1];
	out->len = field_len;
	out->value = &field[4];
	*offset = next_offset;

	return SQ_PROTOCOL_OK;
}

static uint64_t sq_protocol_read_u64_le(const uint8_t *bytes)
{
	uint64_t value = 0;

	for (int i = 7; i >= 0; i--) {
		value <<= 8;
		value |= bytes[i];
	}

	return value;
}

static int sq_protocol_append_bytes_field(uint8_t *payload, size_t cap, size_t *len, uint8_t tag,
					  const uint8_t *value, uint16_t value_len)
{
	size_t needed = *len + 4u + value_len;

	if (payload == NULL || len == NULL || value == NULL) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	if (needed > cap) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	payload[*len] = tag;
	payload[*len + 1u] = SQ_FIELD_BYTES;
	payload[*len + 2u] = value_len & 0xffu;
	payload[*len + 3u] = (value_len >> 8) & 0xffu;
	memcpy(&payload[*len + 4u], value, value_len);
	*len = needed;
	return SQ_PROTOCOL_OK;
}

static int sq_protocol_append_string_field(uint8_t *payload, size_t cap, size_t *len, uint8_t tag,
					   const char *value)
{
	size_t value_len;
	size_t needed;

	if (payload == NULL || len == NULL || value == NULL) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	value_len = strlen(value);
	if (value_len > UINT16_MAX) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	needed = *len + 4u + value_len;
	if (needed > cap) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	payload[*len] = tag;
	payload[*len + 1u] = SQ_FIELD_STRING;
	payload[*len + 2u] = value_len & 0xffu;
	payload[*len + 3u] = (value_len >> 8) & 0xffu;
	memcpy(&payload[*len + 4u], value, value_len);
	*len = needed;
	return SQ_PROTOCOL_OK;
}

static int sq_protocol_append_u64_field(uint8_t *payload, size_t cap, size_t *len, uint8_t tag,
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

	size_t needed = *len + 4u + sizeof(encoded);
	if (payload == NULL || len == NULL || needed > cap) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	payload[*len] = tag;
	payload[*len + 1u] = SQ_FIELD_U64;
	payload[*len + 2u] = sizeof(encoded);
	payload[*len + 3u] = 0;
	memcpy(&payload[*len + 4u], encoded, sizeof(encoded));
	*len = needed;
	return SQ_PROTOCOL_OK;
}

static int sq_protocol_encode_frame_header(uint8_t kind, uint8_t opcode, uint8_t status,
					   uint32_t sequence, const uint8_t *payload,
					   size_t payload_len, uint8_t *out, size_t out_len)
{
	if (out_len < SQ_PROTOCOL_HEADER_LEN) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	memcpy(out, "SQDP", 4);
	out[4] = kind;
	out[5] = opcode;
	out[6] = status;
	out[7] = 0;
	write_u32_le(&out[8], sequence);
	write_u32_le(&out[12], (uint32_t)payload_len);
	write_u32_le(&out[16], sq_protocol_crc32(payload, payload_len));

	return SQ_PROTOCOL_OK;
}

static const uint8_t hello_frame[] = {
	0x53, 0x51, 0x44, 0x50, 0x01, 0x01, 0x00, 0x00,
	0x07, 0x00, 0x00, 0x00, 0x26, 0x00, 0x00, 0x00,
	0x43, 0xa5, 0x05, 0x5c, 0x01, 0x01, 0x11, 0x00,
	0x65, 0x73, 0x70, 0x33, 0x32, 0x63, 0x33, 0x2d,
	0x73, 0x75, 0x70, 0x65, 0x72, 0x6d, 0x69, 0x6e,
	0x69, 0x02, 0x03, 0x01, 0x00, 0x01, 0x03, 0x05,
	0x08, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x00,
};

static const uint8_t headless_counter_sqbc[] = {
	0x53, 0x51, 0x42, 0x43, 0x6e, 0x00, 0x72, 0x01, 0x00, 0x00, 0x08, 0x00,
	0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x6e, 0x00, 0x00, 0x00, 0x1b, 0x00,
	0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x89, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x8b, 0x00, 0x00, 0x00, 0x59, 0x00,
	0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0xe4, 0x00, 0x00, 0x00, 0x1d, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x03, 0x01, 0x00, 0x00, 0x26, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x29, 0x01, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x35, 0x01, 0x00, 0x00, 0x3d, 0x00,
	0x00, 0x00, 0x10, 0x00, 0x68, 0x65, 0x61, 0x64, 0x6c, 0x65, 0x73, 0x73,
	0x2d, 0x63, 0x6f, 0x75, 0x6e, 0x74, 0x65, 0x72, 0x07, 0x00, 0x64, 0x65,
	0x66, 0x61, 0x75, 0x6c, 0x74, 0x00, 0x00, 0x08, 0x00, 0x10, 0x00, 0x68,
	0x65, 0x61, 0x64, 0x6c, 0x65, 0x73, 0x73, 0x2d, 0x63, 0x6f, 0x75, 0x6e,
	0x74, 0x65, 0x72, 0x0c, 0x00, 0x73, 0x74, 0x61, 0x74, 0x65, 0x56, 0x65,
	0x72, 0x73, 0x69, 0x6f, 0x6e, 0x05, 0x00, 0x63, 0x6f, 0x75, 0x6e, 0x74,
	0x07, 0x00, 0x73, 0x74, 0x61, 0x72, 0x74, 0x65, 0x64, 0x09, 0x00, 0x61,
	0x70, 0x70, 0x2e, 0x73, 0x74, 0x61, 0x72, 0x74, 0x0a, 0x00, 0x6b, 0x65,
	0x79, 0x2e, 0x53, 0x45, 0x4c, 0x45, 0x43, 0x54, 0x08, 0x00, 0x6b, 0x65,
	0x79, 0x2e, 0x42, 0x41, 0x43, 0x4b, 0x04, 0x00, 0x6e, 0x6f, 0x6f, 0x70,
	0x03, 0x00, 0x01, 0x00, 0x01, 0x00, 0x02, 0x01, 0x00, 0x00, 0x00, 0x02,
	0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x00,
	0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x04, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x00, 0x2a, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00,
	0x00, 0x2a, 0x00, 0x00, 0x00, 0x0f, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00,
	0x00, 0x39, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x07,
	0x00, 0x3c, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x32, 0x01, 0x0a,
	0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x17, 0x1f, 0x1f, 0x00, 0x00,
	0x00, 0x32, 0x0e, 0x01, 0x01, 0x00, 0x00, 0x00, 0x0b, 0x00, 0x00, 0x1e,
	0x1f, 0x00, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x0b, 0x02, 0x00,
	0x32, 0x02, 0x2a, 0x0a, 0x01, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x14,
	0x0b, 0x01, 0x00, 0x32, 0x02, 0x2a, 0x32, 0x03, 0x2a, 0x2a,
};

static const uint8_t device_binding_app_sqbc[] = {
	0x53, 0x51, 0x42, 0x43, 0x7a, 0x00, 0x30, 0x01, 0x00, 0x00, 0x09, 0x00,
	0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x7a, 0x00, 0x00, 0x00, 0x1d, 0x00,
	0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x97, 0x00, 0x00, 0x00, 0x08, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x9f, 0x00, 0x00, 0x00, 0x65, 0x00,
	0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x04, 0x01, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x06, 0x01, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x08, 0x01, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x0a, 0x01, 0x00, 0x00, 0x0e, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x18, 0x01, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x24, 0x01, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x12, 0x00, 0x64, 0x65, 0x76, 0x69, 0x63, 0x65, 0x2d, 0x62,
	0x69, 0x6e, 0x64, 0x69, 0x6e, 0x67, 0x2d, 0x61, 0x70, 0x70, 0x07, 0x00,
	0x64, 0x65, 0x66, 0x61, 0x75, 0x6c, 0x74, 0x01, 0x00, 0x01, 0x00, 0x02,
	0x00, 0x03, 0x00, 0x07, 0x00, 0x12, 0x00, 0x64, 0x65, 0x76, 0x69, 0x63,
	0x65, 0x2d, 0x62, 0x69, 0x6e, 0x64, 0x69, 0x6e, 0x67, 0x2d, 0x61, 0x70,
	0x70, 0x09, 0x00, 0x69, 0x6e, 0x64, 0x69, 0x63, 0x61, 0x74, 0x6f, 0x72,
	0x07, 0x00, 0x64, 0x65, 0x66, 0x61, 0x75, 0x6c, 0x74, 0x19, 0x00, 0x64,
	0x65, 0x76, 0x69, 0x63, 0x65, 0x2f, 0x69, 0x6e, 0x64, 0x69, 0x63, 0x61,
	0x74, 0x6f, 0x72, 0x2e, 0x73, 0x71, 0x64, 0x65, 0x76, 0x69, 0x63, 0x65,
	0x09, 0x00, 0x61, 0x70, 0x70, 0x2e, 0x73, 0x74, 0x61, 0x72, 0x74, 0x0d,
	0x00, 0x62, 0x69, 0x6e, 0x64, 0x69, 0x6e, 0x67, 0x20, 0x72, 0x65, 0x61,
	0x64, 0x79, 0x04, 0x00, 0x6d, 0x61, 0x69, 0x6e, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	0x0b, 0x00, 0x00, 0x00, 0x01, 0x00, 0x06, 0x00, 0x0b, 0x00, 0x00, 0x00,
	0x01, 0x00, 0x00, 0x00, 0x02, 0x01, 0x32, 0x1b, 0x03, 0x05, 0x00, 0x32,
	0x04, 0x01, 0x2a, 0x2a
};

static const uint8_t inline_gpio_binding_app_sqbc[] = {
	0x53, 0x51, 0x42, 0x43, 0x7a, 0x00, 0x32, 0x01, 0x00, 0x00, 0x09, 0x00,
	0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x7a, 0x00, 0x00, 0x00, 0x22, 0x00,
	0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x9c, 0x00, 0x00, 0x00, 0x08, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0xa4, 0x00, 0x00, 0x00, 0x62, 0x00,
	0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x06, 0x01, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x08, 0x01, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x0a, 0x01, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x0c, 0x01, 0x00, 0x00, 0x0e, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x1a, 0x01, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x26, 0x01, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x17, 0x00, 0x69, 0x6e, 0x6c, 0x69, 0x6e, 0x65, 0x2d, 0x67,
	0x70, 0x69, 0x6f, 0x2d, 0x62, 0x69, 0x6e, 0x64, 0x69, 0x6e, 0x67, 0x2d,
	0x61, 0x70, 0x70, 0x07, 0x00, 0x64, 0x65, 0x66, 0x61, 0x75, 0x6c, 0x74,
	0x01, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x07, 0x00, 0x17, 0x00,
	0x69, 0x6e, 0x6c, 0x69, 0x6e, 0x65, 0x2d, 0x67, 0x70, 0x69, 0x6f, 0x2d,
	0x62, 0x69, 0x6e, 0x64, 0x69, 0x6e, 0x67, 0x2d, 0x61, 0x70, 0x70, 0x09,
	0x00, 0x69, 0x6e, 0x64, 0x69, 0x63, 0x61, 0x74, 0x6f, 0x72, 0x07, 0x00,
	0x64, 0x65, 0x66, 0x61, 0x75, 0x6c, 0x74, 0x0a, 0x00, 0x67, 0x70, 0x69,
	0x6f, 0x3a, 0x47, 0x50, 0x49, 0x4f, 0x38, 0x09, 0x00, 0x61, 0x70, 0x70,
	0x2e, 0x73, 0x74, 0x61, 0x72, 0x74, 0x14, 0x00, 0x69, 0x6e, 0x6c, 0x69,
	0x6e, 0x65, 0x20, 0x62, 0x69, 0x6e, 0x64, 0x69, 0x6e, 0x67, 0x20, 0x72,
	0x65, 0x61, 0x64, 0x79, 0x04, 0x00, 0x6d, 0x61, 0x69, 0x6e, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x0b, 0x00, 0x00, 0x00, 0x01, 0x00, 0x06, 0x00, 0x0b, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x01, 0x32, 0x1b, 0x03, 0x05,
	0x00, 0x32, 0x04, 0x01, 0x2a, 0x2a
};

static const uint8_t display_binding_app_sqbc[] = {
	0x53, 0x51, 0x42, 0x43, 0x7a, 0x00, 0x42, 0x01, 0x00, 0x00, 0x09, 0x00,
	0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x7a, 0x00, 0x00, 0x00, 0x1e, 0x00,
	0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x98, 0x00, 0x00, 0x00, 0x08, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0xa0, 0x00, 0x00, 0x00, 0x70, 0x00,
	0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x10, 0x01, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x12, 0x01, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x14, 0x01, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x16, 0x01, 0x00, 0x00, 0x0e, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x24, 0x01, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x30, 0x01, 0x00, 0x00, 0x12, 0x00,
	0x00, 0x00, 0x13, 0x00, 0x64, 0x69, 0x73, 0x70, 0x6c, 0x61, 0x79, 0x2d,
	0x62, 0x69, 0x6e, 0x64, 0x69, 0x6e, 0x67, 0x2d, 0x61, 0x70, 0x70, 0x07,
	0x00, 0x64, 0x65, 0x66, 0x61, 0x75, 0x6c, 0x74, 0x01, 0x00, 0x01, 0x00,
	0x02, 0x00, 0x03, 0x00, 0x07, 0x00, 0x13, 0x00, 0x64, 0x69, 0x73, 0x70,
	0x6c, 0x61, 0x79, 0x2d, 0x62, 0x69, 0x6e, 0x64, 0x69, 0x6e, 0x67, 0x2d,
	0x61, 0x70, 0x70, 0x07, 0x00, 0x64, 0x69, 0x73, 0x70, 0x6c, 0x61, 0x79,
	0x06, 0x00, 0x73, 0x74, 0x61, 0x74, 0x75, 0x73, 0x1e, 0x00, 0x64, 0x65,
	0x76, 0x69, 0x63, 0x65, 0x2f, 0x73, 0x74, 0x61, 0x74, 0x75, 0x73, 0x2d,
	0x64, 0x69, 0x73, 0x70, 0x6c, 0x61, 0x79, 0x2e, 0x73, 0x71, 0x64, 0x65,
	0x76, 0x69, 0x63, 0x65, 0x09, 0x00, 0x61, 0x70, 0x70, 0x2e, 0x73, 0x74,
	0x61, 0x72, 0x74, 0x04, 0x00, 0x6d, 0x61, 0x69, 0x6e, 0x15, 0x00, 0x64,
	0x69, 0x73, 0x70, 0x6c, 0x61, 0x79, 0x20, 0x62, 0x69, 0x6e, 0x64, 0x69,
	0x6e, 0x67, 0x20, 0x72, 0x65, 0x61, 0x64, 0x79, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	0x0c, 0x00, 0x00, 0x00, 0x01, 0x00, 0x05, 0x00, 0x0c, 0x00, 0x00, 0x00,
	0x06, 0x00, 0x00, 0x00, 0x03, 0x05, 0x00, 0x32, 0x05, 0x03, 0x06, 0x00,
	0x32, 0x04, 0x01, 0x2a, 0x03, 0x02, 0x00, 0x32, 0x16, 0x2a
};

static const uint8_t foreground_memory_sqbc[] = {
	0x53, 0x51, 0x42, 0x43, 0x7a, 0x00, 0x50, 0x01, 0x00, 0x00, 0x09, 0x00,
	0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x7a, 0x00, 0x00, 0x00, 0x1c, 0x00,
	0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x96, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x98, 0x00, 0x00, 0x00, 0x56, 0x00,
	0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0xee, 0x00, 0x00, 0x00, 0x0b, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0xf9, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0xfb, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xfd, 0x00, 0x00, 0x00, 0x1a, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x17, 0x01, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x23, 0x01, 0x00, 0x00, 0x2d, 0x00,
	0x00, 0x00, 0x11, 0x00, 0x66, 0x6f, 0x72, 0x65, 0x67, 0x72, 0x6f, 0x75,
	0x6e, 0x64, 0x2d, 0x6d, 0x65, 0x6d, 0x6f, 0x72, 0x79, 0x07, 0x00, 0x64,
	0x65, 0x66, 0x61, 0x75, 0x6c, 0x74, 0x00, 0x00, 0x07, 0x00, 0x11, 0x00,
	0x66, 0x6f, 0x72, 0x65, 0x67, 0x72, 0x6f, 0x75, 0x6e, 0x64, 0x2d, 0x6d,
	0x65, 0x6d, 0x6f, 0x72, 0x79, 0x05, 0x00, 0x63, 0x6f, 0x75, 0x6e, 0x74,
	0x09, 0x00, 0x61, 0x70, 0x70, 0x2e, 0x73, 0x74, 0x61, 0x72, 0x74, 0x0c,
	0x00, 0x6d, 0x65, 0x6d, 0x6f, 0x72, 0x79, 0x20, 0x73, 0x74, 0x61, 0x72,
	0x74, 0x0a, 0x00, 0x6b, 0x65, 0x79, 0x2e, 0x53, 0x45, 0x4c, 0x45, 0x43,
	0x54, 0x0d, 0x00, 0x6d, 0x65, 0x6d, 0x6f, 0x72, 0x79, 0x20, 0x73, 0x65,
	0x6c, 0x65, 0x63, 0x74, 0x04, 0x00, 0x6d, 0x61, 0x69, 0x6e, 0x01, 0x00,
	0x01, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x02, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x16,
	0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x16, 0x00, 0x00, 0x00, 0x16,
	0x00, 0x00, 0x00, 0x01, 0x00, 0x06, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x01,
	0x00, 0x00, 0x00, 0x0a, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x14,
	0x0b, 0x00, 0x00, 0x03, 0x03, 0x00, 0x0a, 0x00, 0x00, 0x32, 0x04, 0x02,
	0x2a, 0x0a, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x14, 0x0b, 0x00,
	0x00, 0x03, 0x05, 0x00, 0x0a, 0x00, 0x00, 0x32, 0x04, 0x02, 0x2a, 0x2a,
};

static const uint8_t content_pick_file_sqbc[] = {
	0x53, 0x51, 0x42, 0x43, 0x7a, 0x00, 0x2c, 0x01, 0x00, 0x00, 0x09, 0x00,
	0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x7a, 0x00, 0x00, 0x00, 0x19, 0x00,
	0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x93, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x95, 0x00, 0x00, 0x00, 0x58, 0x00,
	0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0xed, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0xef, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0xf1, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xf3, 0x00, 0x00, 0x00, 0x0e, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x0d, 0x01, 0x00, 0x00, 0x1f, 0x00,
	0x00, 0x00, 0x0e, 0x00, 0x63, 0x6f, 0x6e, 0x74, 0x65, 0x6e, 0x74, 0x2d,
	0x70, 0x69, 0x63, 0x6b, 0x65, 0x72, 0x07, 0x00, 0x64, 0x65, 0x66, 0x61,
	0x75, 0x6c, 0x74, 0x00, 0x00, 0x09, 0x00, 0x0e, 0x00, 0x63, 0x6f, 0x6e,
	0x74, 0x65, 0x6e, 0x74, 0x2d, 0x70, 0x69, 0x63, 0x6b, 0x65, 0x72, 0x09,
	0x00, 0x61, 0x70, 0x70, 0x2e, 0x73, 0x74, 0x61, 0x72, 0x74, 0x06, 0x00,
	0x70, 0x69, 0x63, 0x6b, 0x65, 0x64, 0x10, 0x00, 0x63, 0x6f, 0x6e, 0x74,
	0x65, 0x6e, 0x74, 0x2e, 0x70, 0x69, 0x63, 0x6b, 0x46, 0x69, 0x6c, 0x65,
	0x08, 0x00, 0x2e, 0x62, 0x69, 0x6e, 0x62, 0x6f, 0x6f, 0x6b, 0x02, 0x00,
	0x6f, 0x6b, 0x05, 0x00, 0x65, 0x72, 0x72, 0x6f, 0x72, 0x04, 0x00, 0x70,
	0x61, 0x74, 0x68, 0x04, 0x00, 0x6d, 0x61, 0x69, 0x6e, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x1e, 0x00, 0x00, 0x00, 0x01, 0x00, 0x08, 0x00, 0x1e, 0x00, 0x00,
	0x00, 0x01, 0x00, 0x00, 0x00, 0x03, 0x04, 0x00, 0x32, 0x2e, 0x0d, 0x00,
	0x00, 0x0c, 0x00, 0x00, 0x0e, 0x05, 0x00, 0x0c, 0x00, 0x00, 0x0e, 0x06,
	0x00, 0x0c, 0x00, 0x00, 0x0e, 0x07, 0x00, 0x32, 0x04, 0x03, 0x2a, 0x2a,
};

static const uint8_t content_read_sqbc[] = {
	0x53, 0x51, 0x42, 0x43, 0x7a, 0x00, 0x5d, 0x01, 0x00, 0x00, 0x09, 0x00,
	0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x7a, 0x00, 0x00, 0x00, 0x17, 0x00,
	0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x91, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x93, 0x00, 0x00, 0x00, 0x69, 0x00,
	0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0xfc, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0xfe, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x02, 0x01, 0x00, 0x00, 0x0e, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x10, 0x01, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x1c, 0x01, 0x00, 0x00, 0x41, 0x00,
	0x00, 0x00, 0x0c, 0x00, 0x63, 0x6f, 0x6e, 0x74, 0x65, 0x6e, 0x74, 0x2d,
	0x72, 0x65, 0x61, 0x64, 0x07, 0x00, 0x64, 0x65, 0x66, 0x61, 0x75, 0x6c,
	0x74, 0x00, 0x00, 0x0a, 0x00, 0x0c, 0x00, 0x63, 0x6f, 0x6e, 0x74, 0x65,
	0x6e, 0x74, 0x2d, 0x72, 0x65, 0x61, 0x64, 0x09, 0x00, 0x61, 0x70, 0x70,
	0x2e, 0x73, 0x74, 0x61, 0x72, 0x74, 0x04, 0x00, 0x74, 0x65, 0x78, 0x74,
	0x10, 0x00, 0x63, 0x6f, 0x6e, 0x74, 0x65, 0x6e, 0x74, 0x2e, 0x72, 0x65,
	0x61, 0x64, 0x54, 0x65, 0x78, 0x74, 0x09, 0x00, 0x6e, 0x6f, 0x74, 0x65,
	0x73, 0x2e, 0x74, 0x78, 0x74, 0x05, 0x00, 0x6c, 0x69, 0x6e, 0x65, 0x73,
	0x11, 0x00, 0x63, 0x6f, 0x6e, 0x74, 0x65, 0x6e, 0x74, 0x2e, 0x72, 0x65,
	0x61, 0x64, 0x4c, 0x69, 0x6e, 0x65, 0x73, 0x02, 0x00, 0x6f, 0x6b, 0x05,
	0x00, 0x65, 0x72, 0x72, 0x6f, 0x72, 0x04, 0x00, 0x6d, 0x61, 0x69, 0x6e,
	0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x01, 0x00, 0x09, 0x00,
	0x40, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x03, 0x04, 0x00, 0x32,
	0x2f, 0x0d, 0x00, 0x00, 0x03, 0x04, 0x00, 0x01, 0x04, 0x00, 0x00, 0x00,
	0x32, 0x30, 0x0d, 0x01, 0x00, 0x0c, 0x00, 0x00, 0x0e, 0x07, 0x00, 0x0c,
	0x00, 0x00, 0x0e, 0x08, 0x00, 0x0c, 0x00, 0x00, 0x0e, 0x02, 0x00, 0x32,
	0x04, 0x03, 0x0c, 0x01, 0x00, 0x0e, 0x07, 0x00, 0x0c, 0x01, 0x00, 0x0e,
	0x08, 0x00, 0x0c, 0x01, 0x00, 0x0e, 0x05, 0x00, 0x32, 0x04, 0x03, 0x2a,
	0x2a,
};

static const uint8_t wifi_actions_sqbc[] = {
	0x53, 0x51, 0x42, 0x43, 0x6e, 0x00, 0xda, 0x01, 0x00, 0x00, 0x08, 0x00,
	0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x6e, 0x00, 0x00, 0x00, 0x17, 0x00,
	0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x85, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x87, 0x00, 0x00, 0x00, 0xd5, 0x00,
	0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x5c, 0x01, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x5e, 0x01, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x60, 0x01, 0x00, 0x00, 0x0e, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x6e, 0x01, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x7a, 0x01, 0x00, 0x00, 0x60, 0x00,
	0x00, 0x00, 0x0c, 0x00, 0x77, 0x69, 0x66, 0x69, 0x2d, 0x61, 0x63, 0x74,
	0x69, 0x6f, 0x6e, 0x73, 0x07, 0x00, 0x64, 0x65, 0x66, 0x61, 0x75, 0x6c,
	0x74, 0x00, 0x00, 0x11, 0x00, 0x0c, 0x00, 0x77, 0x69, 0x66, 0x69, 0x2d,
	0x61, 0x63, 0x74, 0x69, 0x6f, 0x6e, 0x73, 0x09, 0x00, 0x61, 0x70, 0x70,
	0x2e, 0x73, 0x74, 0x61, 0x72, 0x74, 0x02, 0x00, 0x61, 0x70, 0x14, 0x00,
	0x73, 0x65, 0x72, 0x76, 0x69, 0x63, 0x65, 0x2e, 0x77, 0x69, 0x66, 0x69,
	0x2e, 0x73, 0x74, 0x61, 0x72, 0x74, 0x41, 0x50, 0x0b, 0x00, 0x53, 0x71,
	0x75, 0x69, 0x64, 0x53, 0x63, 0x72, 0x69, 0x70, 0x74, 0x02, 0x00, 0x69,
	0x70, 0x14, 0x00, 0x73, 0x65, 0x72, 0x76, 0x69, 0x63, 0x65, 0x2e, 0x77,
	0x69, 0x66, 0x69, 0x2e, 0x67, 0x65, 0x74, 0x41, 0x50, 0x49, 0x50, 0x04,
	0x00, 0x73, 0x74, 0x6f, 0x70, 0x13, 0x00, 0x73, 0x65, 0x72, 0x76, 0x69,
	0x63, 0x65, 0x2e, 0x77, 0x69, 0x66, 0x69, 0x2e, 0x73, 0x74, 0x6f, 0x70,
	0x41, 0x50, 0x09, 0x00, 0x63, 0x6f, 0x6e, 0x6e, 0x65, 0x63, 0x74, 0x65,
	0x64, 0x14, 0x00, 0x73, 0x65, 0x72, 0x76, 0x69, 0x63, 0x65, 0x2e, 0x77,
	0x69, 0x66, 0x69, 0x2e, 0x63, 0x6f, 0x6e, 0x6e, 0x65, 0x63, 0x74, 0x03,
	0x00, 0x64, 0x65, 0x76, 0x0c, 0x00, 0x64, 0x69, 0x73, 0x63, 0x6f, 0x6e,
	0x6e, 0x65, 0x63, 0x74, 0x65, 0x64, 0x17, 0x00, 0x73, 0x65, 0x72, 0x76,
	0x69, 0x63, 0x65, 0x2e, 0x77, 0x69, 0x66, 0x69, 0x2e, 0x64, 0x69, 0x73,
	0x63, 0x6f, 0x6e, 0x6e, 0x65, 0x63, 0x74, 0x02, 0x00, 0x6f, 0x6b, 0x05,
	0x00, 0x65, 0x72, 0x72, 0x6f, 0x72, 0x04, 0x00, 0x6d, 0x61, 0x69, 0x6e,
	0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x5f, 0x00, 0x00, 0x00, 0x01, 0x00, 0x10, 0x00, 0x5f, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x03, 0x04, 0x00, 0x32, 0x1e, 0x0d,
	0x00, 0x00, 0x32, 0x21, 0x0d, 0x01, 0x00, 0x32, 0x1f, 0x0d, 0x02, 0x00,
	0x03, 0x0b, 0x00, 0x32, 0x23, 0x0d, 0x03, 0x00, 0x32, 0x24, 0x0d, 0x04,
	0x00, 0x0c, 0x00, 0x00, 0x0e, 0x0e, 0x00, 0x0c, 0x00, 0x00, 0x0e, 0x0f,
	0x00, 0x32, 0x04, 0x02, 0x0c, 0x01, 0x00, 0x0e, 0x0f, 0x00, 0x32, 0x04,
	0x01, 0x0c, 0x02, 0x00, 0x0e, 0x0e, 0x00, 0x0c, 0x02, 0x00, 0x0e, 0x0f,
	0x00, 0x0c, 0x03, 0x00, 0x0e, 0x0e, 0x00, 0x0c, 0x03, 0x00, 0x0e, 0x0f,
	0x00, 0x0c, 0x04, 0x00, 0x0e, 0x0e, 0x00, 0x0c, 0x04, 0x00, 0x0e, 0x0f,
	0x00, 0x32, 0x04, 0x06, 0x2a, 0x2a
};

static const uint8_t lifecycle_sqbc[] = {
	0x53, 0x51, 0x42, 0x43, 0x6e, 0x00, 0x37, 0x01, 0x00, 0x00, 0x08, 0x00,
	0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x6e, 0x00, 0x00, 0x00, 0x14, 0x00,
	0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x82, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x84, 0x00, 0x00, 0x00, 0x5e, 0x00,
	0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0xe2, 0x00, 0x00, 0x00, 0x0b, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0xed, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xef, 0x00, 0x00, 0x00, 0x1a, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x09, 0x01, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x15, 0x01, 0x00, 0x00, 0x22, 0x00,
	0x00, 0x00, 0x09, 0x00, 0x6c, 0x69, 0x66, 0x65, 0x63, 0x79, 0x63, 0x6c,
	0x65, 0x07, 0x00, 0x64, 0x65, 0x66, 0x61, 0x75, 0x6c, 0x74, 0x00, 0x00,
	0x09, 0x00, 0x09, 0x00, 0x6c, 0x69, 0x66, 0x65, 0x63, 0x79, 0x63, 0x6c,
	0x65, 0x05, 0x00, 0x63, 0x6f, 0x75, 0x6e, 0x74, 0x09, 0x00, 0x61, 0x70,
	0x70, 0x2e, 0x73, 0x74, 0x61, 0x72, 0x74, 0x04, 0x00, 0x72, 0x65, 0x70,
	0x6c, 0x0e, 0x00, 0x62, 0x72, 0x65, 0x61, 0x6b, 0x2d, 0x72, 0x65, 0x6d,
	0x69, 0x6e, 0x64, 0x65, 0x72, 0x06, 0x00, 0x72, 0x65, 0x61, 0x64, 0x65,
	0x72, 0x0b, 0x00, 0x74, 0x69, 0x6d, 0x65, 0x72, 0x2e, 0x62, 0x72, 0x65,
	0x61, 0x6b, 0x0c, 0x00, 0x6c, 0x69, 0x66, 0x65, 0x63, 0x79, 0x63, 0x6c,
	0x65, 0x20, 0x6f, 0x6b, 0x04, 0x00, 0x6d, 0x61, 0x69, 0x6e, 0x01, 0x00,
	0x01, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
	0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
	0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00,
	0x00, 0x01, 0x00, 0x08, 0x00, 0x21, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
	0x00, 0x2a, 0x03, 0x04, 0x00, 0x32, 0x10, 0x03, 0x05, 0x00, 0x32, 0x0d,
	0x03, 0x04, 0x00, 0x32, 0x11, 0x03, 0x06, 0x00, 0x01, 0xfa, 0x00, 0x00,
	0x00, 0x32, 0x13, 0x03, 0x07, 0x00, 0x32, 0x04, 0x01, 0x2a, 0x2a,
};

static const uint8_t reader_exit_sqbc[] = {
	0x53, 0x51, 0x42, 0x43, 0x6e, 0x00, 0xe5, 0x00, 0x00, 0x00, 0x08, 0x00,
	0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x6e, 0x00, 0x00, 0x00, 0x11, 0x00,
	0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x7f, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x81, 0x00, 0x00, 0x00, 0x2f, 0x00,
	0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0xb0, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0xb2, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xb4, 0x00, 0x00, 0x00, 0x1a, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0xce, 0x00, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0xda, 0x00, 0x00, 0x00, 0x0b, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x72, 0x65, 0x61, 0x64, 0x65, 0x72, 0x07, 0x00,
	0x64, 0x65, 0x66, 0x61, 0x75, 0x6c, 0x74, 0x00, 0x00, 0x05, 0x00, 0x06,
	0x00, 0x72, 0x65, 0x61, 0x64, 0x65, 0x72, 0x09, 0x00, 0x61, 0x70, 0x70,
	0x2e, 0x73, 0x74, 0x61, 0x72, 0x74, 0x0c, 0x00, 0x72, 0x65, 0x61, 0x64,
	0x65, 0x72, 0x20, 0x73, 0x74, 0x61, 0x72, 0x74, 0x04, 0x00, 0x72, 0x65,
	0x70, 0x6c, 0x04, 0x00, 0x6d, 0x61, 0x69, 0x6e, 0x00, 0x00, 0x00, 0x00,
	0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x03, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x04, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x01, 0x00,
	0x00, 0x00, 0x03, 0x02, 0x00, 0x32, 0x04, 0x01, 0x2a, 0x32, 0x03, 0x2a,
	0x2a,
};

static const uint8_t armer_sqbc[] = {
	0x53, 0x51, 0x42, 0x43, 0x6e, 0x00, 0xdc, 0x00, 0x00, 0x00, 0x08, 0x00,
	0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x6e, 0x00, 0x00, 0x00, 0x10, 0x00,
	0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x7e, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x31, 0x00,
	0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0xb1, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0xb3, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xb5, 0x00, 0x00, 0x00, 0x0e, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0xc3, 0x00, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0xcf, 0x00, 0x00, 0x00, 0x0d, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x61, 0x72, 0x6d, 0x65, 0x72, 0x07, 0x00, 0x64,
	0x65, 0x66, 0x61, 0x75, 0x6c, 0x74, 0x00, 0x00, 0x05, 0x00, 0x05, 0x00,
	0x61, 0x72, 0x6d, 0x65, 0x72, 0x09, 0x00, 0x61, 0x70, 0x70, 0x2e, 0x73,
	0x74, 0x61, 0x72, 0x74, 0x0e, 0x00, 0x62, 0x72, 0x65, 0x61, 0x6b, 0x2d,
	0x72, 0x65, 0x6d, 0x69, 0x6e, 0x64, 0x65, 0x72, 0x05, 0x00, 0x61, 0x72,
	0x6d, 0x65, 0x64, 0x04, 0x00, 0x6d, 0x61, 0x69, 0x6e, 0x00, 0x00, 0x00,
	0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0c,
	0x00, 0x00, 0x00, 0x01, 0x00, 0x04, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x01,
	0x00, 0x00, 0x00, 0x03, 0x02, 0x00, 0x32, 0x10, 0x03, 0x03, 0x00, 0x32,
	0x04, 0x01, 0x2a, 0x2a,
};

static const uint8_t break_reminder_sqbc[] = {
	0x53, 0x51, 0x42, 0x43, 0x7a, 0x00, 0x16, 0x01, 0x00, 0x00, 0x09, 0x00,
	0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x7a, 0x00, 0x00, 0x00, 0x19, 0x00,
	0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x93, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x95, 0x00, 0x00, 0x00, 0x39, 0x00,
	0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0xce, 0x00, 0x00, 0x00, 0x0b, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0xd9, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0xdb, 0x00, 0x00, 0x00, 0x0a, 0x00,
	0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xe5, 0x00, 0x00, 0x00, 0x0e, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0xf3, 0x00, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0xff, 0x00, 0x00, 0x00, 0x17, 0x00,
	0x00, 0x00, 0x0e, 0x00, 0x62, 0x72, 0x65, 0x61, 0x6b, 0x2d, 0x72, 0x65,
	0x6d, 0x69, 0x6e, 0x64, 0x65, 0x72, 0x07, 0x00, 0x64, 0x65, 0x66, 0x61,
	0x75, 0x6c, 0x74, 0x00, 0x00, 0x05, 0x00, 0x0e, 0x00, 0x62, 0x72, 0x65,
	0x61, 0x6b, 0x2d, 0x72, 0x65, 0x6d, 0x69, 0x6e, 0x64, 0x65, 0x72, 0x0b,
	0x00, 0x74, 0x69, 0x6d, 0x65, 0x72, 0x2e, 0x62, 0x72, 0x65, 0x61, 0x6b,
	0x05, 0x00, 0x66, 0x69, 0x72, 0x65, 0x73, 0x0b, 0x00, 0x62, 0x72, 0x65,
	0x61, 0x6b, 0x20, 0x66, 0x69, 0x72, 0x65, 0x64, 0x04, 0x00, 0x6d, 0x61,
	0x69, 0x6e, 0x01, 0x00, 0x02, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
	0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x16,
	0x00, 0x00, 0x00, 0x01, 0x00, 0x04, 0x00, 0x16, 0x00, 0x00, 0x00, 0x01,
	0x00, 0x00, 0x00, 0x0a, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x14,
	0x0b, 0x00, 0x00, 0x03, 0x03, 0x00, 0x0a, 0x00, 0x00, 0x32, 0x04, 0x02,
	0x2a, 0x2a,
};

static const uint8_t system_resources_sqbc[] = {
	0x53, 0x51, 0x42, 0x43, 0x7a, 0x00, 0x13, 0x01, 0x00, 0x00, 0x09, 0x00,
	0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x7a, 0x00, 0x00, 0x00, 0x1b, 0x00,
	0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x95, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x97, 0x00, 0x00, 0x00, 0x47, 0x00,
	0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0xde, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0xe0, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0xe2, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xe4, 0x00, 0x00, 0x00, 0x0e, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0xf2, 0x00, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0xfe, 0x00, 0x00, 0x00, 0x15, 0x00,
	0x00, 0x00, 0x10, 0x00, 0x73, 0x79, 0x73, 0x74, 0x65, 0x6d, 0x2d, 0x72,
	0x65, 0x73, 0x6f, 0x75, 0x72, 0x63, 0x65, 0x73, 0x07, 0x00, 0x64, 0x65,
	0x66, 0x61, 0x75, 0x6c, 0x74, 0x00, 0x00, 0x06, 0x00, 0x10, 0x00, 0x73,
	0x79, 0x73, 0x74, 0x65, 0x6d, 0x2d, 0x72, 0x65, 0x73, 0x6f, 0x75, 0x72,
	0x63, 0x65, 0x73, 0x09, 0x00, 0x61, 0x70, 0x70, 0x2e, 0x73, 0x74, 0x61,
	0x72, 0x74, 0x0d, 0x00, 0x73, 0x79, 0x73, 0x74, 0x65, 0x6d, 0x20, 0x6d,
	0x65, 0x6d, 0x6f, 0x72, 0x79, 0x0b, 0x00, 0x73, 0x79, 0x73, 0x74, 0x65,
	0x6d, 0x20, 0x61, 0x70, 0x70, 0x73, 0x04, 0x00, 0x61, 0x70, 0x70, 0x73,
	0x04, 0x00, 0x6d, 0x61, 0x69, 0x6e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x05, 0x00, 0x14, 0x00, 0x00, 0x00, 0x01, 0x00,
	0x00, 0x00, 0x03, 0x02, 0x00, 0x32, 0x14, 0x32, 0x04, 0x02, 0x03, 0x03,
	0x00, 0x03, 0x04, 0x00, 0x32, 0x15, 0x32, 0x04, 0x02, 0x2a, 0x2a,
};

static const uint8_t app_registry_summary_sqbc[] = {
	0x53, 0x51, 0x42, 0x43, 0x7a, 0x00, 0xdf, 0x01, 0x00, 0x00, 0x09, 0x00,
	0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x7a, 0x00, 0x00, 0x00, 0x1f, 0x00,
	0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x99, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x9b, 0x00, 0x00, 0x00, 0x9f, 0x00,
	0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x3a, 0x01, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x3c, 0x01, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x3e, 0x01, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x40, 0x01, 0x00, 0x00, 0x0e, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x4e, 0x01, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x5a, 0x01, 0x00, 0x00, 0x85, 0x00,
	0x00, 0x00, 0x14, 0x00, 0x61, 0x70, 0x70, 0x2d, 0x72, 0x65, 0x67, 0x69,
	0x73, 0x74, 0x72, 0x79, 0x2d, 0x73, 0x75, 0x6d, 0x6d, 0x61, 0x72, 0x79,
	0x07, 0x00, 0x64, 0x65, 0x66, 0x61, 0x75, 0x6c, 0x74, 0x00, 0x00, 0x0e,
	0x00, 0x14, 0x00, 0x61, 0x70, 0x70, 0x2d, 0x72, 0x65, 0x67, 0x69, 0x73,
	0x74, 0x72, 0x79, 0x2d, 0x73, 0x75, 0x6d, 0x6d, 0x61, 0x72, 0x79, 0x09,
	0x00, 0x61, 0x70, 0x70, 0x2e, 0x73, 0x74, 0x61, 0x72, 0x74, 0x04, 0x00,
	0x61, 0x70, 0x70, 0x73, 0x0c, 0x00, 0x61, 0x70, 0x70, 0x2e, 0x72, 0x65,
	0x67, 0x69, 0x73, 0x74, 0x72, 0x79, 0x0c, 0x00, 0x72, 0x65, 0x67, 0x69,
	0x73, 0x74, 0x72, 0x79, 0x20, 0x61, 0x70, 0x70, 0x05, 0x00, 0x61, 0x70,
	0x70, 0x49, 0x64, 0x08, 0x00, 0x73, 0x65, 0x6c, 0x65, 0x63, 0x74, 0x65,
	0x64, 0x10, 0x00, 0x61, 0x70, 0x70, 0x2e, 0x72, 0x65, 0x67, 0x69, 0x73,
	0x74, 0x72, 0x79, 0x2e, 0x67, 0x65, 0x74, 0x11, 0x00, 0x72, 0x65, 0x67,
	0x69, 0x73, 0x74, 0x72, 0x79, 0x20, 0x73, 0x65, 0x6c, 0x65, 0x63, 0x74,
	0x65, 0x64, 0x02, 0x00, 0x69, 0x64, 0x04, 0x00, 0x6e, 0x61, 0x6d, 0x65,
	0x05, 0x00, 0x62, 0x75, 0x69, 0x6c, 0x64, 0x0b, 0x00, 0x64, 0x65, 0x73,
	0x63, 0x72, 0x69, 0x70, 0x74, 0x69, 0x6f, 0x6e, 0x04, 0x00, 0x6d, 0x61,
	0x69, 0x6e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x84, 0x00, 0x00, 0x00, 0x01, 0x00,
	0x0d, 0x00, 0x84, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x32, 0x26,
	0x0d, 0x00, 0x00, 0x0c, 0x00, 0x00, 0x0d, 0x01, 0x00, 0x01, 0x00, 0x00,
	0x00, 0x00, 0x0d, 0x02, 0x00, 0x01, 0x04, 0x00, 0x00, 0x00, 0x0d, 0x03,
	0x00, 0x0c, 0x02, 0x00, 0x0c, 0x01, 0x00, 0x3d, 0x18, 0x1f, 0x58, 0x00,
	0x00, 0x00, 0x0c, 0x02, 0x00, 0x0c, 0x03, 0x00, 0x18, 0x1f, 0x58, 0x00,
	0x00, 0x00, 0x0c, 0x01, 0x00, 0x0c, 0x02, 0x00, 0x3e, 0x0d, 0x04, 0x00,
	0x03, 0x04, 0x00, 0x0c, 0x04, 0x00, 0x32, 0x04, 0x02, 0x0c, 0x02, 0x00,
	0x01, 0x01, 0x00, 0x00, 0x00, 0x14, 0x0d, 0x02, 0x00, 0x1e, 0x1b, 0x00,
	0x00, 0x00, 0x0c, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x32, 0x27,
	0x0d, 0x05, 0x00, 0x03, 0x08, 0x00, 0x0c, 0x05, 0x00, 0x0e, 0x09, 0x00,
	0x0c, 0x05, 0x00, 0x0e, 0x0a, 0x00, 0x0c, 0x05, 0x00, 0x0e, 0x0b, 0x00,
	0x0c, 0x05, 0x00, 0x0e, 0x0c, 0x00, 0x32, 0x04, 0x05, 0x2a, 0x2a,
};

static const uint8_t stack_inspect_sqbc[] = {
	0x53, 0x51, 0x42, 0x43, 0x7a, 0x00, 0x18, 0x02, 0x00, 0x00, 0x09, 0x00,
	0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x7a, 0x00, 0x00, 0x00, 0x18, 0x00,
	0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x92, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x94, 0x00, 0x00, 0x00, 0x8a, 0x00,
	0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x1e, 0x01, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x20, 0x01, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x22, 0x01, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x24, 0x01, 0x00, 0x00, 0x0e, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x32, 0x01, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x3e, 0x01, 0x00, 0x00, 0xda, 0x00,
	0x00, 0x00, 0x0d, 0x00, 0x73, 0x74, 0x61, 0x63, 0x6b, 0x2d, 0x69, 0x6e,
	0x73, 0x70, 0x65, 0x63, 0x74, 0x07, 0x00, 0x64, 0x65, 0x66, 0x61, 0x75,
	0x6c, 0x74, 0x00, 0x00, 0x0c, 0x00, 0x0d, 0x00, 0x73, 0x74, 0x61, 0x63,
	0x6b, 0x2d, 0x69, 0x6e, 0x73, 0x70, 0x65, 0x63, 0x74, 0x09, 0x00, 0x61,
	0x70, 0x70, 0x2e, 0x73, 0x74, 0x61, 0x72, 0x74, 0x07, 0x00, 0x70, 0x72,
	0x6f, 0x63, 0x65, 0x73, 0x73, 0x10, 0x00, 0x61, 0x70, 0x70, 0x2e, 0x70,
	0x72, 0x6f, 0x63, 0x65, 0x73, 0x73, 0x53, 0x74, 0x61, 0x63, 0x6b, 0x05,
	0x00, 0x61, 0x70, 0x70, 0x49, 0x64, 0x05, 0x00, 0x61, 0x72, 0x6d, 0x65,
	0x64, 0x0e, 0x00, 0x61, 0x70, 0x70, 0x2e, 0x61, 0x72, 0x6d, 0x65, 0x64,
	0x53, 0x74, 0x61, 0x63, 0x6b, 0x08, 0x00, 0x61, 0x72, 0x6d, 0x65, 0x64,
	0x41, 0x70, 0x70, 0x05, 0x00, 0x65, 0x76, 0x65, 0x6e, 0x74, 0x08, 0x00,
	0x73, 0x65, 0x6c, 0x65, 0x63, 0x74, 0x65, 0x64, 0x12, 0x00, 0x61, 0x70,
	0x70, 0x2e, 0x61, 0x72, 0x6d, 0x65, 0x64, 0x53, 0x74, 0x61, 0x63, 0x6b,
	0x2e, 0x67, 0x65, 0x74, 0x04, 0x00, 0x6d, 0x61, 0x69, 0x6e, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x00, 0xd9, 0x00, 0x00, 0x00, 0x01, 0x00, 0x0b, 0x00, 0xd9, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x32, 0x28, 0x0d, 0x00, 0x00, 0x0c,
	0x00, 0x00, 0x0d, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x0d, 0x02,
	0x00, 0x01, 0x02, 0x00, 0x00, 0x00, 0x0d, 0x03, 0x00, 0x0c, 0x02, 0x00,
	0x0c, 0x01, 0x00, 0x3d, 0x18, 0x1f, 0x58, 0x00, 0x00, 0x00, 0x0c, 0x02,
	0x00, 0x0c, 0x03, 0x00, 0x18, 0x1f, 0x58, 0x00, 0x00, 0x00, 0x0c, 0x01,
	0x00, 0x0c, 0x02, 0x00, 0x3e, 0x0d, 0x04, 0x00, 0x03, 0x02, 0x00, 0x0c,
	0x04, 0x00, 0x32, 0x04, 0x02, 0x0c, 0x02, 0x00, 0x01, 0x01, 0x00, 0x00,
	0x00, 0x14, 0x0d, 0x02, 0x00, 0x1e, 0x1b, 0x00, 0x00, 0x00, 0x32, 0x29,
	0x0d, 0x05, 0x00, 0x0c, 0x05, 0x00, 0x0d, 0x01, 0x00, 0x01, 0x00, 0x00,
	0x00, 0x00, 0x0d, 0x02, 0x00, 0x01, 0x02, 0x00, 0x00, 0x00, 0x0d, 0x03,
	0x00, 0x0c, 0x02, 0x00, 0x0c, 0x01, 0x00, 0x3d, 0x18, 0x1f, 0xb9, 0x00,
	0x00, 0x00, 0x0c, 0x02, 0x00, 0x0c, 0x03, 0x00, 0x18, 0x1f, 0xb9, 0x00,
	0x00, 0x00, 0x0c, 0x01, 0x00, 0x0c, 0x02, 0x00, 0x3e, 0x0d, 0x06, 0x00,
	0x03, 0x05, 0x00, 0x0c, 0x06, 0x00, 0x0e, 0x04, 0x00, 0x0c, 0x06, 0x00,
	0x0e, 0x08, 0x00, 0x32, 0x04, 0x03, 0x0c, 0x02, 0x00, 0x01, 0x01, 0x00,
	0x00, 0x00, 0x14, 0x0d, 0x02, 0x00, 0x1e, 0x73, 0x00, 0x00, 0x00, 0x0c,
	0x05, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x32, 0x2a, 0x0d, 0x07, 0x00,
	0x03, 0x09, 0x00, 0x0c, 0x07, 0x00, 0x0e, 0x04, 0x00, 0x0c, 0x07, 0x00,
	0x0e, 0x08, 0x00, 0x32, 0x04, 0x03, 0x2a, 0x2a,
};

static const uint8_t display_drawlog_sqbc[] = {
	0x53, 0x51, 0x42, 0x43, 0x7a, 0x00, 0x53, 0x01, 0x00, 0x00, 0x09, 0x00,
	0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x7a, 0x00, 0x00, 0x00, 0x1a, 0x00,
	0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x94, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x96, 0x00, 0x00, 0x00, 0x5a, 0x00,
	0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0xf0, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0xf2, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0xf4, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xf6, 0x00, 0x00, 0x00, 0x0e, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x04, 0x01, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x10, 0x01, 0x00, 0x00, 0x43, 0x00,
	0x00, 0x00, 0x0f, 0x00, 0x64, 0x69, 0x73, 0x70, 0x6c, 0x61, 0x79, 0x2d,
	0x64, 0x72, 0x61, 0x77, 0x6c, 0x6f, 0x67, 0x07, 0x00, 0x64, 0x65, 0x66,
	0x61, 0x75, 0x6c, 0x74, 0x00, 0x00, 0x08, 0x00, 0x0f, 0x00, 0x64, 0x69,
	0x73, 0x70, 0x6c, 0x61, 0x79, 0x2d, 0x64, 0x72, 0x61, 0x77, 0x6c, 0x6f,
	0x67, 0x09, 0x00, 0x61, 0x70, 0x70, 0x2e, 0x73, 0x74, 0x61, 0x72, 0x74,
	0x04, 0x00, 0x6d, 0x61, 0x69, 0x6e, 0x05, 0x00, 0x67, 0x72, 0x61, 0x79,
	0x30, 0x06, 0x00, 0x73, 0x74, 0x61, 0x74, 0x75, 0x73, 0x0d, 0x00, 0x64,
	0x61, 0x74, 0x61, 0x2f, 0x69, 0x63, 0x6f, 0x6e, 0x2e, 0x62, 0x6d, 0x70,
	0x07, 0x00, 0x6c, 0x69, 0x74, 0x65, 0x72, 0x61, 0x6c, 0x0d, 0x00, 0x64,
	0x72, 0x61, 0x77, 0x61, 0x62, 0x6c, 0x65, 0x2f, 0x70, 0x61, 0x67, 0x65,
	0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00,
	0x06, 0x00, 0x00, 0x00, 0x3d, 0x00, 0x00, 0x00, 0x03, 0x02, 0x00, 0x32,
	0x05, 0x2a, 0x03, 0x03, 0x00, 0x32, 0x06, 0x03, 0x04, 0x00, 0x32, 0x16,
	0x03, 0x05, 0x00, 0x01, 0x14, 0x00, 0x00, 0x00, 0x01, 0x18, 0x00, 0x00,
	0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x32,
	0x17, 0x03, 0x07, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
	0x32, 0x18, 0x2a,
};

static const uint8_t display_primitives_sqbc[] = {
	0x53, 0x51, 0x42, 0x43, 0x7a, 0x00, 0x67, 0x01, 0x00, 0x00, 0x09, 0x00,
	0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x7a, 0x00, 0x00, 0x00, 0x1d, 0x00,
	0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x97, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x99, 0x00, 0x00, 0x00, 0x4d, 0x00,
	0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0xe6, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0xe8, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0xea, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xec, 0x00, 0x00, 0x00, 0x0e, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0xfa, 0x00, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x06, 0x01, 0x00, 0x00, 0x61, 0x00,
	0x00, 0x00, 0x12, 0x00, 0x64, 0x69, 0x73, 0x70, 0x6c, 0x61, 0x79, 0x2d,
	0x70, 0x72, 0x69, 0x6d, 0x69, 0x74, 0x69, 0x76, 0x65, 0x73, 0x07, 0x00,
	0x64, 0x65, 0x66, 0x61, 0x75, 0x6c, 0x74, 0x00, 0x00, 0x08, 0x00, 0x12,
	0x00, 0x64, 0x69, 0x73, 0x70, 0x6c, 0x61, 0x79, 0x2d, 0x70, 0x72, 0x69,
	0x6d, 0x69, 0x74, 0x69, 0x76, 0x65, 0x73, 0x09, 0x00, 0x61, 0x70, 0x70,
	0x2e, 0x73, 0x74, 0x61, 0x72, 0x74, 0x04, 0x00, 0x6d, 0x61, 0x69, 0x6e,
	0x05, 0x00, 0x67, 0x72, 0x61, 0x79, 0x30, 0x05, 0x00, 0x48, 0x65, 0x6c,
	0x6c, 0x6f, 0x07, 0x00, 0x6c, 0x69, 0x74, 0x65, 0x72, 0x61, 0x6c, 0x05,
	0x00, 0x67, 0x72, 0x61, 0x79, 0x34, 0x06, 0x00, 0x67, 0x72, 0x61, 0x79,
	0x31, 0x35, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x01, 0x00,
	0x02, 0x00, 0x06, 0x00, 0x00, 0x00, 0x5b, 0x00, 0x00, 0x00, 0x03, 0x02,
	0x00, 0x32, 0x05, 0x2a, 0x03, 0x03, 0x00, 0x32, 0x06, 0x03, 0x04, 0x00,
	0x01, 0x0a, 0x00, 0x00, 0x00, 0x01, 0x14, 0x00, 0x00, 0x00, 0x01, 0x00,
	0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
	0x00, 0x04, 0x04, 0x04, 0x04, 0x32, 0x07, 0x01, 0x01, 0x00, 0x00, 0x00,
	0x01, 0x02, 0x00, 0x00, 0x00, 0x01, 0x03, 0x00, 0x00, 0x00, 0x01, 0x04,
	0x00, 0x00, 0x00, 0x03, 0x06, 0x00, 0x04, 0x32, 0x08, 0x01, 0x05, 0x00,
	0x00, 0x00, 0x01, 0x06, 0x00, 0x00, 0x00, 0x01, 0x07, 0x00, 0x00, 0x00,
	0x01, 0x08, 0x00, 0x00, 0x00, 0x03, 0x07, 0x00, 0x32, 0x09, 0x2a,
};

static bool field_string_equals(const struct sq_protocol_field *field, const char *expected)
{
	return field->type == SQ_FIELD_STRING && field->len == strlen(expected) &&
	       memcmp(field->value, expected, field->len) == 0;
}

static uint8_t ffi_context_storage[65536] __aligned(8);
static uint8_t ffi_scratch[4096];

static struct fs_mount_t test_fs_mount = {
	.type = FS_NATIVE_MOUNT,
	.mnt_point = "/sqtest",
	.fs_data = TEST_FS_DIR,
};

ZTEST_SUITE(squidscript_protocol, NULL, NULL, NULL, NULL, NULL);

static void wait_runtime_done(struct sq_vm_runtime *runtime)
{
	for (int i = 0; i < 100 && runtime->status == SQ_VM_RUNTIME_RUNNING; i++) {
		k_sleep(K_MSEC(1));
	}
}

static int write_test_file(const char *path, const uint8_t *bytes, size_t len)
{
	struct fs_file_t file;
	int result;

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

static int read_test_file(const char *path, uint8_t *bytes, size_t cap, size_t *out_len)
{
	struct fs_dirent entry;
	struct fs_file_t file;
	int result;

	if (bytes == NULL || out_len == NULL) {
		return -EINVAL;
	}
	*out_len = 0;
	result = fs_stat(path, &entry);
	if (result != 0) {
		return result;
	}
	if (entry.type != FS_DIR_ENTRY_FILE || entry.size > cap) {
		return -EINVAL;
	}

	fs_file_t_init(&file);
	result = fs_open(&file, path, FS_O_READ);
	if (result != 0) {
		return result;
	}
	ssize_t read = fs_read(&file, bytes, entry.size);
	result = fs_close(&file);
	if (read < 0) {
		return (int)read;
	}
	if ((size_t)read != entry.size) {
		return -EIO;
	}
	*out_len = (size_t)read;
	return result;
}

static int unlink_test_file_if_exists(const char *path)
{
	struct fs_dirent entry;
	int result = fs_stat(path, &entry);

	if (result == -ENOENT) {
		return 0;
	}
	if (result != 0) {
		return result;
	}
	return fs_unlink(path);
}

static bool resource_value_for_key(const struct sq_protocol_frame *frame, const char *key,
				   uint64_t *out)
{
	size_t offset = 0;
	struct sq_protocol_field entry;

	while (sq_protocol_next_field(frame->payload, frame->payload_len, &offset, &entry) ==
	       SQ_PROTOCOL_OK) {
		size_t record_offset = 0;
		struct sq_protocol_field field;
		const char *record_key = NULL;
		size_t record_key_len = 0;
		uint64_t record_value = 0;
		bool has_value = false;

		if (entry.tag != SQ_DEVICE_RECORD_FIELD_ENTRY || entry.type != SQ_FIELD_RECORD) {
			continue;
		}

		while (sq_protocol_next_field(entry.value, entry.len, &record_offset, &field) ==
		       SQ_PROTOCOL_OK) {
			if (field.tag == SQ_DEVICE_RECORD_FIELD_KEY && field.type == SQ_FIELD_STRING) {
				record_key = (const char *)field.value;
				record_key_len = field.len;
			} else if (field.tag == SQ_DEVICE_RECORD_FIELD_VALUE &&
				   field.type == SQ_FIELD_U64 && field.len == 8) {
				record_value = sq_protocol_read_u64_le(field.value);
				has_value = true;
			}
		}

		if (record_key != NULL && has_value && strlen(key) == record_key_len &&
		    memcmp(record_key, key, record_key_len) == 0) {
			*out = record_value;
			return true;
		}
	}

	return false;
}

static bool resource_value_equals(const struct sq_protocol_frame *frame, const char *key,
				  uint64_t expected)
{
	uint64_t actual = 0;

	return resource_value_for_key(frame, key, &actual) && actual == expected;
}

static int mount_test_fs(void)
{
	int result = fs_mount(&test_fs_mount);

	return result == -EALREADY ? 0 : result;
}

ZTEST(squidscript_protocol, test_decodes_rust_golden_hello_frame)
{
	struct sq_protocol_frame frame;
	struct sq_protocol_field field;
	size_t offset = 0;

	zassert_equal(sq_protocol_decode_frame(hello_frame, sizeof(hello_frame), &frame), 0);
	zassert_equal(frame.kind, SQ_FRAME_REQUEST);
	zassert_equal(frame.opcode, SQ_OPCODE_HELLO);
	zassert_equal(frame.status, SQ_STATUS_OK);
	zassert_equal(frame.sequence, 7);
	zassert_equal(frame.payload_len, 38);
	zassert_equal(frame.payload_crc, 0x5c05a543);

	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field), 0);
	zassert_equal(field.tag, 1);
	zassert_equal(field.type, SQ_FIELD_STRING);
	zassert_equal(field.len, 17);
	zassert_mem_equal(field.value, "esp32c3-supermini", 17);

	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field), 0);
	zassert_equal(field.tag, 2);
	zassert_equal(field.type, SQ_FIELD_BOOL);
	zassert_equal(field.len, 1);
	zassert_equal(field.value[0], 1);

	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field), 0);
	zassert_equal(field.tag, 3);
	zassert_equal(field.type, SQ_FIELD_U64);
	zassert_equal(field.len, 8);
	zassert_equal(sq_protocol_read_u64_le(field.value), 4096);

	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_DONE);
}

ZTEST(squidscript_protocol, test_rejects_payload_crc_mismatch)
{
	uint8_t corrupted[sizeof(hello_frame)];
	struct sq_protocol_frame frame;

	memcpy(corrupted, hello_frame, sizeof(corrupted));
	corrupted[sizeof(corrupted) - 1] ^= 0xff;

	zassert_equal(sq_protocol_decode_frame(corrupted, sizeof(corrupted), &frame),
		      SQ_PROTOCOL_ERR_PAYLOAD_CRC);
}

ZTEST(squidscript_protocol, test_encodes_header_for_existing_payload)
{
	const uint8_t payload[] = {
		0x01, 0x05, 0x08, 0x00, 0x00, 0x40, 0x06, 0x00,
		0x00, 0x00, 0x00, 0x00,
	};
	uint8_t encoded[SQ_PROTOCOL_HEADER_LEN + sizeof(payload)];
	struct sq_protocol_frame frame;

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_RESPONSE, SQ_OPCODE_RESOURCES_GET,
						      SQ_STATUS_OK, 12, payload, sizeof(payload),
						      encoded, sizeof(encoded)), 0);
	memcpy(encoded + SQ_PROTOCOL_HEADER_LEN, payload, sizeof(payload));

	zassert_equal(sq_protocol_decode_frame(encoded, sizeof(encoded), &frame), 0);
	zassert_equal(frame.kind, SQ_FRAME_RESPONSE);
	zassert_equal(frame.opcode, SQ_OPCODE_RESOURCES_GET);
	zassert_equal(frame.sequence, 12);
}

ZTEST(squidscript_protocol, test_handles_hello_request_with_identity_response)
{
	uint8_t response[512];
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct sq_protocol_field field;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_device_protocol_context context = {
		.identity = &identity,
	};
	size_t offset = 0;

	zassert_equal(sq_device_protocol_handle_frame(hello_frame, sizeof(hello_frame), &context,
						      response, sizeof(response), &response_len), 0);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), 0);
	zassert_equal(frame.kind, SQ_FRAME_RESPONSE);
	zassert_equal(frame.opcode, SQ_OPCODE_HELLO);
	zassert_equal(frame.status, SQ_STATUS_OK);
	zassert_equal(frame.sequence, 7);

	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field), 0);
	zassert_equal(field.tag, SQ_DEVICE_FIELD_TARGET);
	zassert_equal(field.type, SQ_FIELD_STRING);
	zassert_mem_equal(field.value, "esp32c3-supermini", 17);

	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field), 0);
	zassert_equal(field.tag, SQ_DEVICE_FIELD_FIRMWARE);
	zassert_equal(field.type, SQ_FIELD_STRING);
	zassert_mem_equal(field.value, "squidscript-zephyr", 18);

	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field), 0);
	zassert_equal(field.tag, SQ_DEVICE_FIELD_DIAGNOSTIC);
	zassert_equal(field.type, SQ_FIELD_BOOL);
	zassert_equal(field.value[0], 1);
}

ZTEST(squidscript_protocol, test_handles_app_list_request_with_registry_records)
{
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_app_registry registry = {
		.count = 2,
		.apps = {
			{.app_id = "alpha", .sqbc_len = 5},
			{.app_id = "beta", .sqbc_len = 6},
		},
	};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
	};
	uint8_t request[SQ_PROTOCOL_HEADER_LEN];
	uint8_t response[128];
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct sq_protocol_field app_record;
	struct sq_protocol_field app_field;
	size_t offset = 0;
	size_t record_offset = 0;

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_APP_LIST,
						      SQ_STATUS_OK, 22, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);

	zassert_equal(sq_device_protocol_handle_frame(request, sizeof(request), &context, response,
						      sizeof(response), &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.kind, SQ_FRAME_RESPONSE);
	zassert_equal(frame.opcode, SQ_OPCODE_APP_LIST);
	zassert_equal(frame.sequence, 22);
	zassert_equal(frame.status, SQ_STATUS_OK);

	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset,
					     &app_record),
		      SQ_PROTOCOL_OK);
	zassert_equal(app_record.tag, 1);
	zassert_equal(app_record.type, SQ_FIELD_RECORD);
	zassert_equal(sq_protocol_next_field(app_record.value, app_record.len, &record_offset,
					     &app_field),
		      SQ_PROTOCOL_OK);
	zassert_equal(app_field.tag, 1);
	zassert_equal(app_field.type, SQ_FIELD_STRING);
	zassert_mem_equal(app_field.value, "alpha", 5);
	zassert_equal(sq_protocol_next_field(app_record.value, app_record.len, &record_offset,
					     &app_field),
		      SQ_PROTOCOL_OK);
	zassert_equal(app_field.tag, 2);
	zassert_equal(app_field.type, SQ_FIELD_U64);
	zassert_equal(sq_protocol_read_u64_le(app_field.value), 5);
}

ZTEST(squidscript_protocol, test_handles_output_get_with_empty_framed_response)
{
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_device_protocol_context context = {
		.identity = &identity,
	};
	uint8_t request[SQ_PROTOCOL_HEADER_LEN];
	uint8_t response[64];
	size_t response_len = 0;
	struct sq_protocol_frame frame;

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_OUTPUT_GET,
						      SQ_STATUS_OK, 24, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, sizeof(request), &context,
						      response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.kind, SQ_FRAME_RESPONSE);
	zassert_equal(frame.opcode, SQ_OPCODE_OUTPUT_GET);
	zassert_equal(frame.status, SQ_STATUS_OK);
	zassert_equal(frame.sequence, 24);
	zassert_equal(frame.payload_len, 0);
}

ZTEST(squidscript_protocol, test_handles_trace_resources_and_wifi_error_frames)
{
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {
		.traces = {"app.start", "state.save"},
		.trace_count = 2,
		.drawlog = {"draw=clear color=gray0", "draw=text text=\"Hello\" x=10 y=20",
			    "draw=rect x=1 y=2 w=3 h=4", "draw=line x1=5 y1=6 x2=7 y2=8"},
		.drawlog_count = 4,
	};
	struct sq_app_registry registry = {.count = 1};
	struct sq_device_install_session install_session = {0};
	struct sq_device_temp_session temp_session = {0};
	struct sq_device_resource_session resource_session = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
		.install_session = &install_session,
		.temp_session = &temp_session,
		.resource_session = &resource_session,
		.runtime = &runtime,
	};
	uint8_t request[SQ_PROTOCOL_HEADER_LEN];
	uint8_t response[512];
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct sq_protocol_field field;
	size_t offset = 0;

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_TRACE_GET,
						      SQ_STATUS_OK, 61, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, sizeof(request), &context,
						      response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_TRACE_GET);
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_equal(field.tag, SQ_DEVICE_LINE_FIELD_VALUE);
	zassert_mem_equal(field.value, "app.start", 9);

	offset = 0;
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_DRAWLOG_GET,
						      SQ_STATUS_OK, 64, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, sizeof(request), &context,
						      response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_DRAWLOG_GET);
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_equal(field.tag, SQ_DEVICE_LINE_FIELD_VALUE);
	zassert_mem_equal(field.value, "draw=clear color=gray0", strlen("draw=clear color=gray0"));
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_mem_equal(field.value, "draw=text text=\"Hello\" x=10 y=20",
			  strlen("draw=text text=\"Hello\" x=10 y=20"));
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_mem_equal(field.value, "draw=rect x=1 y=2 w=3 h=4",
			  strlen("draw=rect x=1 y=2 w=3 h=4"));
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_mem_equal(field.value, "draw=line x1=5 y1=6 x2=7 y2=8",
			  strlen("draw=line x1=5 y1=6 x2=7 y2=8"));

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_RESOURCES_GET,
						      SQ_STATUS_OK, 62, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, sizeof(request), &context,
						      response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_RESOURCES_GET);
	zassert_true(frame.payload_len > 0);

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_WIFI_PROFILE_SET,
						      SQ_STATUS_OK, 63, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, sizeof(request), &context,
						      response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_WIFI_PROFILE_SET);
	zassert_equal(frame.status, SQ_STATUS_ERROR);
}

ZTEST(squidscript_protocol, test_errors_get_reports_vm_status_label_and_errno)
{
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {
		.status = SQ_VM_RUNTIME_ERROR,
		.result_code = -EINVAL,
		.result = {
			.status = SQVM_STATUS_INVALID_ARGUMENT,
		},
	};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.runtime = &runtime,
	};
	uint8_t request[SQ_PROTOCOL_HEADER_LEN];
	uint8_t response[128];
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct sq_protocol_field field;
	size_t offset = 0;

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_ERRORS_GET,
						      SQ_STATUS_OK, 65, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, sizeof(request), &context,
						      response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_ERRORS_GET);
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_true(field_string_equals(&field, "runtime=invalid_argument code=-22"));
}

ZTEST(squidscript_protocol, test_wifi_profile_set_stores_volatile_profile_without_echoing_secret)
{
	uint8_t payload[96];
	uint8_t request[SQ_PROTOCOL_HEADER_LEN + sizeof(payload)];
	uint8_t response[128];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct sq_vm_runtime runtime = {0};
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.runtime = &runtime,
	};

	sq_vm_runtime_init(&runtime);
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "dev"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 2,
						      "ExampleSSID"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 3,
						      "secret-pass"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_WIFI_PROFILE_SET,
						      SQ_STATUS_OK, 76, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);

	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_WIFI_PROFILE_SET);
	zassert_equal(frame.status, SQ_STATUS_OK);
	zassert_equal(frame.payload_len, 0);
	zassert_equal(runtime.wifi_profile_len, 3);
	zassert_equal(runtime.wifi_profile_ssid_len, 11);
	zassert_equal(runtime.wifi_profile_password_len, 11);
	zassert_mem_equal(runtime.wifi_profile, "dev", 3);
	zassert_mem_equal(runtime.wifi_profile_ssid, "ExampleSSID", 11);
	zassert_mem_equal(runtime.wifi_profile_password, "secret-pass", 11);
}

ZTEST(squidscript_protocol, test_storage_format_clears_runtime_before_erasing_files)
{
	const uint8_t sqbc[] = {'s', 'q', 'b', 'c'};
	uint8_t request[SQ_PROTOCOL_HEADER_LEN];
	uint8_t response[128];
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct fs_dirent entry;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {
		.status = SQ_VM_RUNTIME_COMPLETE,
		.trace_count = 1,
		.traces = {"state.save"},
	};
	struct sq_app_registry registry = {.count = 1};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_install_session install_session = {.active = true};
	struct sq_device_temp_session temp_session = {.active = true};
	struct sq_device_resource_session resource_session = {.active = true};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
		.mutable_registry = &registry,
		.install_session = &install_session,
		.temp_session = &temp_session,
		.resource_session = &resource_session,
		.runtime = &runtime,
		.store_mount_point = test_fs_mount.mnt_point,
		.launch_storage = &launch_storage,
	};
	int handle_result;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "main", sqbc, sizeof(sqbc)),
		      0);
	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point, "main",
						     &launch_storage),
		      0);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_STORAGE_FORMAT,
						      SQ_STATUS_OK, 64, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);

	handle_result = sq_device_protocol_handle_frame(request, sizeof(request), &context, response,
						       sizeof(response), &response_len);
	zassert_equal(handle_result, SQ_PROTOCOL_OK, "handle result %d", handle_result);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_STORAGE_FORMAT);
	zassert_equal(frame.status, SQ_STATUS_OK);
	zassert_equal(registry.count, 0);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_IDLE);
	zassert_false(install_session.active);
	zassert_false(temp_session.active);
	zassert_false(resource_session.active);
	zassert_equal(launch_storage.sqbc_path[0], '\0');
	zassert_equal(fs_stat("/sqtest/apps/main/main.sqbc", &entry), -ENOENT);
	zassert_equal(fs_stat("/sqtest/apps/main", &entry), -ENOENT);
	zassert_equal(fs_stat("/sqtest/apps", &entry), 0);
	zassert_equal(fs_stat("/sqtest/state", &entry), 0);
	zassert_equal(fs_stat("/sqtest/tmp", &entry), 0);
	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_handles_installed_app_begin_chunk_commit)
{
	const uint8_t chunk_a[] = {'h', 'e', 'l'};
	const uint8_t chunk_b[] = {'l', 'o'};
	uint8_t begin_payload[64];
	uint8_t chunk_payload[32];
	uint8_t request[128];
	uint8_t response[128];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_app_registry registry = {0};
	struct sq_device_install_session install_session = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
		.install_session = &install_session,
		.store_mount_point = test_fs_mount.mnt_point,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_prepare_filesystem(test_fs_mount.mnt_point), 0);
	zassert_true(sizeof(install_session) < 512,
		     "installed app write session must not reserve full SQBC payload RAM");

	zassert_equal(sq_protocol_append_string_field(begin_payload, sizeof(begin_payload),
						     &payload_len, 1, "framed-app"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_u64_field(begin_payload, sizeof(begin_payload),
						  &payload_len, 2, 5),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_u64_field(begin_payload, sizeof(begin_payload),
						  &payload_len, 3,
						  sq_protocol_crc32((const uint8_t *)"hello", 5)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST,
						      SQ_OPCODE_APP_INSTALL_BEGIN,
						      SQ_STATUS_OK, 30, begin_payload,
						      payload_len, request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], begin_payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);

	payload_len = 0;
	zassert_equal(sq_protocol_append_u64_field(chunk_payload, sizeof(chunk_payload),
						  &payload_len, 1, 0),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_bytes_field(chunk_payload, sizeof(chunk_payload),
						    &payload_len, 2, chunk_a,
						    sizeof(chunk_a)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST,
						      SQ_OPCODE_APP_INSTALL_CHUNK,
						      SQ_STATUS_OK, 31, chunk_payload,
						      payload_len, request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], chunk_payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);

	payload_len = 0;
	zassert_equal(sq_protocol_append_u64_field(chunk_payload, sizeof(chunk_payload),
						  &payload_len, 1, 3),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_bytes_field(chunk_payload, sizeof(chunk_payload),
						    &payload_len, 2, chunk_b,
						    sizeof(chunk_b)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST,
						      SQ_OPCODE_APP_INSTALL_CHUNK,
						      SQ_STATUS_OK, 32, chunk_payload,
						      payload_len, request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], chunk_payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST,
						      SQ_OPCODE_APP_INSTALL_COMMIT,
						      SQ_STATUS_OK, 33, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN, &context,
						      response, sizeof(response), &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_app_store_scan_registry(test_fs_mount.mnt_point, &registry), 0);
	zassert_not_null(sq_app_registry_find(&registry, "framed-app"));

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_handles_app_launch_dispatches_installed_app_start)
{
	uint8_t payload[32];
	uint8_t request[64];
	uint8_t response[128];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "launch-app",
					       headless_counter_sqbc,
					       sizeof(headless_counter_sqbc)),
		      0);
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "launch-app"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_APP_LAUNCH,
						      SQ_STATUS_OK, 40, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);

	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_true(response_len >= SQ_PROTOCOL_HEADER_LEN);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_RUNNING);
	wait_runtime_done(&runtime);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);
	zassert_equal(runtime.result_code, 0);
	zassert_equal(runtime.trace_count, 3);
	zassert_str_equal(runtime.traces[0], "app.start");
	zassert_str_equal(runtime.traces[1], "state.load");
	zassert_str_equal(runtime.traces[2], "state.save");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_foreground_events_preserve_vm_memory_until_relaunch)
{
	uint8_t payload[96];
	uint8_t request[144];
	uint8_t response[256];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "foreground-memory",
					       foreground_memory_sqbc,
					       sizeof(foreground_memory_sqbc)),
		      0);

	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "foreground-memory"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_APP_LAUNCH,
						      SQ_STATUS_OK, 440, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	wait_runtime_done(&runtime);
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "memory start 1");

	for (uint32_t sequence = 441; sequence <= 442; sequence++) {
		payload_len = 0;
		zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload),
							      &payload_len, 1,
							      "foreground-memory"),
			      SQ_PROTOCOL_OK);
		zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload),
							      &payload_len, 2,
							      "key.SELECT"),
			      SQ_PROTOCOL_OK);
		zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST,
							      SQ_OPCODE_EVENT_DISPATCH,
							      SQ_STATUS_OK, sequence, payload,
							      payload_len, request,
							      sizeof(request)),
			      SQ_PROTOCOL_OK);
		memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
		zassert_equal(sq_device_protocol_handle_frame(
				      request, SQ_PROTOCOL_HEADER_LEN + payload_len, &context,
				      response, sizeof(response), &response_len),
			      SQ_PROTOCOL_OK);
		wait_runtime_done(&runtime);
	}
	zassert_equal(runtime.output_count, 3);
	zassert_str_equal(runtime.outputs[1], "memory select 2");
	zassert_str_equal(runtime.outputs[2], "memory select 3");

	payload_len = 0;
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "foreground-memory"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_APP_LAUNCH,
						      SQ_STATUS_OK, 443, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	wait_runtime_done(&runtime);
	zassert_equal(runtime.output_count, 4);
	zassert_str_equal(runtime.outputs[3], "memory start 1");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_event_dispatch_rejects_non_foreground_app_target)
{
	uint8_t payload[80];
	uint8_t request[128];
	uint8_t response[256];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "headless-counter",
					       headless_counter_sqbc,
					       sizeof(headless_counter_sqbc)),
		      0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "reader", reader_exit_sqbc,
					       sizeof(reader_exit_sqbc)),
		      0);

	strncpy(runtime.current_app, "headless-counter", sizeof(runtime.current_app) - 1);
	runtime.context_ready = true;
	zassert_str_equal(runtime.current_app, "headless-counter");
	zassert_true(runtime.context_ready);

	payload_len = 0;
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "reader"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 2,
						      "repl"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_EVENT_DISPATCH,
						      SQ_STATUS_OK, 402, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);

	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_EVENT_DISPATCH);
	zassert_equal(frame.status, SQ_STATUS_ERROR);
	zassert_str_equal(runtime.current_app, "headless-counter");
	zassert_true(runtime.context_ready);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_event_dispatch_exposes_lifecycle_trace_records)
{
	uint8_t payload[80];
	uint8_t request[128];
	uint8_t response[512];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct sq_protocol_field field;
	size_t offset = 0;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "lifecycle", lifecycle_sqbc,
					       sizeof(lifecycle_sqbc)),
		      0);
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "lifecycle"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 2,
						      "repl"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_EVENT_DISPATCH,
						      SQ_STATUS_OK, 41, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);

	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	wait_runtime_done(&runtime);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);
	zassert_equal(runtime.result_code, 0);

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_TRACE_GET,
						      SQ_STATUS_OK, 42, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN, &context,
						      response, sizeof(response), &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_TRACE_GET);
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_mem_equal(field.value, "repl", strlen("repl"));
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_mem_equal(field.value, "app.arm break-reminder",
			  strlen("app.arm break-reminder"));
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_mem_equal(field.value, "app.launch reader", strlen("app.launch reader"));
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_mem_equal(field.value, "app.disarm break-reminder",
			  strlen("app.disarm break-reminder"));

	k_sleep(K_MSEC(300));
	zassert_equal(sq_vm_runtime_poll(&runtime), 0);
	wait_runtime_done(&runtime);

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_TRACE_GET,
						      SQ_STATUS_OK, 43, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN, &context,
						      response, sizeof(response), &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	offset = 0;
	bool saw_arm = false;
	bool saw_launch = false;
	bool saw_disarm = false;
	while (sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field) ==
	       SQ_PROTOCOL_OK) {
		saw_arm = saw_arm || field_string_equals(&field, "app.arm break-reminder");
		saw_launch = saw_launch || field_string_equals(&field, "app.launch reader");
		saw_disarm = saw_disarm || field_string_equals(&field, "app.disarm break-reminder");
	}
	zassert_true(saw_arm);
	zassert_true(saw_launch);
	zassert_true(saw_disarm);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_app_launch_and_exit_update_foreground_stack)
{
	uint8_t payload[80];
	uint8_t request[128];
	uint8_t response[512];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "lifecycle", lifecycle_sqbc,
					       sizeof(lifecycle_sqbc)),
		      0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "reader", reader_exit_sqbc,
					       sizeof(reader_exit_sqbc)),
		      0);

	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "lifecycle"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_APP_LAUNCH,
						      SQ_STATUS_OK, 44, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	wait_runtime_done(&runtime);
	zassert_str_equal(runtime.current_app, "lifecycle");
	zassert_equal(runtime.return_stack_count, 0);

	payload_len = 0;
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "lifecycle"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 2,
						      "repl"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_EVENT_DISPATCH,
						      SQ_STATUS_OK, 45, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	wait_runtime_done(&runtime);
	for (int i = 0; i < 20; i++) {
		zassert_equal(sq_device_protocol_poll(&context), 0);
		if (runtime.status != SQ_VM_RUNTIME_RUNNING && strcmp(runtime.current_app, "reader") == 0) {
			break;
		}
		k_sleep(K_MSEC(1));
	}
	zassert_str_equal(runtime.current_app, "reader");
	zassert_equal(runtime.return_stack_count, 1);
	zassert_str_equal(runtime.return_stack[0], "lifecycle");
	zassert_equal(runtime.output_count, 2);
	zassert_str_equal(runtime.outputs[1], "reader start");

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_LIFECYCLE_GET,
						      SQ_STATUS_OK, 47, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN, &context,
						      response, sizeof(response), &response_len),
		      SQ_PROTOCOL_OK);
	struct sq_protocol_frame frame;
	struct sq_protocol_field field;
	size_t offset = 0;
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_true(field_string_equals(&field, "active=reader"));
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_true(field_string_equals(&field, "process_stack[0]=lifecycle"));
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_true(field_string_equals(&field, "armed_stack="));

	payload_len = 0;
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "reader"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 2,
						      "repl"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_EVENT_DISPATCH,
						      SQ_STATUS_OK, 46, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	wait_runtime_done(&runtime);
	for (int i = 0; i < 20; i++) {
		zassert_equal(sq_device_protocol_poll(&context), 0);
		if (runtime.status != SQ_VM_RUNTIME_RUNNING &&
		    strcmp(runtime.current_app, "lifecycle") == 0) {
			break;
		}
		k_sleep(K_MSEC(1));
	}
	zassert_str_equal(runtime.current_app, "lifecycle");
	zassert_equal(runtime.return_stack_count, 0);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_app_arm_registers_timer_and_dispatches_armed_app)
{
	enum { PADDED_TRIGGER_SQBC_LEN = 4096 + SQVM_STORAGE_TRANSFER_CAPACITY };
	static uint8_t padded_break_reminder_sqbc[PADDED_TRIGGER_SQBC_LEN];
	uint8_t payload[80];
	uint8_t request[128];
	uint8_t response[512];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
	};

	memcpy(padded_break_reminder_sqbc, break_reminder_sqbc, sizeof(break_reminder_sqbc));
	memset(&padded_break_reminder_sqbc[sizeof(break_reminder_sqbc)], 0xa5,
	       sizeof(padded_break_reminder_sqbc) - sizeof(break_reminder_sqbc));
	padded_break_reminder_sqbc[6] = PADDED_TRIGGER_SQBC_LEN & 0xff;
	padded_break_reminder_sqbc[7] = (PADDED_TRIGGER_SQBC_LEN >> 8) & 0xff;
	padded_break_reminder_sqbc[8] = (PADDED_TRIGGER_SQBC_LEN >> 16) & 0xff;
	padded_break_reminder_sqbc[9] = (PADDED_TRIGGER_SQBC_LEN >> 24) & 0xff;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "armer", armer_sqbc,
					       sizeof(armer_sqbc)),
		      0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "break-reminder",
					       padded_break_reminder_sqbc,
					       sizeof(padded_break_reminder_sqbc)),
		      0);

	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "armer"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_APP_LAUNCH,
						      SQ_STATUS_OK, 90, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	wait_runtime_done(&runtime);
	for (int i = 0; i < 20; i++) {
		zassert_equal(sq_device_protocol_poll(&context), 0);
		if (runtime.status != SQ_VM_RUNTIME_RUNNING && runtime.armed_timer_count == 1) {
			break;
		}
		k_sleep(K_MSEC(1));
	}

	zassert_str_equal(runtime.current_app, "armer");
	zassert_equal(runtime.armed_timer_count, 1);

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_LIFECYCLE_GET,
						      SQ_STATUS_OK, 91, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN, &context,
						      response, sizeof(response), &response_len),
		      SQ_PROTOCOL_OK);
	struct sq_protocol_frame frame;
	struct sq_protocol_field field;
	size_t offset = 0;
	bool saw_armed = false;
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	while (sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field) ==
	       SQ_PROTOCOL_OK) {
		saw_armed = saw_armed ||
			    field_string_equals(&field,
						"armed_stack[0]=break-reminder timer.break");
	}
	zassert_true(saw_armed);

	k_sleep(K_MSEC(5));
	for (int i = 0; i < 40; i++) {
		zassert_equal(sq_device_protocol_poll(&context), 0);
		if (runtime.status != SQ_VM_RUNTIME_RUNNING &&
		    strcmp(runtime.current_app, "break-reminder") == 0) {
			break;
		}
		k_sleep(K_MSEC(1));
	}
	zassert_str_equal(runtime.current_app, "break-reminder");
	zassert_equal(runtime.return_stack_count, 1);
	zassert_str_equal(runtime.return_stack[0], "armer");
	zassert_equal(runtime.output_count, 2);
	zassert_str_equal(runtime.outputs[1], "break fired 1");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_app_exit_return_takes_priority_over_due_foreground_timer)
{
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "reader", reader_exit_sqbc,
					       sizeof(reader_exit_sqbc)),
		      0);

	sq_vm_runtime_init(&runtime);
	strncpy(runtime.current_app, "break-reminder", sizeof(runtime.current_app) - 1);
	strncpy(runtime.return_stack[0], "reader", sizeof(runtime.return_stack[0]) - 1);
	runtime.return_stack_count = 1;
	runtime.dispatch_exited = true;
	runtime.status = SQ_VM_RUNTIME_COMPLETE;
	runtime.job_backend = sq_app_store_vm_storage_backend(&launch_storage);
	runtime.timers[0].active = true;
	runtime.timers[0].repeating = true;
	runtime.timers[0].interval_ms = 500;
	runtime.timers[0].due_ms = k_uptime_get() - 1;
	strncpy(runtime.timers[0].event, "timer.clock", sizeof(runtime.timers[0].event) - 1);

	zassert_equal(sq_device_protocol_poll(&context), 0);
	zassert_str_equal(runtime.current_app, "reader");
	zassert_equal(runtime.return_stack_count, 0);
	wait_runtime_done(&runtime);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_foreground_timers_clear_when_armed_app_takes_foreground)
{
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "break-reminder",
					       break_reminder_sqbc, sizeof(break_reminder_sqbc)),
		      0);

	sq_vm_runtime_init(&runtime);
	strncpy(runtime.current_app, "reader", sizeof(runtime.current_app) - 1);
	runtime.status = SQ_VM_RUNTIME_COMPLETE;
	runtime.timers[0].active = true;
	runtime.timers[0].repeating = true;
	runtime.timers[0].interval_ms = 500;
	runtime.timers[0].due_ms = k_uptime_get() - 1;
	strncpy(runtime.timers[0].event, "timer.clock", sizeof(runtime.timers[0].event) - 1);
	runtime.armed_timers[0].active = true;
	runtime.armed_timers[0].repeating = false;
	runtime.armed_timers[0].interval_ms = 1000;
	runtime.armed_timers[0].due_ms = k_uptime_get() - 1;
	strncpy(runtime.armed_timers[0].app_id, "break-reminder",
		sizeof(runtime.armed_timers[0].app_id) - 1);
	strncpy(runtime.armed_timers[0].event, "timer.break",
		sizeof(runtime.armed_timers[0].event) - 1);

	zassert_equal(sq_device_protocol_poll(&context), 0);
	zassert_str_equal(runtime.current_app, "break-reminder");
	zassert_false(runtime.timers[0].active);
	wait_runtime_done(&runtime);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_handles_temp_run_commit_dispatches_file_staged_app_start)
{
	uint8_t begin_payload[64];
	uint8_t chunk_payload[512];
	uint8_t request[768];
	uint8_t response[128];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_device_temp_session temp_session = {0};
	struct sq_vm_runtime runtime = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.temp_session = &temp_session,
		.runtime = &runtime,
		.store_mount_point = test_fs_mount.mnt_point,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_prepare_filesystem(test_fs_mount.mnt_point), 0);
	zassert_true(sizeof(temp_session) < 512,
		     "temp-run session must not reserve full SQBC payload RAM");

	zassert_equal(sq_protocol_append_string_field(begin_payload, sizeof(begin_payload),
						     &payload_len, 1, "temp-app"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_u64_field(begin_payload, sizeof(begin_payload),
						  &payload_len, 2,
						  sizeof(headless_counter_sqbc)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_u64_field(begin_payload, sizeof(begin_payload),
						  &payload_len, 3,
						  sq_protocol_crc32(headless_counter_sqbc,
								    sizeof(headless_counter_sqbc))),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_TEMP_RUN_BEGIN,
						      SQ_STATUS_OK, 50, begin_payload,
						      payload_len, request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], begin_payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);

	payload_len = 0;
	zassert_equal(sq_protocol_append_u64_field(chunk_payload, sizeof(chunk_payload),
						  &payload_len, 1, 0),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_bytes_field(chunk_payload, sizeof(chunk_payload),
						    &payload_len, 2, headless_counter_sqbc,
						    sizeof(headless_counter_sqbc)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_TEMP_RUN_CHUNK,
						      SQ_STATUS_OK, 51, chunk_payload,
						      payload_len, request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], chunk_payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_TEMP_RUN_COMMIT,
						      SQ_STATUS_OK, 52, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN, &context,
						      response, sizeof(response), &response_len),
		      SQ_PROTOCOL_OK);
	zassert_true(response_len >= SQ_PROTOCOL_HEADER_LEN);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_RUNNING);
	wait_runtime_done(&runtime);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);
	zassert_equal(runtime.result_code, 0);
	zassert_equal(runtime.trace_count, 3);
	zassert_str_equal(runtime.traces[0], "app.start");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_serial_transport_accumulates_one_complete_frame)
{
	struct sq_serial_transport transport;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_device_protocol_context context = {
		.identity = &identity,
	};
	uint8_t response[128];
	size_t response_len = 0;
	int completed = 0;

	sq_serial_transport_init(&transport);

	for (size_t i = 0; i < sizeof(hello_frame); i++) {
		int result = sq_serial_transport_push_byte(&transport, hello_frame[i], &context,
							   response, sizeof(response), &response_len);
		zassert_true(result >= 0, "transport rejected byte %zu with %d", i, result);
		completed += result;
	}

	zassert_equal(completed, 1);
	zassert_true(response_len > SQ_PROTOCOL_HEADER_LEN);

	struct sq_protocol_frame frame;
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), 0);
	zassert_equal(frame.kind, SQ_FRAME_RESPONSE);
	zassert_equal(frame.opcode, SQ_OPCODE_HELLO);
	zassert_equal(frame.sequence, 7);
}

ZTEST(squidscript_protocol, test_links_squidvm_ffi_context_metadata)
{
	zassert_true(sqvm_context_size() > 0);
	zassert_true(sqvm_context_align() > 0);
	zassert_true(sqvm_context_size() <= SQ_VM_RUNTIME_CONTEXT_BYTES);
#if !defined(CONFIG_BOARD_NATIVE_SIM)
	zassert_true(SQ_VM_RUNTIME_CONTEXT_BYTES <= 11264);
#endif
	zassert_true(SQ_VM_RUNTIME_WORK_STACK_SIZE <= 24576);
}

ZTEST(squidscript_protocol, test_runtime_reuses_transfer_storage_for_init_scratch_and_completion)
{
	static struct sq_vm_runtime runtime;

	zassert_equal(sizeof(runtime.transfer.init_scratch), SQ_VM_RUNTIME_SCRATCH_BYTES);
	zassert_true(sizeof(runtime.transfer) >= sizeof(runtime.transfer.init_scratch));
	zassert_true(sizeof(runtime.transfer) >= sizeof(runtime.transfer.completion));
#if !defined(CONFIG_BOARD_NATIVE_SIM)
	size_t runtime_static = sizeof(runtime);
	zassert_true(runtime_static <= 16640, "runtime_static=%zu", runtime_static);
#endif
}

ZTEST(squidscript_protocol, test_resources_report_vm_worker_stack_diagnostics)
{
	uint8_t request[SQ_PROTOCOL_HEADER_LEN];
	uint8_t response[SQ_DEVICE_RESPONSE_BYTES];
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	static struct sq_vm_runtime runtime;
	struct sq_device_identity identity = {
		.target = "native-test",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.runtime = &runtime,
	};
	uint64_t stack_unused = 0;
	uint64_t stack_used = 0;
	uint64_t protocol_stack_unused = 0;
	uint64_t protocol_stack_used = 0;
	uint64_t vm_sqbc_chunk = 0;
	int result;

	memset(&runtime, 0, sizeof(runtime));
	sq_vm_runtime_init(&runtime);

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_RESOURCES_GET,
						      SQ_STATUS_OK, 73, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(request, sizeof(request), &frame), SQ_PROTOCOL_OK,
		      "request decode result before handle");
	result = sq_device_protocol_handle_frame(request, sizeof(request), &context, response,
						 sizeof(response), &response_len);
	zassert_equal(result, SQ_PROTOCOL_OK, "resources result %d", result);
	zassert_true(response_len <= SQ_DEVICE_RESPONSE_BYTES);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);

	zassert_true(resource_value_equals(&frame, "vm_worker_stack_size_bytes",
					   SQ_VM_RUNTIME_WORK_STACK_SIZE));
	zassert_true(resource_value_equals(&frame, "protocol_thread_stack_size_bytes",
					   CONFIG_MAIN_STACK_SIZE));
	zassert_true(resource_value_for_key(&frame, "vm_sqbc_chunk_bytes", &vm_sqbc_chunk));
	zassert_equal(vm_sqbc_chunk, SQVM_STORAGE_TRANSFER_CAPACITY);
	zassert_true(resource_value_for_key(&frame, "protocol_thread_stack_unused_bytes",
					    &protocol_stack_unused));
	zassert_true(resource_value_for_key(&frame, "protocol_thread_stack_used_bytes",
					    &protocol_stack_used));
	zassert_true(protocol_stack_unused <= CONFIG_MAIN_STACK_SIZE);
	zassert_true(protocol_stack_used <= CONFIG_MAIN_STACK_SIZE);
	if (protocol_stack_unused != 0 || protocol_stack_used != 0) {
		zassert_equal(protocol_stack_unused + protocol_stack_used, CONFIG_MAIN_STACK_SIZE,
			      "unused=%llu used=%llu", protocol_stack_unused, protocol_stack_used);
	}
	zassert_true(resource_value_for_key(&frame, "vm_worker_stack_unused_bytes", &stack_unused));
	zassert_true(resource_value_for_key(&frame, "vm_worker_stack_used_bytes", &stack_used));
	zassert_true(stack_unused <= SQ_VM_RUNTIME_WORK_STACK_SIZE);
	zassert_true(stack_used <= SQ_VM_RUNTIME_WORK_STACK_SIZE);
	zassert_equal(stack_unused + stack_used, SQ_VM_RUNTIME_WORK_STACK_SIZE,
		      "unused=%llu used=%llu", stack_unused, stack_used);
}

ZTEST(squidscript_protocol, test_exposes_resumable_squidvm_ffi_abi)
{
	SqvmCallbacks callbacks = {0};
	SqvmDispatchResult result = {0};
	SqvmStorageCompletion completion = {0};

	zassert_equal(sqvm_storage_transfer_capacity(), SQVM_STORAGE_TRANSFER_CAPACITY);
	zassert_equal(sqvm_saved_state_capacity(), SQVM_SAVED_STATE_CAPACITY);
	zassert_equal(sizeof(result.storage.bytes), SQVM_STORAGE_TRANSFER_CAPACITY);
	zassert_equal(sizeof(completion.bytes), SQVM_STORAGE_TRANSFER_CAPACITY);
	zassert_equal(SQ_VM_RUNTIME_SCRATCH_BYTES, SQVM_STORAGE_TRANSFER_CAPACITY);
	zassert_equal(SQ_DEVICE_TEMP_STATE_BYTES, SQVM_SAVED_STATE_CAPACITY);

	zassert_equal(sqvm_dispatch_start_resumable(NULL, callbacks, (const uint8_t *)"app.start",
						    9, &result),
		      SQVM_STATUS_INVALID_ARGUMENT);
	zassert_equal(sqvm_dispatch_resume_storage(NULL, callbacks, &completion, &result),
		      SQVM_STATUS_INVALID_ARGUMENT);
	zassert_str_equal(sq_vm_runtime_status_name(SQVM_STATUS_INVALID_ARGUMENT),
			  "invalid_argument");
	zassert_equal(sq_vm_runtime_status_to_errno(SQVM_STATUS_INVALID_ARGUMENT), -EINVAL);
	zassert_str_equal(sq_vm_runtime_status_name(SQVM_STATUS_VM_ERROR), "vm_error");
	zassert_equal(sq_vm_runtime_status_to_errno(SQVM_STATUS_VM_ERROR), -EIO);
}

ZTEST(squidscript_protocol, test_vm_runtime_callback_boundary_statuses)
{
	static const char *timer_events[] = {
		"timer.0",  "timer.1",  "timer.2",  "timer.3",  "timer.4",  "timer.5",
		"timer.6",  "timer.7",  "timer.8",  "timer.9",  "timer.10", "timer.11",
		"timer.12", "timer.13", "timer.14", "timer.15",
	};
	struct sq_vm_runtime runtime = {0};
	bool indicator = false;

	zassert_true(ARRAY_SIZE(timer_events) >= SQ_VM_RUNTIME_TIMER_MAX);
	sq_vm_runtime_init(&runtime);

	zassert_equal(sq_vm_runtime_indicator_read(NULL, &indicator), -EINVAL);
	zassert_equal(sq_vm_runtime_indicator_read(&runtime, NULL), -EINVAL);
	zassert_equal(sq_vm_runtime_indicator_blink(&runtime, 0, 80), -EINVAL);
	zassert_equal(sq_vm_runtime_indicator_write(&runtime, true), -ENODEV);
	zassert_equal(sq_vm_runtime_hardware_gpio_write(NULL, (const uint8_t *)"GPIO8", 5, true),
		      -EINVAL);
	zassert_equal(sq_vm_runtime_hardware_gpio_write(&runtime, NULL, 0, true), -EINVAL);
	zassert_equal(sq_vm_runtime_hardware_gpio_write(&runtime, (const uint8_t *)"BAD8", 4,
							true),
		      -EINVAL);
	zassert_equal(sq_vm_runtime_hardware_gpio_write(&runtime, (const uint8_t *)"GPIO26", 6,
							true),
		      -EINVAL);

	zassert_equal(sq_vm_runtime_register_timer(NULL, (const uint8_t *)"timer.ok", 8, 100,
						   true),
		      -EINVAL);
	zassert_equal(sq_vm_runtime_register_timer(&runtime, NULL, 0, 100, true), -EINVAL);
	zassert_equal(sq_vm_runtime_register_timer(&runtime, (const uint8_t *)"timer.ok", 8, 0,
						   true),
		      -EINVAL);

	for (size_t i = 0; i < SQ_VM_RUNTIME_TIMER_MAX; i++) {
		zassert_equal(sq_vm_runtime_register_timer(&runtime,
							   (const uint8_t *)timer_events[i],
							   strlen(timer_events[i]), 100, true),
			      0);
	}
	zassert_equal(sq_vm_runtime_register_timer(&runtime, (const uint8_t *)"timer.overflow",
						   strlen("timer.overflow"), 100, true),
		      -ENOSPC);
	zassert_equal(sq_vm_runtime_register_timer(&runtime, (const uint8_t *)timer_events[0],
						   strlen(timer_events[0]), 200, false),
		      0);
	zassert_false(runtime.timers[0].repeating);
	zassert_equal(runtime.timers[0].interval_ms, 200);
}

ZTEST(squidscript_protocol, test_transfer_sessions_use_internal_staging_path_capacity)
{
	zassert_equal(SQ_DEVICE_STAGING_PATH_BYTES, 80);
	zassert_true(SQ_DEVICE_STAGING_PATH_BYTES < SQ_APP_STORE_PATH_MAX);
	zassert_equal(sizeof(((struct sq_device_install_session *)0)->staging_path),
		      SQ_DEVICE_STAGING_PATH_BYTES);
	zassert_equal(sizeof(((struct sq_device_temp_session *)0)->staging_path),
		      SQ_DEVICE_STAGING_PATH_BYTES);
	zassert_equal(sizeof(((struct sq_device_resource_session *)0)->staging_path),
		      SQ_DEVICE_STAGING_PATH_BYTES);
	zassert_equal(sizeof(((struct sq_device_resource_session *)0)->resource_path),
		      SQ_APP_STORE_PATH_MAX);
}

struct vm_storage_fixture {
	const uint8_t *sqbc;
	size_t sqbc_len;
	uint8_t state[SQVM_STORAGE_TRANSFER_CAPACITY];
	size_t state_len;
	bool state_present;
	bool reset_called;
};

struct ffi_vm_fixture {
	struct vm_storage_fixture storage;
	char traces[4][16];
	size_t trace_count;
};

static int fixture_read_sqbc(void *user_data, size_t offset, uint8_t *out, size_t len);

static void ffi_trace(void *user_data, const uint8_t *message, size_t message_len)
{
	struct ffi_vm_fixture *fixture = user_data;

	if (fixture->trace_count >= ARRAY_SIZE(fixture->traces)) {
		return;
	}
	size_t len = MIN(message_len, sizeof(fixture->traces[0]) - 1);
	memcpy(fixture->traces[fixture->trace_count], message, len);
	fixture->traces[fixture->trace_count][len] = '\0';
	fixture->trace_count++;
}

static int32_t ffi_read_exact_at(void *user_data, size_t offset, uint8_t *out, size_t out_len)
{
	struct ffi_vm_fixture *fixture = user_data;

	return fixture_read_sqbc(&fixture->storage, offset, out, out_len);
}

static int fixture_read_sqbc(void *user_data, size_t offset, uint8_t *out, size_t len)
{
	struct vm_storage_fixture *fixture = user_data;

	if (offset > fixture->sqbc_len || len > fixture->sqbc_len - offset) {
		return -EINVAL;
	}
	memcpy(out, fixture->sqbc + offset, len);
	return 0;
}

static int fixture_load_state(void *user_data, uint8_t *out, size_t out_len, size_t *len)
{
	struct vm_storage_fixture *fixture = user_data;

	if (!fixture->state_present) {
		*len = 0;
		return 0;
	}
	if (fixture->state_len > out_len) {
		return -ENOSPC;
	}
	memcpy(out, fixture->state, fixture->state_len);
	*len = fixture->state_len;
	return 0;
}

static int fixture_save_state(void *user_data, const uint8_t *bytes, size_t len)
{
	struct vm_storage_fixture *fixture = user_data;

	if (len > sizeof(fixture->state)) {
		return -ENOSPC;
	}
	memcpy(fixture->state, bytes, len);
	fixture->state_len = len;
	fixture->state_present = true;
	return 0;
}

static int fixture_reset_state(void *user_data)
{
	struct vm_storage_fixture *fixture = user_data;

	fixture->state_len = 0;
	fixture->state_present = false;
	fixture->reset_called = true;
	return 0;
}

ZTEST(squidscript_protocol, test_vm_storage_adapter_completes_sqbc_and_state_requests)
{
	const uint8_t sqbc[] = {0x10, 0x20, 0x30, 0x40, 0x50};
	struct vm_storage_fixture fixture = {
		.sqbc = sqbc,
		.sqbc_len = sizeof(sqbc),
		.state = {0xaa, 0xbb, 0xcc},
		.state_len = 3,
		.state_present = true,
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	SqvmStorageCompletion completion = {0};
	SqvmStorageRequest request = {
		.kind = SQVM_STORAGE_REQUEST_SQBC_READ,
		.offset = 1,
		.len = 3,
	};

	zassert_equal(sq_vm_storage_complete_request(&backend, &request, &completion), 0);
	zassert_true(completion.has_len);
	zassert_equal(completion.len, 3);
	zassert_mem_equal(completion.bytes, &sqbc[1], 3);

	request = (SqvmStorageRequest){.kind = SQVM_STORAGE_REQUEST_STATE_LOAD};
	memset(&completion, 0, sizeof(completion));
	zassert_equal(sq_vm_storage_complete_request(&backend, &request, &completion), 0);
	zassert_true(completion.has_len);
	zassert_equal(completion.len, 3);
	zassert_mem_equal(completion.bytes, fixture.state, 3);

	request = (SqvmStorageRequest){.kind = SQVM_STORAGE_REQUEST_STATE_SAVE, .len = 2};
	request.bytes[0] = 0x7a;
	request.bytes[1] = 0x7b;
	memset(&completion, 0xff, sizeof(completion));
	zassert_equal(sq_vm_storage_complete_request(&backend, &request, &completion), 0);
	zassert_false(completion.has_len);
	zassert_equal(fixture.state_len, 2);
	zassert_mem_equal(fixture.state, request.bytes, 2);

	request = (SqvmStorageRequest){.kind = SQVM_STORAGE_REQUEST_STATE_RESET};
	zassert_equal(sq_vm_storage_complete_request(&backend, &request, &completion), 0);
	zassert_true(fixture.reset_called);
	zassert_false(fixture.state_present);
}

ZTEST(squidscript_protocol, test_vm_fs_storage_reads_sqbc_and_persists_state)
{
	const char *sqbc_path = "/sqtest/app.sqbc";
	const char *state_path = "/sqtest/app.state";
	const uint8_t sqbc[] = {0x10, 0x11, 0x12, 0x13, 0x14};
	const uint8_t saved_state[] = {0xa0, 0xa1, 0xa2, 0xa3};
	struct sq_vm_fs_storage storage = {
		.sqbc_path = sqbc_path,
		.state_path = state_path,
	};
	struct sq_vm_storage_backend backend = sq_vm_fs_storage_backend(&storage);
	SqvmStorageRequest request = {0};
	SqvmStorageCompletion completion = {0};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(unlink_test_file_if_exists(sqbc_path), 0);
	zassert_equal(unlink_test_file_if_exists(state_path), 0);
	zassert_equal(write_test_file(sqbc_path, sqbc, sizeof(sqbc)), 0);

	request = (SqvmStorageRequest){
		.kind = SQVM_STORAGE_REQUEST_SQBC_READ,
		.offset = 2,
		.len = 3,
	};
	zassert_equal(sq_vm_storage_complete_request(&backend, &request, &completion), 0);
	zassert_true(completion.has_len);
	zassert_equal(completion.len, 3);
	zassert_mem_equal(completion.bytes, &sqbc[2], 3);

	request = (SqvmStorageRequest){
		.kind = SQVM_STORAGE_REQUEST_STATE_SAVE,
		.len = sizeof(saved_state),
	};
	memcpy(request.bytes, saved_state, sizeof(saved_state));
	memset(&completion, 0, sizeof(completion));
	zassert_equal(sq_vm_storage_complete_request(&backend, &request, &completion), 0);
	zassert_false(completion.has_len);

	request = (SqvmStorageRequest){.kind = SQVM_STORAGE_REQUEST_STATE_LOAD};
	zassert_equal(sq_vm_storage_complete_request(&backend, &request, &completion), 0);
	zassert_true(completion.has_len);
	zassert_equal(completion.len, sizeof(saved_state));
	zassert_mem_equal(completion.bytes, saved_state, sizeof(saved_state));

	request = (SqvmStorageRequest){.kind = SQVM_STORAGE_REQUEST_STATE_RESET};
	zassert_equal(sq_vm_storage_complete_request(&backend, &request, &completion), 0);

	request = (SqvmStorageRequest){.kind = SQVM_STORAGE_REQUEST_STATE_LOAD};
	zassert_equal(sq_vm_storage_complete_request(&backend, &request, &completion), 0);
	zassert_false(completion.has_len);
	zassert_equal(completion.len, 0);

	zassert_equal(unlink_test_file_if_exists(sqbc_path), 0);
	zassert_equal(unlink_test_file_if_exists(state_path), 0);
	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_app_store_derives_vm_storage_paths_from_mount)
{
	struct sq_app_store_vm_storage app_storage = {0};
	struct sq_vm_storage_backend backend;
	struct fs_dirent entry;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_prepare_filesystem(test_fs_mount.mnt_point), 0);

	zassert_equal(fs_stat("/sqtest/apps", &entry), 0);
	zassert_equal(entry.type, FS_DIR_ENTRY_DIR);
	zassert_equal(fs_stat("/sqtest/state", &entry), 0);
	zassert_equal(entry.type, FS_DIR_ENTRY_DIR);

	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point,
						      "headless-counter", &app_storage),
		      0);
	zassert_str_equal(app_storage.sqbc_path, "/sqtest/apps/headless-counter/main.sqbc");
	zassert_str_equal(app_storage.state_path, "/sqtest/state/headless-counter.state");

	backend = sq_app_store_vm_storage_backend(&app_storage);
	zassert_not_null(backend.read_sqbc);
	zassert_not_null(backend.load_state);
	zassert_equal(backend.user_data, &app_storage.fs_storage);

	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point,
						      "../bad", &app_storage),
		      -EINVAL);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_installed_app_launch_reads_sqbc_in_bounded_file_chunks)
{
	enum { PADDED_SQBC_LEN = sizeof(headless_counter_sqbc) + SQVM_STORAGE_TRANSFER_CAPACITY };
	uint8_t padded_sqbc[PADDED_SQBC_LEN];
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_vm_storage_backend backend;
	static struct sq_vm_runtime runtime;

	memcpy(padded_sqbc, headless_counter_sqbc, sizeof(headless_counter_sqbc));
	memset(&padded_sqbc[sizeof(headless_counter_sqbc)], 0xa5,
	       sizeof(padded_sqbc) - sizeof(headless_counter_sqbc));
	padded_sqbc[6] = PADDED_SQBC_LEN & 0xff;
	padded_sqbc[7] = (PADDED_SQBC_LEN >> 8) & 0xff;
	padded_sqbc[8] = (PADDED_SQBC_LEN >> 16) & 0xff;
	padded_sqbc[9] = (PADDED_SQBC_LEN >> 24) & 0xff;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_prepare_filesystem(test_fs_mount.mnt_point), 0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "padded", padded_sqbc,
					       sizeof(padded_sqbc)),
		      0);
	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point, "padded",
						      &launch_storage),
		      0);
	backend = sq_app_store_vm_storage_backend(&launch_storage);

	memset(&runtime, 0, sizeof(runtime));
	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), 0);
	zassert_equal(runtime.result_code, 0);
	zassert_equal(runtime.trace_count, 3);
	zassert_true(launch_storage.fs_storage.sqbc_read_count > 0);
	zassert_true(launch_storage.fs_storage.sqbc_max_read_len <= SQVM_STORAGE_TRANSFER_CAPACITY);
	zassert_true(launch_storage.fs_storage.sqbc_total_read_len < sizeof(padded_sqbc));

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_app_store_installs_app_and_rebuilds_registry)
{
	const uint8_t sqbc_a[] = {0x53, 0x51, 0x42, 0x43, 0x01};
	const uint8_t sqbc_b[] = {0x53, 0x51, 0x42, 0x43, 0x02, 0x03};
	struct sq_app_registry registry = {0};
	struct fs_dirent entry;
	const struct sq_app_registry_entry *installed;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_prepare_filesystem(test_fs_mount.mnt_point), 0);

	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "alpha", sqbc_a,
					       sizeof(sqbc_a)),
		      0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "beta", sqbc_b,
					       sizeof(sqbc_b)),
		      0);

	zassert_equal(fs_stat("/sqtest/apps/alpha", &entry), 0);
	zassert_equal(entry.type, FS_DIR_ENTRY_DIR);
	zassert_equal(fs_stat("/sqtest/apps/alpha/main.sqbc", &entry), 0);
	zassert_equal(entry.type, FS_DIR_ENTRY_FILE);
	zassert_equal(entry.size, sizeof(sqbc_a));

	zassert_equal(sq_app_store_scan_registry(test_fs_mount.mnt_point, &registry), 0);
	zassert_true(registry.count >= 2);

	installed = sq_app_registry_find(&registry, "alpha");
	zassert_not_null(installed);
	zassert_str_equal(installed->app_id, "alpha");
	zassert_equal(installed->sqbc_len, sizeof(sqbc_a));

	installed = sq_app_registry_find(&registry, "beta");
	zassert_not_null(installed);
	zassert_str_equal(installed->app_id, "beta");
	zassert_equal(installed->sqbc_len, sizeof(sqbc_b));

	zassert_is_null(sq_app_registry_find(&registry, "../bad"));
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "../bad", sqbc_a,
					       sizeof(sqbc_a)),
		      -EINVAL);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_app_store_installs_and_resolves_package_resources)
{
	const uint8_t sqbc[] = {0x53, 0x51, 0x42, 0x43};
	const uint8_t resource[] = {0xde, 0xad, 0xbe, 0xef};
	char resource_path[SQ_APP_STORE_PATH_MAX];
	struct fs_dirent entry;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "resource-app", sqbc,
					       sizeof(sqbc)),
		      0);
	zassert_equal(sq_app_store_install_resource(test_fs_mount.mnt_point, "resource-app",
						    "icons/main.bin", resource,
						    sizeof(resource)),
		      0);
	zassert_equal(sq_app_store_resource_path(test_fs_mount.mnt_point, "resource-app",
						 "icons/main.bin", resource_path,
						 sizeof(resource_path)),
		      0);
	zassert_str_equal(resource_path, "/sqtest/apps/resource-app/resources/icons/main.bin");
	zassert_equal(fs_stat(resource_path, &entry), 0);
	zassert_equal(entry.type, FS_DIR_ENTRY_FILE);
	zassert_equal(entry.size, sizeof(resource));

	zassert_equal(sq_app_store_install_resource(test_fs_mount.mnt_point, "resource-app",
						    "../escape.bin", resource,
						    sizeof(resource)),
		      -EINVAL);
	zassert_equal(sq_app_store_resource_path(test_fs_mount.mnt_point, "resource-app",
						 "/absolute.bin", resource_path,
						 sizeof(resource_path)),
		      -EINVAL);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_vm_runtime_loads_package_sqdevice_resource_into_draft)
{
	const uint8_t sqbc[] = {0x53, 0x51, 0x42, 0x43};
	const uint8_t sqdevice[] = "SQDEVICE\n"
				   "service string 17:indicator.default\n"
				   "mode string 4:gpio\n"
				   "activeLow bool true\n";
	struct sq_vm_runtime runtime = {0};
	SqvmDeviceConfigResult result = {0};
	SqvmDeviceConfigValue value = {
		.kind = SQVM_DEVICE_CONFIG_VALUE_STRING,
		.string = (const uint8_t *)"GPIO8",
		.string_len = 5,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "device-config-app", sqbc,
					       sizeof(sqbc)),
		      0);
	zassert_equal(sq_app_store_install_resource(test_fs_mount.mnt_point, "device-config-app",
						    "device/indicator.sqdevice", sqdevice,
						    sizeof(sqdevice) - 1),
		      0);

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_set_store_mount_point(&runtime, test_fs_mount.mnt_point);
	strncpy(runtime.current_app, "device-config-app", sizeof(runtime.current_app) - 1);

	zassert_equal(sq_vm_runtime_device_config_load(
			      &runtime, (const uint8_t *)"package:device/indicator.sqdevice",
			      strlen("package:device/indicator.sqdevice"), &result),
		      0);
	zassert_true(result.ok);
	zassert_true(runtime.device_config_draft_loaded);
	zassert_equal(runtime.device_config_draft.count, 3);

	memset(&result, 0, sizeof(result));
	zassert_equal(sq_vm_runtime_device_config_set(&runtime, (const uint8_t *)"pinName",
						      strlen("pinName"), value, &result),
		      0);
	zassert_true(result.ok);
	zassert_equal(runtime.device_config_draft.records[3].value.kind, SQDC_VALUE_STRING);
	zassert_mem_equal(runtime.device_config_draft.records[3].value.string, "GPIO8", 5);

	memset(&result, 0, sizeof(result));
	zassert_equal(sq_vm_runtime_device_config_rebind(
			      &runtime, (const uint8_t *)"indicator.default",
			      strlen("indicator.default"), &result),
		      0);
	zassert_true(result.ok);
	zassert_true(runtime.indicator_binding_active);
	zassert_equal(runtime.indicator_binding_pin, 8);
	zassert_true(runtime.indicator_binding_active_low);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_vm_runtime_rejects_target_unknown_gpio_binding)
{
	struct sq_vm_runtime runtime = {0};
	SqvmDeviceConfigResult result = {0};
	SqvmDeviceConfigValue value = {
		.kind = SQVM_DEVICE_CONFIG_VALUE_STRING,
		.string = (const uint8_t *)"GPIO18",
		.string_len = strlen("GPIO18"),
	};

	sq_vm_runtime_init(&runtime);
	zassert_equal(sqdc_config_clear(&runtime.device_config_draft), SQDC_STATUS_OK);
	runtime.device_config_draft_loaded = true;
	zassert_equal(sqdc_config_set_string(&runtime.device_config_draft, (const uint8_t *)"service",
					     strlen("service"),
					     (const uint8_t *)"indicator.default",
					     strlen("indicator.default")),
		      SQDC_STATUS_OK);
	zassert_equal(sqdc_config_set_string(&runtime.device_config_draft, (const uint8_t *)"mode",
					     strlen("mode"), (const uint8_t *)"gpio",
					     strlen("gpio")),
		      SQDC_STATUS_OK);
	zassert_equal(sq_vm_runtime_device_config_set(&runtime, (const uint8_t *)"pinName",
						      strlen("pinName"), value, &result),
		      0);
	zassert_true(result.ok);
	memset(&result, 0, sizeof(result));
	zassert_equal(sqdc_config_set_bool(&runtime.device_config_draft,
					   (const uint8_t *)"activeLow", strlen("activeLow"),
					   false),
		      SQDC_STATUS_OK);

	zassert_equal(sq_vm_runtime_device_config_rebind(
			      &runtime, (const uint8_t *)"indicator.default",
			      strlen("indicator.default"), &result),
		      0);
	zassert_false(result.ok);
	zassert_mem_equal(result.error, "unsupported target gpio",
			  strlen("unsupported target gpio"));
	zassert_false(runtime.indicator_binding_active);
}

ZTEST(squidscript_protocol, test_vm_runtime_rebinds_display_device_config)
{
	struct sq_vm_runtime runtime = {0};
	SqvmDeviceConfigResult result = {0};

	sq_vm_runtime_init(&runtime);
	zassert_equal(sqdc_config_clear(&runtime.device_config_draft), SQDC_STATUS_OK);
	runtime.device_config_draft_loaded = true;
	zassert_equal(sqdc_config_set_string(&runtime.device_config_draft, (const uint8_t *)"service",
					     strlen("service"),
					     (const uint8_t *)"display.status",
					     strlen("display.status")),
		      SQDC_STATUS_OK);
	zassert_equal(sqdc_config_set_string(&runtime.device_config_draft, (const uint8_t *)"mode",
					     strlen("mode"), (const uint8_t *)"drawlog",
					     strlen("drawlog")),
		      SQDC_STATUS_OK);

	zassert_equal(sq_vm_runtime_device_config_rebind(
			      &runtime, (const uint8_t *)"display.status",
			      strlen("display.status"), &result),
		      0);
	zassert_true(result.ok);
	zassert_equal(runtime.active_binding_count, 1);
	zassert_true(runtime.active_bindings[0].active);
	zassert_str_equal(runtime.active_bindings[0].alias, "display.status");
}

ZTEST(squidscript_protocol, test_vm_runtime_resets_target_indicator_default_as_device_config)
{
	struct sq_vm_runtime runtime = {0};

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_reset(&runtime);

#if SQ_TARGET_INDICATOR_DEFAULT_HAS_GPIO
	zassert_true(runtime.device_config_draft_loaded);
	zassert_equal(runtime.device_config_draft.count, 4);
	zassert_true(runtime.indicator_binding_active);
	zassert_equal(runtime.indicator_binding_pin, SQ_TARGET_INDICATOR_DEFAULT_GPIO_PIN);
	zassert_equal(runtime.indicator_binding_active_low,
		      SQ_TARGET_INDICATOR_DEFAULT_ACTIVE_LOW != 0);
#else
	zassert_false(runtime.device_config_draft_loaded);
	zassert_false(runtime.indicator_binding_active);
#endif
}

ZTEST(squidscript_protocol, test_vm_runtime_rebuilds_indicator_default_on_app_start)
{
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage storage = {0};
	struct sq_vm_storage_backend backend;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "default-indicator-app",
					       headless_counter_sqbc,
					       sizeof(headless_counter_sqbc)),
		      0);
	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point,
						      "default-indicator-app", &storage),
		      0);
	backend = sq_app_store_vm_storage_backend(&storage);

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_set_store_mount_point(&runtime, test_fs_mount.mnt_point);
	strncpy(runtime.current_app, "default-indicator-app", sizeof(runtime.current_app) - 1);
	runtime.indicator_binding_active = true;
	runtime.indicator_binding_pin = 10;
	runtime.indicator_binding_active_low = false;

	zassert_equal(sq_vm_runtime_start(&runtime, &backend, "app.start"), 0);
	wait_runtime_done(&runtime);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);
	zassert_equal(runtime.result_code, 0);

#if SQ_TARGET_INDICATOR_DEFAULT_HAS_GPIO
	zassert_true(runtime.indicator_binding_active);
	zassert_equal(runtime.indicator_binding_pin, SQ_TARGET_INDICATOR_DEFAULT_GPIO_PIN);
	zassert_equal(runtime.indicator_binding_active_low,
		      SQ_TARGET_INDICATOR_DEFAULT_ACTIVE_LOW != 0);
#else
	zassert_false(runtime.indicator_binding_active);
#endif

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_vm_runtime_saves_device_config_draft_to_flash_sqdc)
{
	const uint8_t sqbc[] = {0x53, 0x51, 0x42, 0x43};
	const uint8_t sqdevice[] = "SQDEVICE\n"
				   "service string 17:indicator.default\n"
				   "mode string 4:gpio\n"
				   "pinName string 5:GPIO8\n"
				   "activeLow bool true\n";
	struct sq_vm_runtime runtime = {0};
	SqvmDeviceConfigResult result = {0};
	uint8_t saved[256];
	size_t saved_len = 0;
	SqdcConfig decoded = {0};
	char path[SQ_APP_STORE_PATH_MAX];

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "device-save-app", sqbc,
					       sizeof(sqbc)),
		      0);
	zassert_equal(sq_app_store_install_resource(test_fs_mount.mnt_point, "device-save-app",
						    "device/indicator.sqdevice", sqdevice,
						    sizeof(sqdevice) - 1),
		      0);

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_set_store_mount_point(&runtime, test_fs_mount.mnt_point);
	strncpy(runtime.current_app, "device-save-app", sizeof(runtime.current_app) - 1);

	zassert_equal(sq_vm_runtime_device_config_load(
			      &runtime, (const uint8_t *)"package:device/indicator.sqdevice",
			      strlen("package:device/indicator.sqdevice"), &result),
		      0);
	zassert_true(result.ok);

	memset(&result, 0, sizeof(result));
	zassert_equal(sq_vm_runtime_device_config_save(&runtime, (const uint8_t *)"flash",
						       strlen("flash"), &result),
		      0);
	zassert_true(result.ok);

	zassert_equal(sq_app_store_device_config_path(test_fs_mount.mnt_point, path, sizeof(path)),
		      0);
	zassert_equal(read_test_file(path, saved, sizeof(saved), &saved_len), 0);
	zassert_true(saved_len > 0);
	zassert_equal(sqdc_decode_sqdc(saved, saved_len, &decoded), SQDC_STATUS_OK);
	zassert_equal(decoded.count, 4);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_vm_runtime_applies_saved_device_config_before_app_start)
{
	const uint8_t sqdevice[] = "SQDEVICE\n"
				   "service string 17:indicator.default\n"
				   "mode string 4:gpio\n"
				   "pinName string 5:GPIO8\n"
				   "activeLow bool true\n";
	struct sq_vm_runtime save_runtime = {0};
	struct sq_vm_runtime launch_runtime = {0};
	struct sq_app_store_vm_storage storage = {0};
	struct sq_vm_storage_backend backend;
	SqvmDeviceConfigResult result = {0};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "saved-default-app",
					       headless_counter_sqbc,
					       sizeof(headless_counter_sqbc)),
		      0);
	zassert_equal(sq_app_store_install_resource(test_fs_mount.mnt_point, "saved-default-app",
						    "device/indicator.sqdevice", sqdevice,
						    sizeof(sqdevice) - 1),
		      0);
	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point,
						      "saved-default-app", &storage),
		      0);
	backend = sq_app_store_vm_storage_backend(&storage);

	sq_vm_runtime_init(&save_runtime);
	sq_vm_runtime_set_store_mount_point(&save_runtime, test_fs_mount.mnt_point);
	strncpy(save_runtime.current_app, "saved-default-app", sizeof(save_runtime.current_app) - 1);
	zassert_equal(sq_vm_runtime_device_config_load(
			      &save_runtime, (const uint8_t *)"package:device/indicator.sqdevice",
			      strlen("package:device/indicator.sqdevice"), &result),
		      0);
	zassert_true(result.ok);
	memset(&result, 0, sizeof(result));
	zassert_equal(sq_vm_runtime_device_config_save(&save_runtime, (const uint8_t *)"flash",
						       strlen("flash"), &result),
		      0);
	zassert_true(result.ok);

	sq_vm_runtime_init(&launch_runtime);
	sq_vm_runtime_set_store_mount_point(&launch_runtime, test_fs_mount.mnt_point);
	strncpy(launch_runtime.current_app, "saved-default-app",
		sizeof(launch_runtime.current_app) - 1);
	zassert_equal(sq_vm_runtime_start(&launch_runtime, &backend, "app.start"), 0);
	wait_runtime_done(&launch_runtime);
	zassert_equal(launch_runtime.status, SQ_VM_RUNTIME_COMPLETE);
	zassert_equal(launch_runtime.result_code, 0);
	zassert_true(launch_runtime.indicator_binding_active);
	zassert_equal(launch_runtime.indicator_binding_pin, 8);
	zassert_true(launch_runtime.indicator_binding_active_low);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_vm_runtime_applies_packaged_device_binding_before_app_start)
{
	const uint8_t sqdevice[] = "SQDEVICE\n"
				   "service string 17:indicator.default\n"
				   "mode string 4:gpio\n"
				   "pinName string 5:GPIO8\n"
				   "activeLow bool false\n";
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage storage = {0};
	struct sq_vm_storage_backend backend;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "device-binding-app",
					       device_binding_app_sqbc,
					       sizeof(device_binding_app_sqbc)),
		      0);
	zassert_equal(sq_app_store_install_resource(test_fs_mount.mnt_point, "device-binding-app",
						    "device/indicator.sqdevice", sqdevice,
						    sizeof(sqdevice) - 1),
		      0);
	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point, "device-binding-app",
						      &storage),
		      0);
	backend = sq_app_store_vm_storage_backend(&storage);

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_set_store_mount_point(&runtime, test_fs_mount.mnt_point);
	strncpy(runtime.current_app, "device-binding-app", sizeof(runtime.current_app) - 1);

	zassert_equal(sq_vm_runtime_start(&runtime, &backend, "app.start"), 0);
	wait_runtime_done(&runtime);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);
	zassert_equal(runtime.result_code, 0);
	zassert_true(runtime.indicator_binding_active);
	zassert_equal(runtime.indicator_binding_pin, 8);
	zassert_false(runtime.indicator_binding_active_low);
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "binding ready");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_vm_runtime_applies_packaged_display_binding_before_app_start)
{
	const uint8_t sqdevice[] = "SQDEVICE\n"
				   "service string 14:display.status\n"
				   "mode string 7:drawlog\n";
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage storage = {0};
	struct sq_vm_storage_backend backend;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "display-binding-app",
					       display_binding_app_sqbc,
					       sizeof(display_binding_app_sqbc)),
		      0);
	zassert_equal(sq_app_store_install_resource(test_fs_mount.mnt_point, "display-binding-app",
						    "device/status-display.sqdevice", sqdevice,
						    sizeof(sqdevice) - 1),
		      0);
	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point,
						      "display-binding-app", &storage),
		      0);
	backend = sq_app_store_vm_storage_backend(&storage);

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_set_store_mount_point(&runtime, test_fs_mount.mnt_point);
	strncpy(runtime.current_app, "display-binding-app", sizeof(runtime.current_app) - 1);

	zassert_equal(sq_vm_runtime_start(&runtime, &backend, "app.start"), 0);
	wait_runtime_done(&runtime);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);
	zassert_equal(runtime.result_code, 0);
	zassert_true(runtime_has_active_binding(&runtime, "indicator.default"));
	zassert_true(runtime_has_active_binding(&runtime, "display.status"));
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "display binding ready");
	zassert_equal(runtime.drawlog_count, 1);
	zassert_str_equal(runtime.drawlog[0], "draw=select name=status");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_vm_runtime_applies_inline_gpio_device_binding_before_app_start)
{
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage storage = {0};
	struct sq_vm_storage_backend backend;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "inline-gpio-binding-app",
					       inline_gpio_binding_app_sqbc,
					       sizeof(inline_gpio_binding_app_sqbc)),
		      0);
	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point,
						      "inline-gpio-binding-app", &storage),
		      0);
	backend = sq_app_store_vm_storage_backend(&storage);

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_set_store_mount_point(&runtime, test_fs_mount.mnt_point);
	strncpy(runtime.current_app, "inline-gpio-binding-app", sizeof(runtime.current_app) - 1);

	zassert_equal(sq_vm_runtime_start(&runtime, &backend, "app.start"), 0);
	wait_runtime_done(&runtime);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);
	zassert_equal(runtime.result_code, 0);
	zassert_true(runtime.indicator_binding_active);
	zassert_equal(runtime.indicator_binding_pin, 8);
	zassert_false(runtime.indicator_binding_active_low);
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "inline binding ready");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_vm_runtime_rejects_unsupported_packaged_gpio_as_unsupported)
{
	const uint8_t sqdevice[] = "SQDEVICE\n"
				   "service string 17:indicator.default\n"
				   "mode string 4:gpio\n"
				   "pinName string 6:GPIO18\n"
				   "activeLow bool false\n";
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage storage = {0};
	struct sq_vm_storage_backend backend;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "unsupported-gpio-binding-app",
					       device_binding_app_sqbc,
					       sizeof(device_binding_app_sqbc)),
		      0);
	zassert_equal(sq_app_store_install_resource(test_fs_mount.mnt_point,
						    "unsupported-gpio-binding-app",
						    "device/indicator.sqdevice", sqdevice,
						    sizeof(sqdevice) - 1),
		      0);
	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point,
						      "unsupported-gpio-binding-app", &storage),
		      0);
	backend = sq_app_store_vm_storage_backend(&storage);

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_set_store_mount_point(&runtime, test_fs_mount.mnt_point);
	strncpy(runtime.current_app, "unsupported-gpio-binding-app", sizeof(runtime.current_app) - 1);

	zassert_equal(sq_vm_runtime_start(&runtime, &backend, "app.start"), -ENOTSUP);
	zassert_true(runtime.indicator_binding_active);
	zassert_equal(runtime.indicator_binding_pin, 8);
	zassert_equal(runtime.output_count, 0);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_zephyr_calls_squidvm_ffi_with_storage_adapter)
{
	struct ffi_vm_fixture fixture = {
		.storage = {
			.sqbc = headless_counter_sqbc,
			.sqbc_len = sizeof(headless_counter_sqbc),
		},
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture.storage,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	SqvmCallbacks callbacks = {
		.user_data = &fixture,
		.trace = ffi_trace,
		.read_exact_at = ffi_read_exact_at,
	};
	SqvmDispatchResult result = {0};
	SqvmStorageCompletion completion = {0};
	zassert_true(sqvm_context_size() <= sizeof(ffi_context_storage));
	zassert_equal(sqvm_context_prepare(ffi_context_storage, sizeof(ffi_context_storage)),
		      SQVM_STATUS_OK);
	zassert_equal(sqvm_context_init_in_place(ffi_context_storage, callbacks, ffi_scratch,
						 sizeof(ffi_scratch)),
		      SQVM_STATUS_OK);

	zassert_equal(sqvm_dispatch_start_resumable(ffi_context_storage, callbacks,
						    (const uint8_t *)"app.start", 9, &result),
		      SQVM_STATUS_OK);

	while (result.outcome == SQVM_DISPATCH_PENDING_STORAGE) {
		zassert_equal(sq_vm_storage_complete_request(&backend, &result.storage, &completion),
			      0);
		zassert_equal(sqvm_dispatch_resume_storage(ffi_context_storage, callbacks,
							   &completion, &result),
			      SQVM_STATUS_OK);
	}

	zassert_equal(result.outcome, SQVM_DISPATCH_COMPLETE);
	zassert_equal(fixture.trace_count, 3);
	zassert_str_equal(fixture.traces[0], "app.start");
	zassert_str_equal(fixture.traces[1], "state.load");
	zassert_str_equal(fixture.traces[2], "state.save");
	zassert_true(fixture.storage.state_present);
	zassert_true(fixture.storage.state_len > 0);
}

ZTEST(squidscript_protocol, test_vm_runtime_dispatches_app_start_and_records_trace)
{
	struct vm_storage_fixture fixture = {
		.sqbc = headless_counter_sqbc,
		.sqbc_len = sizeof(headless_counter_sqbc),
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	struct sq_vm_runtime runtime = {0};

	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), 0);
	zassert_equal(runtime.trace_count, 3);
	zassert_str_equal(runtime.traces[0], "app.start");
	zassert_str_equal(runtime.traces[1], "state.load");
	zassert_str_equal(runtime.traces[2], "state.save");
}

ZTEST(squidscript_protocol, test_vm_runtime_dispatches_system_resource_callbacks)
{
	struct vm_storage_fixture fixture = {
		.sqbc = system_resources_sqbc,
		.sqbc_len = sizeof(system_resources_sqbc),
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	static struct sq_vm_runtime runtime;

	memset(&runtime, 0, sizeof(runtime));
	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), -EIO);
	zassert_equal(runtime.output_count, 1);
	zassert_true(strncmp(runtime.outputs[0], "system memory RAM ", strlen("system memory RAM ")) ==
			     0,
		     "memory output was %s", runtime.outputs[0]);

	memset(&runtime, 0, sizeof(runtime));
	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_prepare_filesystem(test_fs_mount.mnt_point), 0);
	sq_vm_runtime_set_store_mount_point(&runtime, test_fs_mount.mnt_point);
	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), 0);
	zassert_equal(runtime.output_count, 2);
	zassert_true(strncmp(runtime.outputs[0], "system memory RAM ", strlen("system memory RAM ")) ==
			     0,
		     "memory output was %s", runtime.outputs[0]);
	zassert_true(strncmp(runtime.outputs[1], "system apps Apps ", strlen("system apps Apps ")) ==
			     0,
		     "storage output was %s", runtime.outputs[1]);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_vm_runtime_dispatches_app_registry_callbacks)
{
	struct vm_storage_fixture fixture = {
		.sqbc = app_registry_summary_sqbc,
		.sqbc_len = sizeof(app_registry_summary_sqbc),
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	struct sq_app_registry registry = {
		.count = 2,
		.apps = {
			{.app_id = "alpha", .sqbc_len = 5},
			{.app_id = "beta", .sqbc_len = 6},
		},
	};
	static struct sq_vm_runtime runtime;

	memset(&runtime, 0, sizeof(runtime));
	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), -EIO);
	zassert_equal(runtime.output_count, 0);

	memset(&runtime, 0, sizeof(runtime));
	sq_vm_runtime_set_registry(&runtime, &registry);
	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), 0);
	zassert_equal(runtime.output_count, 3);
	zassert_str_equal(runtime.outputs[0], "registry app alpha");
	zassert_str_equal(runtime.outputs[1], "registry app beta");
	zassert_str_equal(runtime.outputs[2], "registry selected alpha alpha  ");
}

ZTEST(squidscript_protocol, test_vm_runtime_dispatches_stack_inspection_callbacks)
{
	struct vm_storage_fixture fixture = {
		.sqbc = stack_inspect_sqbc,
		.sqbc_len = sizeof(stack_inspect_sqbc),
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	static struct sq_vm_runtime runtime;

	memset(&runtime, 0, sizeof(runtime));
	strcpy(runtime.return_stack[0], "launcher");
	strcpy(runtime.return_stack[1], "parent");
	runtime.return_stack_count = 2;
	runtime.armed_timers[0] = (struct sq_vm_runtime_armed_timer){
		.active = true,
		.app_id = "break-reminder",
		.event = "timer.break",
	};
	runtime.armed_timers[1] = (struct sq_vm_runtime_armed_timer){
		.active = true,
		.app_id = "reader-clock",
		.event = "timer.clock",
	};
	runtime.armed_timer_count = 2;

	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), 0);
	zassert_equal(runtime.output_count, 5);
	zassert_str_equal(runtime.outputs[0], "process launcher");
	zassert_str_equal(runtime.outputs[1], "process parent");
	zassert_str_equal(runtime.outputs[2], "armed break-reminder timer.break");
	zassert_str_equal(runtime.outputs[3], "armed reader-clock timer.clock");
	zassert_str_equal(runtime.outputs[4], "selected reader-clock timer.clock");
}

ZTEST(squidscript_protocol, test_vm_runtime_dispatches_display_drawlog_callbacks)
{
	struct vm_storage_fixture fixture = {
		.sqbc = display_drawlog_sqbc,
		.sqbc_len = sizeof(display_drawlog_sqbc),
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	static struct sq_vm_runtime runtime;

	memset(&runtime, 0, sizeof(runtime));
	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), 0);
	zassert_equal(runtime.drawlog_count, 4);
	zassert_str_equal(runtime.drawlog[0], "draw=clear color=gray0");
	zassert_str_equal(runtime.drawlog[1], "draw=select name=status");
	zassert_str_equal(runtime.drawlog[2], "draw=image path=\"data/icon.bmp\" x=20 y=24");
	zassert_str_equal(runtime.drawlog[3],
			  "draw=resource drawable=\"drawable/page\" x=0 y=0");

	fixture.sqbc = display_primitives_sqbc;
	fixture.sqbc_len = sizeof(display_primitives_sqbc);
	memset(&runtime, 0, sizeof(runtime));
	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), 0);
	zassert_equal(runtime.drawlog_count, 4);
	zassert_str_equal(runtime.drawlog[0], "draw=clear color=gray0");
	zassert_str_equal(runtime.drawlog[1], "draw=text text=\"Hello\" x=10 y=20");
	zassert_str_equal(runtime.drawlog[2], "draw=rect x=1 y=2 w=3 h=4");
	zassert_str_equal(runtime.drawlog[3], "draw=line x1=5 y1=6 x2=7 y2=8");
}

ZTEST(squidscript_protocol, test_vm_runtime_dispatches_wifi_action_stubs)
{
	struct vm_storage_fixture fixture = {
		.sqbc = wifi_actions_sqbc,
		.sqbc_len = sizeof(wifi_actions_sqbc),
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	static struct sq_vm_runtime runtime;

	memset(&runtime, 0, sizeof(runtime));
	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), 0);
	zassert_equal(runtime.output_count, 3);
	zassert_str_equal(runtime.outputs[0], "false unsupported");
	zassert_str_equal(runtime.outputs[1], "unsupported");
	zassert_str_equal(runtime.outputs[2],
			  "false unsupported false unsupported false unsupported");
}

ZTEST(squidscript_protocol, test_vm_runtime_dispatches_content_pick_file_unsupported_result)
{
	struct vm_storage_fixture fixture = {
		.sqbc = content_pick_file_sqbc,
		.sqbc_len = sizeof(content_pick_file_sqbc),
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	static struct sq_vm_runtime runtime;

	memset(&runtime, 0, sizeof(runtime));
	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), 0);
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "false unsupported null");
}

ZTEST(squidscript_protocol, test_vm_runtime_dispatches_content_read_unsupported_results)
{
	struct vm_storage_fixture fixture = {
		.sqbc = content_read_sqbc,
		.sqbc_len = sizeof(content_read_sqbc),
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	static struct sq_vm_runtime runtime;

	memset(&runtime, 0, sizeof(runtime));
	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), 0);
	zassert_equal(runtime.output_count, 2);
	zassert_str_equal(runtime.outputs[0], "false unsupported null");
	zassert_str_equal(runtime.outputs[1], "false unsupported <list>");
}

ZTEST(squidscript_protocol, test_vm_runtime_formats_wifi_bssid_without_heap)
{
	const uint8_t mac[] = {0x02, 0x34, 0xab, 0xcd, 0xef, 0x10};
	char bssid[SQ_VM_RUNTIME_WIFI_BSSID_LEN];

	zassert_equal(sq_vm_runtime_wifi_format_bssid(mac, sizeof(mac), bssid, sizeof(bssid)), 0);
	zassert_str_equal(bssid, "02:34:ab:cd:ef:10");
	zassert_equal(sq_vm_runtime_wifi_format_bssid(mac, 5, bssid, sizeof(bssid)), -EINVAL);
	zassert_equal(sq_vm_runtime_wifi_format_bssid(mac, sizeof(mac), bssid, 17), -ENOSPC);
}

ZTEST(squidscript_protocol, test_sqdc_ffi_parses_and_encodes_device_config)
{
	const uint8_t source[] =
		"SQDEVICE\n"
		"service string 17:indicator.default\n"
		"backend string 4:gpio\n"
		"activeLow bool false\n"
		"pin int 8\n";
	SqdcConfig config = {0};
	SqdcConfig decoded = {0};
	uint8_t encoded[256];
	size_t encoded_len = 0;

	zassert_equal(sqdc_parse_sqdevice(source, strlen((const char *)source), &config),
		      SQDC_STATUS_OK);
	zassert_equal(config.count, 4);
	zassert_equal(sqdc_config_set_string(&config, (const uint8_t *)"pinName",
					     strlen("pinName"), (const uint8_t *)"GPIO8",
					     strlen("GPIO8")),
		      SQDC_STATUS_OK);
	zassert_equal(sqdc_config_set_bool(&config, (const uint8_t *)"activeLow",
					   strlen("activeLow"), true),
		      SQDC_STATUS_OK);
	zassert_equal(sqdc_encode_sqdc(&config, encoded, sizeof(encoded), &encoded_len),
		      SQDC_STATUS_OK);
	zassert_mem_equal(encoded, "SQDC", 4);
	zassert_equal(sqdc_decode_sqdc(encoded, encoded_len, &decoded), SQDC_STATUS_OK);
	zassert_equal(decoded.count, config.count);
	zassert_equal(sqdc_is_safe_sqdevice_path((const uint8_t *)"device/indicator.sqdevice",
						 strlen("device/indicator.sqdevice")),
		      SQDC_STATUS_OK);
	zassert_equal(sqdc_is_safe_sqdevice_path((const uint8_t *)"../indicator.sqdevice",
						 strlen("../indicator.sqdevice")),
		      SQDC_STATUS_INVALID_ARGUMENT);
}

ZTEST(squidscript_protocol, test_sqdc_ffi_plans_device_binding_resources)
{
	SqdcDeviceBindingPlan plan = {0};
	SqdcConfig inline_config = {0};

	zassert_equal(sqdc_plan_device_binding((const uint8_t *)"indicator",
					       strlen("indicator"), (const uint8_t *)"default",
					       strlen("default"), (const uint8_t *)"gpio:GPIO8",
					       strlen("gpio:GPIO8"), &plan, &inline_config),
		      SQDC_STATUS_OK);
	zassert_equal(plan.kind, SQDC_DEVICE_BINDING_RESOURCE_INLINE_GPIO);
	zassert_equal(plan.alias_len, strlen("indicator.default"));
	zassert_mem_equal(plan.alias, "indicator.default", strlen("indicator.default"));
	zassert_equal(inline_config.count, 4);
	zassert_equal(inline_config.records[2].value.kind, SQDC_VALUE_STRING);
	zassert_mem_equal(inline_config.records[2].value.string, "GPIO8", strlen("GPIO8"));
	zassert_false(inline_config.records[3].value.bool_value);

	memset(&plan, 0, sizeof(plan));
	memset(&inline_config, 0, sizeof(inline_config));
	zassert_equal(sqdc_plan_device_binding((const uint8_t *)"indicator",
					       strlen("indicator"), (const uint8_t *)"default",
					       strlen("default"),
					       (const uint8_t *)"device/indicator.sqdevice",
					       strlen("device/indicator.sqdevice"), &plan,
					       &inline_config),
		      SQDC_STATUS_OK);
	zassert_equal(plan.kind, SQDC_DEVICE_BINDING_RESOURCE_PACKAGE_SQDEVICE);
	zassert_equal(plan.resource_len, strlen("device/indicator.sqdevice"));
	zassert_mem_equal(plan.resource, "device/indicator.sqdevice",
			  strlen("device/indicator.sqdevice"));
	zassert_equal(inline_config.count, 0);

	memset(&plan, 0, sizeof(plan));
	memset(&inline_config, 0, sizeof(inline_config));
	zassert_equal(sqdc_plan_device_binding((const uint8_t *)"display", strlen("display"),
					       (const uint8_t *)"status", strlen("status"),
					       (const uint8_t *)"device/display.sqdevice",
					       strlen("device/display.sqdevice"), &plan,
					       &inline_config),
		      SQDC_STATUS_OK);
	zassert_equal(plan.kind, SQDC_DEVICE_BINDING_RESOURCE_PACKAGE_SQDEVICE);
	zassert_equal(plan.alias_len, strlen("display.status"));
	zassert_mem_equal(plan.alias, "display.status", strlen("display.status"));
	zassert_equal(inline_config.count, 0);

	zassert_equal(sqdc_plan_device_binding((const uint8_t *)"sensor", strlen("sensor"),
					       (const uint8_t *)"default", strlen("default"),
					       (const uint8_t *)"device/sensor.sqdevice",
					       strlen("device/sensor.sqdevice"), &plan,
					       &inline_config),
		      SQDC_STATUS_INVALID_ARGUMENT);
}

ZTEST(squidscript_protocol, test_vm_runtime_tracks_output_indicator_and_due_timers)
{
	struct sq_vm_runtime runtime = {0};
	char event[SQ_VM_RUNTIME_EVENT_LEN];

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_reset(&runtime);

	zassert_equal(sq_vm_runtime_record_output(&runtime, (const uint8_t *)"hello", 5), 0);
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "hello");

	zassert_equal(sq_vm_runtime_indicator_write(&runtime, true), 0);
	bool value = false;
	zassert_equal(sq_vm_runtime_indicator_read(&runtime, &value), 0);
	zassert_true(value);
	zassert_equal(sq_vm_runtime_indicator_toggle(&runtime), 0);
	zassert_equal(sq_vm_runtime_indicator_read(&runtime, &value), 0);
	zassert_false(value);

	zassert_equal(sq_vm_runtime_indicator_breathe(&runtime), 0);
	zassert_true(runtime.indicator_breathe_active);
	uint8_t first_step = runtime.indicator_breathe_step;
	runtime.indicator_breathe_next_ms = k_uptime_get() - 1;
	zassert_equal(sq_vm_runtime_poll(&runtime), 0);
	zassert_true(runtime.indicator_breathe_active);
	zassert_not_equal(runtime.indicator_breathe_step, first_step);

	zassert_equal(sq_vm_runtime_indicator_blink(&runtime, 10, 20), 0);
	zassert_true(runtime.indicator_blink_active);
	zassert_false(runtime.indicator_breathe_active);
	zassert_true(runtime.indicator_blink_on);
	zassert_true(runtime.indicator_state);
	zassert_equal(runtime.indicator_blink_on_ms, 10);
	zassert_equal(runtime.indicator_blink_off_ms, 20);
	runtime.indicator_blink_next_ms = k_uptime_get() - 1;
	zassert_equal(sq_vm_runtime_poll(&runtime), 0);
	zassert_true(runtime.indicator_blink_active);
	zassert_false(runtime.indicator_blink_on);
	zassert_false(runtime.indicator_state);
	runtime.indicator_blink_next_ms = k_uptime_get() - 1;
	zassert_equal(sq_vm_runtime_poll(&runtime), 0);
	zassert_true(runtime.indicator_blink_on);
	zassert_true(runtime.indicator_state);

	zassert_equal(sq_vm_runtime_indicator_write(&runtime, true), 0);
	zassert_false(runtime.indicator_breathe_active);
	zassert_false(runtime.indicator_blink_active);

	zassert_equal(sq_vm_runtime_indicator_breathe(&runtime), 0);
	zassert_true(runtime.indicator_breathe_active);
	zassert_false(runtime.indicator_blink_active);
	zassert_equal(sq_vm_runtime_hardware_gpio_write(&runtime, (const uint8_t *)"GPIO8",
							strlen("GPIO8"), true),
		      0);
	zassert_equal(sq_vm_runtime_hardware_gpio_read(&runtime, (const uint8_t *)"GPIO8",
						       strlen("GPIO8"), &value),
		      0);
	zassert_true(value);
	zassert_equal(sq_vm_runtime_indicator_breathe(&runtime), 0);
	zassert_true(runtime.indicator_breathe_active);
	zassert_equal(sq_vm_runtime_hardware_gpio_toggle(&runtime, (const uint8_t *)"GPIO8",
							 strlen("GPIO8")),
		      0);
	zassert_equal(sq_vm_runtime_hardware_gpio_read(&runtime, (const uint8_t *)"GPIO8",
						       strlen("GPIO8"), &value),
		      0);
	zassert_false(value);

	zassert_equal(sq_vm_runtime_register_timer(&runtime, (const uint8_t *)"timer.debug",
						   strlen("timer.debug"), 1, true),
		      0);
	k_sleep(K_MSEC(2));
	zassert_equal(sq_vm_runtime_next_due_timer(&runtime, event, sizeof(event)), 0);
	zassert_str_equal(event, "timer.debug");
	zassert_not_equal(sq_vm_runtime_next_due_timer(&runtime, event, sizeof(event)), 0);
}
