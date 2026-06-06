#ifndef SQUIDSCRIPT_BLE_OBJECT_TRANSFER_H
#define SQUIDSCRIPT_BLE_OBJECT_TRANSFER_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <sys/types.h>

/*
 * Transport-neutral BLE object-transfer core: the single in-flight staging
 * session, the object-name router/parser, and the completed-transfer pending
 * event handed off to the runtime poll loop. The BLE transport front-end
 * (ble_app_transfer.c) drives this core; it lives in its own translation unit
 * so additional transport front-ends can sit on top without depending on a
 * specific one.
 */

/* Reject codes the object-name parser / create returns when the request is
 * malformed (INV_PARAM) or a transfer is already in progress (OBJ_LOCKED). A
 * transport front-end maps them to its own wire-level error (e.g. an ATT error
 * for the GATT transport).
 */
#define SQ_BLE_OTS_OACP_RES_INV_PARAM  0x03
#define SQ_BLE_OTS_OACP_RES_OBJ_LOCKED 0x0a
#define BT_GATT_OTS_OACP_RES_INV_PARAM SQ_BLE_OTS_OACP_RES_INV_PARAM
#define BT_GATT_OTS_OACP_RES_OBJ_LOCKED SQ_BLE_OTS_OACP_RES_OBJ_LOCKED

#ifdef __cplusplus
extern "C" {
#endif

int sq_ble_ots_parse_object_name(const char *name, char *app_id_out, size_t app_id_cap,
				 char *profile_id_out, size_t profile_id_cap,
				 char *extension_out, size_t extension_cap);

/* Transport-facing API used by a BLE transport front-end (custom GATT service).
 * begin opens a staging session from the object name + declared size; write
 * appends a chunk at `offset` and, when `rem` reaches 0, publishes the
 * completion pending event; abort discards an in-flight session. Return values
 * are 0 on success or a reject/-errno code (see codes above / errno.h).
 */
int sq_ble_transfer_begin(const char *name, size_t alloc_size);

int sq_ble_transfer_write_chunk(const void *data, size_t len, off_t offset, size_t rem);

void sq_ble_transfer_abort(void);

/* Framed transfer for transports whose individual writes must stay small (e.g.
 * GATT at the default ATT MTU). begin_framed declares the content size and the
 * object-name length; feed_name appends name bytes (across one or more writes)
 * until name_len is reached, at which point the name is parsed and the staging
 * file opened; feed_content then appends content bytes, and completion (content
 * fully received) publishes the same pending event as the non-framed path.
 * Each returns 0 on success or a reject/-errno code.
 */
int sq_ble_transfer_begin_framed(size_t total_size, size_t name_len);

int sq_ble_transfer_feed_name(const void *data, size_t len);

int sq_ble_transfer_feed_content(const void *data, size_t len);

void sq_ble_ots_reset_session(void);

int sq_ble_ots_drain_pending_event(char *app_id_out, size_t app_id_cap, char *event_out,
				   size_t event_cap);

const char *sq_ble_ots_pending_staging_path(void);

const char *sq_ble_ots_pending_profile_id(void);

size_t sq_ble_ots_pending_bytes_received(void);

size_t sq_ble_ots_pending_total_bytes(void);

void sq_ble_ots_cleanup_staging(void);

bool sq_ble_ots_pending_is_complete(void);

const char *sq_ble_ots_pending_app_id(void);

const char *sq_ble_ots_pending_event_name(void);

int sq_ble_ots_test_invoke_obj_created_with_name(const char *name, size_t alloc_size,
						 char *staging_path_out,
						 size_t staging_path_out_len);

int sq_ble_ots_test_invoke_obj_write_with_path(const char *staging_path, const void *data,
					       size_t len, off_t offset, size_t rem);

void sq_ble_ots_test_invoke_abort(void);

#ifdef __cplusplus
}
#endif

#endif /* SQUIDSCRIPT_BLE_OBJECT_TRANSFER_H */
