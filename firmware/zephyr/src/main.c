#include <zephyr/kernel.h>
#include <zephyr/device.h>
#include <zephyr/drivers/uart.h>
#include <zephyr/logging/log.h>
#include <zephyr/fs/fs.h>
#include <zephyr/sys/util.h>
#include <errno.h>
#if defined(CONFIG_SOC_ESP32C3)
#include <zephyr/sys/poweroff.h>
#include <esp_sleep.h>
#endif

#include "app_store.h"
#include "device_protocol.h"
#include "serial_transport.h"
#include "squidscript_fallback_app.h"

LOG_MODULE_REGISTER(squidscript, LOG_LEVEL_INF);

#if defined(CONFIG_SOC_ESP32C3)
int sq_device_protocol_enter_planned_sleep(int32_t wake_after_ms)
{
	if (wake_after_ms <= 0) {
		return -EINVAL;
	}
	esp_sleep_enable_timer_wakeup((uint64_t)wake_after_ms * 1000ULL);
	LOG_INF("planned sleep entering deep sleep for %d ms", wake_after_ms);
	sys_poweroff();
	return 0;
}

static bool planned_resume_wake_cause(void)
{
	return (esp_sleep_get_wakeup_causes() & BIT(ESP_SLEEP_WAKEUP_TIMER)) != 0;
}
#else
static bool planned_resume_wake_cause(void)
{
	return false;
}
#endif

static void clear_stale_planned_resume(const char *mount_point)
{
	char path[SQ_APP_STORE_PLANNED_RESUME_PATH_MAX];

	if (mount_point == NULL ||
	    sq_app_store_planned_resume_path(mount_point, path, sizeof(path)) != 0) {
		return;
	}
	(void)fs_unlink(path);
}

int main(void)
{
	const struct device *uart = DEVICE_DT_GET(DT_CHOSEN(zephyr_console));
	static struct sq_serial_transport transport;
	static const struct sq_device_identity identity = {
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
	static struct sq_app_store_vm_storage trigger_storage;
	static uint8_t response[SQ_DEVICE_RESPONSE_BYTES];
	static struct sq_device_protocol_context protocol_context;
	size_t response_len = 0;
	uint8_t byte;
	bool registry_ready = false;

#if IS_ENABLED(CONFIG_SQUIDSCRIPT_ZEPHYR_DIAGNOSTIC)
	LOG_INF("SquidScript Zephyr firmware diagnostic boot");
#endif

	if (!device_is_ready(uart)) {
		LOG_ERR("Zephyr console UART is not ready");
		return 1;
	}

	protocol_context = (struct sq_device_protocol_context){
		.identity = &identity,
		.registry = &registry,
		.mutable_registry = &registry,
		.install_session = &install_session,
		.temp_session = &temp_session,
		.resource_session = &resource_session,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
		.trigger_storage = &trigger_storage,
		.fallback_app = &sq_zephyr_fallback_app,
	};

	int storage_result = sq_app_store_mount_target_filesystem();
	if (storage_result == 0) {
		LOG_INF("Mounted SquidScript app store at %s", sq_app_store_mount_point());
		protocol_context.store_mount_point = sq_app_store_mount_point();
		sq_vm_runtime_set_store_mount_point(&runtime, sq_app_store_mount_point());
		int registry_result = sq_app_store_scan_registry(sq_app_store_mount_point(), &registry);
		if (registry_result != 0) {
			LOG_WRN("SquidScript app registry unavailable: %d", registry_result);
		} else {
			registry_ready = true;
		}
	} else {
		LOG_WRN("SquidScript app store unavailable: %d", storage_result);
	}

	sq_serial_transport_init(&transport);
	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_set_registry(&runtime, &registry);
	if (registry_ready) {
		int root_result;

		if (planned_resume_wake_cause()) {
			root_result = sq_device_protocol_restore_planned_resume(&protocol_context);
			if (root_result == 0) {
				LOG_INF("planned resume restored foreground app");
			} else {
				LOG_WRN("planned resume restore failed: %d", root_result);
				root_result = sq_device_protocol_start_root(&protocol_context);
			}
		} else {
			clear_stale_planned_resume(protocol_context.store_mount_point);
			root_result = sq_device_protocol_start_root(&protocol_context);
		}
		if (root_result != 0) {
			LOG_WRN("SquidScript root app launch failed: %d", root_result);
		}
	}

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
