#ifndef SQUIDSCRIPT_PROTOCOL_H
#define SQUIDSCRIPT_PROTOCOL_H

#include <stddef.h>
#include <stdint.h>

#define SQ_PROTOCOL_HEADER_LEN 20u
#define SQ_PROTOCOL_DONE 1

enum sq_protocol_result {
	SQ_PROTOCOL_OK = 0,
	SQ_PROTOCOL_ERR_TRUNCATED_HEADER = -1,
	SQ_PROTOCOL_ERR_BAD_MAGIC = -2,
	SQ_PROTOCOL_ERR_LENGTH_MISMATCH = -3,
	SQ_PROTOCOL_ERR_PAYLOAD_CRC = -4,
	SQ_PROTOCOL_ERR_TRUNCATED_FIELD = -5,
	SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL = -6,
};

enum sq_frame_kind {
	SQ_FRAME_REQUEST = 1,
	SQ_FRAME_RESPONSE = 2,
	SQ_FRAME_EVENT = 3,
};

enum sq_opcode {
	SQ_OPCODE_HELLO = 1,
	SQ_OPCODE_APP_INSTALL_BEGIN = 16,
	SQ_OPCODE_APP_INSTALL_CHUNK = 17,
	SQ_OPCODE_APP_INSTALL_COMMIT = 18,
	SQ_OPCODE_TEMP_RUN_BEGIN = 24,
	SQ_OPCODE_TEMP_RUN_CHUNK = 25,
	SQ_OPCODE_TEMP_RUN_COMMIT = 26,
	SQ_OPCODE_APP_LAUNCH = 32,
	SQ_OPCODE_APP_LIST = 33,
	SQ_OPCODE_KEY = 48,
	SQ_OPCODE_OUTPUT_GET = 64,
	SQ_OPCODE_STATE_GET = 65,
	SQ_OPCODE_DRAWLOG_GET = 66,
	SQ_OPCODE_TRACE_GET = 67,
	SQ_OPCODE_ERRORS_GET = 68,
	SQ_OPCODE_RESOURCES_GET = 69,
	SQ_OPCODE_RESET = 80,
};

enum sq_status {
	SQ_STATUS_OK = 0,
	SQ_STATUS_ERROR = 1,
	SQ_STATUS_PENDING = 2,
};

enum sq_field_type {
	SQ_FIELD_BYTES = 0,
	SQ_FIELD_STRING = 1,
	SQ_FIELD_BOOL = 3,
	SQ_FIELD_I64 = 4,
	SQ_FIELD_U64 = 5,
	SQ_FIELD_RECORD = 32,
};

struct sq_protocol_frame {
	uint8_t kind;
	uint8_t opcode;
	uint8_t status;
	uint32_t sequence;
	const uint8_t *payload;
	uint32_t payload_len;
	uint32_t payload_crc;
};

struct sq_protocol_field {
	uint8_t tag;
	uint8_t type;
	const uint8_t *value;
	uint16_t len;
};

uint32_t sq_protocol_crc32(const uint8_t *data, size_t len);
int sq_protocol_decode_frame(const uint8_t *bytes, size_t len, struct sq_protocol_frame *out);
int sq_protocol_next_field(const uint8_t *payload, size_t payload_len, size_t *offset,
			   struct sq_protocol_field *out);
uint64_t sq_protocol_read_u64_le(const uint8_t *bytes);
int sq_protocol_append_bytes_field(uint8_t *payload, size_t cap, size_t *len, uint8_t tag,
				   const uint8_t *value, uint16_t value_len);
int sq_protocol_append_string_field(uint8_t *payload, size_t cap, size_t *len, uint8_t tag,
				    const char *value);
int sq_protocol_append_u64_field(uint8_t *payload, size_t cap, size_t *len, uint8_t tag,
				 uint64_t value);
int sq_protocol_encode_frame_header(uint8_t kind, uint8_t opcode, uint8_t status,
				    uint32_t sequence, const uint8_t *payload, size_t payload_len,
				    uint8_t *out, size_t out_len);

#endif
