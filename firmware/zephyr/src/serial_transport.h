#ifndef SQUIDSCRIPT_SERIAL_TRANSPORT_H
#define SQUIDSCRIPT_SERIAL_TRANSPORT_H

#include <stddef.h>
#include <stdint.h>

#include "device_protocol.h"
#include "protocol.h"

#define SQ_SERIAL_MAX_FRAME_LEN 320u

struct sq_serial_transport {
	uint8_t request[SQ_SERIAL_MAX_FRAME_LEN];
	size_t request_len;
	size_t expected_len;
};

void sq_serial_transport_init(struct sq_serial_transport *transport);
int sq_serial_transport_push_byte(struct sq_serial_transport *transport, uint8_t byte,
				  const struct sq_device_protocol_context *context,
				  uint8_t *response, size_t response_cap, size_t *response_len);

#endif
