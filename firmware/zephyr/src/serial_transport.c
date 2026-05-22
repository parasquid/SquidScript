#include "serial_transport.h"

static uint32_t read_u32_le_transport(const uint8_t *bytes)
{
	return ((uint32_t)bytes[0]) | ((uint32_t)bytes[1] << 8) |
	       ((uint32_t)bytes[2] << 16) | ((uint32_t)bytes[3] << 24);
}

void sq_serial_transport_init(struct sq_serial_transport *transport)
{
	transport->request_len = 0;
	transport->expected_len = 0;
}

int sq_serial_transport_push_byte(struct sq_serial_transport *transport, uint8_t byte,
				  const struct sq_device_protocol_context *context,
				  uint8_t *response, size_t response_cap, size_t *response_len)
{
	*response_len = 0;

	if (transport->request_len >= sizeof(transport->request)) {
		sq_serial_transport_init(transport);
		return SQ_PROTOCOL_ERR_LENGTH_MISMATCH;
	}

	transport->request[transport->request_len++] = byte;

	if (transport->request_len == SQ_PROTOCOL_HEADER_LEN) {
		uint32_t payload_len = read_u32_le_transport(&transport->request[12]);
		size_t expected_len = SQ_PROTOCOL_HEADER_LEN + (size_t)payload_len;

		if (expected_len > sizeof(transport->request)) {
			sq_serial_transport_init(transport);
			return SQ_PROTOCOL_ERR_LENGTH_MISMATCH;
		}
		transport->expected_len = expected_len;
	}

	if (transport->expected_len == 0 || transport->request_len < transport->expected_len) {
		return 0;
	}

	int result = sq_device_protocol_handle_frame(transport->request, transport->request_len,
						    context, response, response_cap, response_len);
	sq_serial_transport_init(transport);

	if (result != SQ_PROTOCOL_OK) {
		return result;
	}

	return 1;
}
