#include <zephyr/kernel.h>
#include <zephyr/device.h>
#include <zephyr/drivers/uart.h>
#include <zephyr/logging/log.h>

#include "app_store.h"
#include "device_protocol.h"
#include "serial_transport.h"

LOG_MODULE_REGISTER(squidscript, LOG_LEVEL_INF);

int main(void)
{
	const struct device *uart = DEVICE_DT_GET(DT_CHOSEN(zephyr_console));
	struct sq_serial_transport transport;
	const struct sq_device_identity identity = {
		.target = CONFIG_BOARD,
		.firmware = "squidscript-zephyr",
		.diagnostic = IS_ENABLED(CONFIG_SQUIDSCRIPT_ZEPHYR_DIAGNOSTIC),
	};
	static struct sq_app_registry registry;
	static struct sq_device_install_session install_session;
	static struct sq_device_temp_session temp_session;
	static struct sq_device_resource_session resource_session;
	static struct sq_vm_runtime runtime;
	static struct sq_app_store_vm_storage launch_storage;
	static uint8_t response[SQ_DEVICE_RESPONSE_BYTES];
	struct sq_device_protocol_context protocol_context = {
		.identity = &identity,
		.registry = &registry,
		.mutable_registry = &registry,
		.install_session = &install_session,
		.temp_session = &temp_session,
		.resource_session = &resource_session,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
	};
	size_t response_len = 0;
	uint8_t byte;

#if IS_ENABLED(CONFIG_SQUIDSCRIPT_ZEPHYR_DIAGNOSTIC)
	LOG_INF("SquidScript Zephyr firmware diagnostic boot");
#endif

	if (!device_is_ready(uart)) {
		LOG_ERR("Zephyr console UART is not ready");
		return 1;
	}

	int storage_result = sq_app_store_mount_target_filesystem();
	if (storage_result == 0) {
		LOG_INF("Mounted SquidScript app store at %s", sq_app_store_mount_point());
		protocol_context.store_mount_point = sq_app_store_mount_point();
		sq_vm_runtime_set_store_mount_point(&runtime, sq_app_store_mount_point());
		int registry_result = sq_app_store_scan_registry(sq_app_store_mount_point(), &registry);
		if (registry_result != 0) {
			LOG_WRN("SquidScript app registry unavailable: %d", registry_result);
		}
	} else {
		LOG_WRN("SquidScript app store unavailable: %d", storage_result);
	}

	sq_serial_transport_init(&transport);
	sq_vm_runtime_init(&runtime);

	while (true) {
		bool consumed = false;

		while (uart_poll_in(uart, &byte) == 0) {
			consumed = true;
			int result = sq_serial_transport_push_byte(&transport, byte,
								   &protocol_context,
								   response, sizeof(response),
								   &response_len);
			if (result > 0) {
				for (size_t i = 0; i < response_len; i++) {
					uart_poll_out(uart, response[i]);
				}
			} else if (result < 0) {
				sq_serial_transport_init(&transport);
			}
		}

		(void)sq_device_protocol_poll(&protocol_context);

		if (!consumed) {
			k_sleep(K_MSEC(1));
		}
	}
}
