#ifndef SQUIDSCRIPT_BLE_OBJECT_TRANSFER_H
#define SQUIDSCRIPT_BLE_OBJECT_TRANSFER_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <sys/types.h>

/*
 * Transport-neutral BLE object-transfer core: the single in-flight staging
 * session, the object-name router/parser, and the completed-transfer pending
 * event handed off to the runtime poll loop. Both the OTS (ble_ots.c) and the
 * custom GATT (ble_app_transfer.c) front-ends drive this core, so it lives in
 * its own translation unit independent of any one transport.
 *
 * NOTE: symbols still carry the historical sq_ble_ots_* prefix; renaming to a
 * neutral sq_ble_transfer_* prefix is a follow-up step.
 */

/* OACP result codes the object-name parser returns on a rejected create. The
 * numeric values match the BT SIG GSS spec / Zephyr bt_gatt_ots_oacp_res_code
 * so the OTS front-end can return them verbatim without reaching into the OTS
 * internal header. (The custom GATT front-end maps them to its own status.)
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

void sq_ble_ots_reset_session(void);

int sq_ble_ots_drain_pending_event(char *app_id_out, size_t app_id_cap, char *event_out,
				   size_t event_cap);

const char *sq_ble_ots_pending_staging_path(void);

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
