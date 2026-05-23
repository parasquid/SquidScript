#include "protocol.h"

#include <string.h>

static const uint8_t SQ_PROTOCOL_MAGIC[4] = {'S', 'Q', 'D', 'P'};

static uint32_t read_u32_le(const uint8_t *bytes)
{
	return ((uint32_t)bytes[0]) | ((uint32_t)bytes[1] << 8) |
	       ((uint32_t)bytes[2] << 16) | ((uint32_t)bytes[3] << 24);
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
