#include "ble_smoke.h"

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/sys/util.h>

#if IS_ENABLED(CONFIG_BT)
#include <zephyr/bluetooth/bluetooth.h>
#include <zephyr/bluetooth/gap.h>
#include <zephyr/bluetooth/uuid.h>
#endif

LOG_MODULE_REGISTER(squidscript_ble, LOG_LEVEL_INF);

#if IS_ENABLED(CONFIG_BT)
static const struct bt_data ad[] = {
	BT_DATA_BYTES(BT_DATA_FLAGS, BT_LE_AD_GENERAL | BT_LE_AD_NO_BREDR),
};

static const struct bt_data sd[] = {
	BT_DATA(BT_DATA_NAME_COMPLETE, CONFIG_BT_DEVICE_NAME,
		sizeof(CONFIG_BT_DEVICE_NAME) - 1),
};
#endif

int sq_ble_smoke_start(void)
{
#if IS_ENABLED(CONFIG_BT)
	int result = bt_enable(NULL);
	if (result != 0) {
		LOG_WRN("BLE init failed: %d", result);
		return result;
	}

	result = bt_le_adv_start(BT_LE_ADV_CONN_FAST_1, ad, ARRAY_SIZE(ad), sd, ARRAY_SIZE(sd));
	if (result != 0) {
		LOG_WRN("BLE advertising failed: %d", result);
		return result;
	}

	LOG_INF("BLE advertising started: %s", CONFIG_BT_DEVICE_NAME);
	return 0;
#else
	return -ENOTSUP;
#endif
}
