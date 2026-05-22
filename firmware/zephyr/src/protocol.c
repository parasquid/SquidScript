#include "protocol.h"

#include <string.h>

static const uint8_t SQ_PROTOCOL_MAGIC[4] = {'S', 'Q', 'D', 'P'};

static uint32_t read_u32_le(const uint8_t *bytes)
{
	return ((uint32_t)bytes[0]) | ((uint32_t)bytes[1] << 8) |
	       ((uint32_t)bytes[2] << 16) | ((uint32_t)bytes[3] << 24);
}

static void write_u32_le(uint8_t *bytes, uint32_t value)
{
	bytes[0] = value & 0xff;
	bytes[1] = (value >> 8) & 0xff;
	bytes[2] = (value >> 16) & 0xff;
	bytes[3] = (value >> 24) & 0xff;
}

uint32_t sq_protocol_crc32(const uint8_t *data, size_t len)
{
	uint32_t crc = 0xffffffffu;

	for (size_t i = 0; i < len; i++) {
		crc ^= data[i];
		for (int bit = 0; bit < 8; bit++) {
			uint32_t mask = 0u - (crc & 1u);
			crc = (crc >> 1) ^ (0xedb88320u & mask);
		}
	}

	return ~crc;
}

int sq_protocol_decode_frame(const uint8_t *bytes, size_t len, struct sq_protocol_frame *out)
{
	if (len < SQ_PROTOCOL_HEADER_LEN) {
		return SQ_PROTOCOL_ERR_TRUNCATED_HEADER;
	}
	if (memcmp(bytes, SQ_PROTOCOL_MAGIC, sizeof(SQ_PROTOCOL_MAGIC)) != 0) {
		return SQ_PROTOCOL_ERR_BAD_MAGIC;
	}

	uint32_t payload_len = read_u32_le(&bytes[12]);
	size_t expected_len = SQ_PROTOCOL_HEADER_LEN + (size_t)payload_len;

	if (len != expected_len) {
		return SQ_PROTOCOL_ERR_LENGTH_MISMATCH;
	}

	const uint8_t *payload = &bytes[SQ_PROTOCOL_HEADER_LEN];
	uint32_t payload_crc = read_u32_le(&bytes[16]);

	if (sq_protocol_crc32(payload, payload_len) != payload_crc) {
		return SQ_PROTOCOL_ERR_PAYLOAD_CRC;
	}

	out->kind = bytes[4];
	out->opcode = bytes[5];
	out->status = bytes[6];
	out->sequence = read_u32_le(&bytes[8]);
	out->payload = payload;
	out->payload_len = payload_len;
	out->payload_crc = payload_crc;

	return SQ_PROTOCOL_OK;
}

int sq_protocol_next_field(const uint8_t *payload, size_t payload_len, size_t *offset,
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

uint64_t sq_protocol_read_u64_le(const uint8_t *bytes)
{
	uint64_t value = 0;

	for (int i = 7; i >= 0; i--) {
		value <<= 8;
		value |= bytes[i];
	}

	return value;
}

int sq_protocol_append_bytes_field(uint8_t *payload, size_t cap, size_t *len, uint8_t tag,
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

int sq_protocol_append_string_field(uint8_t *payload, size_t cap, size_t *len, uint8_t tag,
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

int sq_protocol_append_u64_field(uint8_t *payload, size_t cap, size_t *len, uint8_t tag,
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

int sq_protocol_encode_frame_header(uint8_t kind, uint8_t opcode, uint8_t status,
				    uint32_t sequence, const uint8_t *payload, size_t payload_len,
				    uint8_t *out, size_t out_len)
{
	if (out_len < SQ_PROTOCOL_HEADER_LEN) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	memcpy(out, SQ_PROTOCOL_MAGIC, sizeof(SQ_PROTOCOL_MAGIC));
	out[4] = kind;
	out[5] = opcode;
	out[6] = status;
	out[7] = 0;
	write_u32_le(&out[8], sequence);
	write_u32_le(&out[12], (uint32_t)payload_len);
	write_u32_le(&out[16], sq_protocol_crc32(payload, payload_len));

	return SQ_PROTOCOL_OK;
}
