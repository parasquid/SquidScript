#include "ble_smoke.h"
#include "ble_smoke_sm.h"

#include <errno.h>
#include <stdbool.h>

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/sys/util.h>

#if IS_ENABLED(CONFIG_BT)
#include <zephyr/bluetooth/bluetooth.h>
#include <zephyr/bluetooth/conn.h>
#include <zephyr/bluetooth/gap.h>
#include <zephyr/bluetooth/uuid.h>

#include "ble_app_transfer.h"
#include "ble_ots.h"
#endif

LOG_MODULE_REGISTER(squidscript_ble, LOG_LEVEL_INF);

#if IS_ENABLED(CONFIG_BT) || defined(CONFIG_SQUIDSCRIPT_BLE_SMOKE_TEST)

static enum sq_ble_smoke_state sq_ble_smoke_state = SQ_BLE_SMOKE_STATE_IDLE;
static const struct sq_ble_smoke_adv_api *sq_ble_smoke_api;

void sq_ble_smoke_sm_install_api(const struct sq_ble_smoke_adv_api *api)
{
	sq_ble_smoke_api = api;
}

enum sq_ble_smoke_state sq_ble_smoke_sm_get_state(void)
{
	return sq_ble_smoke_state;
}

static int sq_ble_smoke_call_adv_start(void)
{
	if (sq_ble_smoke_api == NULL || sq_ble_smoke_api->start == NULL) {
		return -ENOTSUP;
	}
	return sq_ble_smoke_api->start();
}

static int sq_ble_smoke_call_adv_stop(void)
{
	if (sq_ble_smoke_api == NULL || sq_ble_smoke_api->stop == NULL) {
		return -ENOTSUP;
	}
	return sq_ble_smoke_api->stop();
}

static void sq_ble_smoke_advertising_restart_work(struct k_work *work);

K_WORK_DELAYABLE_DEFINE(sq_ble_smoke_restart_advertising,
			sq_ble_smoke_advertising_restart_work);

void sq_ble_smoke_sm_reset(void)
{
	sq_ble_smoke_state = SQ_BLE_SMOKE_STATE_IDLE;
	(void)k_work_cancel_delayable(&sq_ble_smoke_restart_advertising);
}

int sq_ble_smoke_sm_begin_advertising(void)
{
	int result = sq_ble_smoke_call_adv_start();

	if (result == -EALREADY) {
		sq_ble_smoke_state = SQ_BLE_SMOKE_STATE_ADVERTISING;
		return 0;
	}
	if (result != 0) {
		return result;
	}
	sq_ble_smoke_state = SQ_BLE_SMOKE_STATE_ADVERTISING;
	return 0;
}

int sq_ble_smoke_sm_handle_disconnect(void)
{
	sq_ble_smoke_state = SQ_BLE_SMOKE_STATE_RESTART_PENDING;
	(void)k_work_schedule(&sq_ble_smoke_restart_advertising, K_MSEC(100));
	return 0;
}

static void sq_ble_smoke_advertising_restart_work(struct k_work *work)
{
	ARG_UNUSED(work);

	int stop_result = sq_ble_smoke_call_adv_stop();
	if (stop_result != 0 && stop_result != -EALREADY) {
		LOG_WRN("BLE advertising stop before restart failed: %d", stop_result);
		sq_ble_smoke_state = SQ_BLE_SMOKE_STATE_IDLE;
		return;
	}
	LOG_INF("BLE advertising stopped before restart");

	int start_result = sq_ble_smoke_sm_begin_advertising();
	if (start_result != 0) {
		LOG_WRN("BLE advertising restart failed after disconnect: %d", start_result);
		sq_ble_smoke_state = SQ_BLE_SMOKE_STATE_IDLE;
		return;
	}
	LOG_INF("BLE advertising restarted after disconnect");
}

int sq_ble_smoke_sm_run_restart(void)
{
	sq_ble_smoke_advertising_restart_work(&sq_ble_smoke_restart_advertising.work);
	return 0;
}

#if IS_ENABLED(CONFIG_BT)
static const struct bt_data ad[] = {
	BT_DATA_BYTES(BT_DATA_FLAGS, BT_LE_AD_GENERAL | BT_LE_AD_NO_BREDR),
	/* Advertise the app-transfer service UUID so a Web Bluetooth client can
	 * filter for and discover the device.
	 */
	BT_DATA(BT_DATA_UUID128_ALL, sq_ble_app_transfer_adv_uuid,
		sizeof(sq_ble_app_transfer_adv_uuid)),
};

static const struct bt_data sd[] = {
	BT_DATA(BT_DATA_NAME_COMPLETE, CONFIG_BT_DEVICE_NAME,
		sizeof(CONFIG_BT_DEVICE_NAME) - 1),
};

static int sq_ble_smoke_adv_start_real(void)
{
	return bt_le_adv_start(BT_LE_ADV_CONN_FAST_1, ad, ARRAY_SIZE(ad), sd, ARRAY_SIZE(sd));
}

static int sq_ble_smoke_adv_stop_real(void)
{
	return bt_le_adv_stop();
}

static const struct sq_ble_smoke_adv_api sq_ble_smoke_real_api = {
	.start = sq_ble_smoke_adv_start_real,
	.stop = sq_ble_smoke_adv_stop_real,
};

static void sq_ble_smoke_disconnected(struct bt_conn *conn, uint8_t reason)
{
	ARG_UNUSED(conn);
	ARG_UNUSED(reason);

	/* Discard any in-flight BLE object transfer that the peer abandoned. */
	sq_ble_app_transfer_reset();
	sq_ble_ots_reset_session();
	(void)sq_ble_smoke_sm_handle_disconnect();
}

BT_CONN_CB_DEFINE(sq_ble_smoke_conn_callbacks) = {
	.disconnected = sq_ble_smoke_disconnected,
};
#endif

#endif

int sq_ble_smoke_start(void)
{
#if IS_ENABLED(CONFIG_BT)
	int result = bt_enable(NULL);
	if (result != 0) {
		LOG_WRN("BLE init failed: %d", result);
		return result;
	}

	/* Register the OTS object-transfer GATT service before advertising so it
	 * is present in the GATT database for connecting peers. Non-fatal: a
	 * failure here must not prevent basic advertising.
	 */
	int ots_result = sq_ble_ots_init();
	if (ots_result != 0) {
		LOG_WRN("BLE OTS init failed: %d", ots_result);
	}

	sq_ble_smoke_sm_install_api(&sq_ble_smoke_real_api);

	result = sq_ble_smoke_sm_begin_advertising();
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
