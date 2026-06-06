#include "ble_app_transfer.h"

#include <zephyr/sys/util.h>

#if IS_ENABLED(CONFIG_BT)

#include <string.h>

#include <zephyr/bluetooth/att.h>
#include <zephyr/bluetooth/bluetooth.h>
#include <zephyr/bluetooth/conn.h>
#include <zephyr/bluetooth/gatt.h>
#include <zephyr/bluetooth/uuid.h>
#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/sys/byteorder.h>

#include "ble_object_transfer.h"

LOG_MODULE_REGISTER(squidscript_ble_app_transfer, LOG_LEVEL_INF);

/* Vendor 128-bit UUIDs (base 7e57c0de-000N-4a5b-8c6d-0123456789ab). */
#define SQ_XFER_SVC_UUID  BT_UUID_128_ENCODE(0x7e57c0de, 0x0001, 0x4a5b, 0x8c6d, 0x0123456789ab)
#define SQ_XFER_CTRL_UUID BT_UUID_128_ENCODE(0x7e57c0de, 0x0002, 0x4a5b, 0x8c6d, 0x0123456789ab)
#define SQ_XFER_DATA_UUID BT_UUID_128_ENCODE(0x7e57c0de, 0x0003, 0x4a5b, 0x8c6d, 0x0123456789ab)
#define SQ_XFER_STAT_UUID BT_UUID_128_ENCODE(0x7e57c0de, 0x0004, 0x4a5b, 0x8c6d, 0x0123456789ab)

static const struct bt_uuid_128 sq_xfer_svc_uuid = BT_UUID_INIT_128(SQ_XFER_SVC_UUID);
static const struct bt_uuid_128 sq_xfer_ctrl_uuid = BT_UUID_INIT_128(SQ_XFER_CTRL_UUID);
static const struct bt_uuid_128 sq_xfer_data_uuid = BT_UUID_INIT_128(SQ_XFER_DATA_UUID);
static const struct bt_uuid_128 sq_xfer_stat_uuid = BT_UUID_INIT_128(SQ_XFER_STAT_UUID);

const uint8_t sq_ble_app_transfer_adv_uuid[16] = {SQ_XFER_SVC_UUID};

/* Control opcodes (first byte of a control write). */
#define SQ_XFER_OP_BEGIN 0x01
#define SQ_XFER_OP_NAME  0x02
#define SQ_XFER_OP_ABORT 0x03

/* Status notification codes. */
#define SQ_XFER_STATUS_COMPLETE 0x00
#define SQ_XFER_STATUS_ERROR    0x01

/* The transfer state (size, name, offset) lives in the transport-neutral core
 * (ble_object_transfer.c); this shim only frames opcodes onto its API. All GATT
 * write callbacks run on the BT RX thread.
 */
static void sq_xfer_notify_status(uint8_t code);

static ssize_t sq_xfer_ctrl_write(struct bt_conn *conn, const struct bt_gatt_attr *attr,
				  const void *buf, uint16_t len, uint16_t offset, uint8_t flags)
{
	const uint8_t *bytes = buf;
	uint8_t op;

	ARG_UNUSED(conn);
	ARG_UNUSED(attr);
	ARG_UNUSED(offset);
	ARG_UNUSED(flags);

	if (len < 1) {
		return BT_GATT_ERR(BT_ATT_ERR_INVALID_ATTRIBUTE_LEN);
	}
	op = bytes[0];

	if (op == SQ_XFER_OP_BEGIN) {
		uint32_t size;
		uint16_t name_len;

		if (len != 7) {
			return BT_GATT_ERR(BT_ATT_ERR_INVALID_ATTRIBUTE_LEN);
		}
		size = sys_get_le32(&bytes[1]);
		name_len = sys_get_le16(&bytes[5]);
		if (sq_ble_transfer_begin_framed(size, name_len) != 0) {
			LOG_WRN("xfer begin rejected (size=%u name_len=%u)", size, name_len);
			sq_xfer_notify_status(SQ_XFER_STATUS_ERROR);
			return BT_GATT_ERR(BT_ATT_ERR_WRITE_NOT_PERMITTED);
		}
		LOG_INF("xfer begin size=%u name_len=%u", size, name_len);
		return len;
	}

	if (op == SQ_XFER_OP_NAME) {
		if (sq_ble_transfer_feed_name(&bytes[1], (size_t)len - 1u) != 0) {
			sq_ble_transfer_abort();
			sq_xfer_notify_status(SQ_XFER_STATUS_ERROR);
			return BT_GATT_ERR(BT_ATT_ERR_WRITE_NOT_PERMITTED);
		}
		return len;
	}

	if (op == SQ_XFER_OP_ABORT) {
		sq_ble_transfer_abort();
		LOG_INF("xfer aborted by client");
		return len;
	}

	return BT_GATT_ERR(BT_ATT_ERR_VALUE_NOT_ALLOWED);
}

static ssize_t sq_xfer_data_write(struct bt_conn *conn, const struct bt_gatt_attr *attr,
				  const void *buf, uint16_t len, uint16_t offset, uint8_t flags)
{
	int result;

	ARG_UNUSED(conn);
	ARG_UNUSED(attr);
	ARG_UNUSED(offset);
	ARG_UNUSED(flags);

	result = sq_ble_transfer_feed_content(buf, len);
	if (result != 0) {
		sq_ble_transfer_abort();
		LOG_WRN("xfer content rejected: %d", result);
		sq_xfer_notify_status(SQ_XFER_STATUS_ERROR);
		return BT_GATT_ERR(BT_ATT_ERR_UNLIKELY);
	}
	if (sq_ble_ots_pending_is_complete()) {
		LOG_INF("xfer content complete");
		sq_xfer_notify_status(SQ_XFER_STATUS_COMPLETE);
	}
	return len;
}

static void sq_xfer_ccc_changed(const struct bt_gatt_attr *attr, uint16_t value)
{
	ARG_UNUSED(attr);
	ARG_UNUSED(value);
}

BT_GATT_SERVICE_DEFINE(sq_ble_app_transfer_svc,
	BT_GATT_PRIMARY_SERVICE(&sq_xfer_svc_uuid),
	BT_GATT_CHARACTERISTIC(&sq_xfer_ctrl_uuid.uuid, BT_GATT_CHRC_WRITE, BT_GATT_PERM_WRITE,
			       NULL, sq_xfer_ctrl_write, NULL),
	BT_GATT_CHARACTERISTIC(&sq_xfer_data_uuid.uuid,
			       BT_GATT_CHRC_WRITE | BT_GATT_CHRC_WRITE_WITHOUT_RESP,
			       BT_GATT_PERM_WRITE, NULL, sq_xfer_data_write, NULL),
	BT_GATT_CHARACTERISTIC(&sq_xfer_stat_uuid.uuid, BT_GATT_CHRC_NOTIFY, BT_GATT_PERM_NONE,
			       NULL, NULL, NULL),
	BT_GATT_CCC(sq_xfer_ccc_changed, BT_GATT_PERM_READ | BT_GATT_PERM_WRITE),
);

/* Attribute layout of sq_ble_app_transfer_svc.attrs:
 *   [0] primary service
 *   [1] control char declaration   [2] control char value
 *   [3] data char declaration      [4] data char value
 *   [5] status char declaration    [6] status char value  <-- notify here
 *   [7] status CCC
 */
#define SQ_XFER_STATUS_VALUE_ATTR (&sq_ble_app_transfer_svc.attrs[6])

static void sq_xfer_notify_status(uint8_t code)
{
	(void)bt_gatt_notify(NULL, SQ_XFER_STATUS_VALUE_ATTR, &code, sizeof(code));
}

void sq_ble_app_transfer_reset(void)
{
	/* Discard any in-flight transfer; the framing/session state lives in the
	 * transport-neutral core.
	 */
	sq_ble_transfer_abort();
}

#else /* !CONFIG_BT */

const uint8_t sq_ble_app_transfer_adv_uuid[16] = {0};

void sq_ble_app_transfer_reset(void)
{
}

#endif
