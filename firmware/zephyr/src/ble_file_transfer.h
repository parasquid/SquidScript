#ifndef SQUIDSCRIPT_BLE_FILE_TRANSFER_H
#define SQUIDSCRIPT_BLE_FILE_TRANSFER_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Custom GATT file-transfer service: the bleak/Web-Bluetooth-drivable upload
 * transport. The service is auto-registered via BT_GATT_SERVICE_DEFINE, so
 * there is no init/register call; it drives the transport-neutral core in
 * ble_file_transfer_core.h. The control characteristic carries BEGIN(name+size)/
 * ABORT, the data characteristic carries chunked content, and the status
 * characteristic notifies completion/error.
 */

/* Reset the per-connection transport state (call on disconnect). */
void sq_ble_file_transfer_reset(void);

/* The advertised 128-bit service UUID as a raw 16-byte little-endian array, for
 * BT_DATA(BT_DATA_UUID128_ALL, ...). Web Bluetooth filters by this UUID. All
 * zeroes when CONFIG_BT is disabled.
 */
extern const uint8_t sq_ble_file_transfer_adv_uuid[16];

#ifdef __cplusplus
}
#endif

#endif /* SQUIDSCRIPT_BLE_FILE_TRANSFER_H */
