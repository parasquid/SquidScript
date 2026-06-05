#ifndef SQUIDSCRIPT_BLE_OTS_H
#define SQUIDSCRIPT_BLE_OTS_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include <zephyr/bluetooth/services/ots.h>

#include "vm_runtime.h"

/* OTS Object Action Control Point result codes that this module emits when
 * the OTS layer should reject an OACP Create. The numeric values match the
 * BT SIG GSS spec and the Zephyr bt_gatt_ots_oacp_res_code enum in
 * subsys/bluetooth/services/ots/ots_oacp_internal.h. We re-declare them here
 * so callers do not need to reach into the OTS internal header.
 */
#define SQ_BLE_OTS_OACP_RES_INV_PARAM  0x03
#define SQ_BLE_OTS_OACP_RES_OBJ_LOCKED 0x0a
#define BT_GATT_OTS_OACP_RES_INV_PARAM SQ_BLE_OTS_OACP_RES_INV_PARAM
#define BT_GATT_OTS_OACP_RES_OBJ_LOCKED SQ_BLE_OTS_OACP_RES_OBJ_LOCKED

#ifdef __cplusplus
extern "C" {
#endif

struct sq_ble_ots_obj_created_args {
	struct bt_conn *conn;
	uint64_t id;
	const struct bt_ots_obj_add_param *add_param;
	struct bt_ots_obj_created_desc *created_desc;
};

struct sq_ble_ots_obj_write_args {
	struct bt_conn *conn;
	uint64_t id;
	const void *data;
	size_t len;
	off_t offset;
	size_t rem;
};

int sq_ble_ots_init(void);

void *sq_ble_ots_svc_decl_get(void);

int sq_ble_ots_parse_object_name(const char *name, char *app_id_out, size_t app_id_cap,
				 char *profile_id_out, size_t profile_id_cap,
				 char *extension_out, size_t extension_cap);

struct sq_ble_profile_entry {
	char app_id[SQ_APP_STORE_APP_ID_MAX];
	char profile_id[SQVM_BLE_PROFILE_TEXT_CAP];
	char accept_exts[SQVM_BLE_PROFILE_ACCEPT_MAX][SQVM_BLE_PROFILE_TEXT_CAP];
	uint8_t accept_count;
	SqvmBleProfileEventRoute events[SQVM_BLE_PROFILE_EVENT_MAX];
	uint8_t event_count;
};

int sq_ble_profile_table_add(const char *app_id, const char *profile_id,
			     const char (*accept_exts)[SQVM_BLE_PROFILE_TEXT_CAP],
			     uint8_t accept_count,
			     const SqvmBleProfileEventRoute *events, uint8_t event_count);

void sq_ble_profile_table_remove_app(const char *app_id);

void sq_ble_profile_table_reset(void);

size_t sq_ble_profile_table_count(void);

const struct sq_ble_profile_entry *sq_ble_profile_lookup(const char *app_id,
							const char *profile_id);

void sq_ble_ots_reset_session(void);

int sq_ble_ots_drain_pending_event(char *app_id_out, size_t app_id_cap, char *event_out,
				   size_t event_cap);

bool sq_ble_ots_pending_is_complete(void);

const char *sq_ble_ots_pending_app_id(void);

const char *sq_ble_ots_pending_event_name(void);

int sq_ble_ots_test_invoke_obj_created_with_name(const char *name, size_t alloc_size,
						 char *staging_path_out,
						 size_t staging_path_out_len);

int sq_ble_ots_test_invoke_obj_write_with_path(const char *staging_path, const void *data,
					       size_t len, off_t offset, size_t rem);

void sq_ble_ots_test_invoke_abort(void);

ssize_t sq_ble_ots_test_invoke_obj_write(struct bt_conn *conn, uint64_t id, const void *data,
					size_t len, off_t offset, size_t rem);

int sq_ble_ots_test_invoke_obj_created(struct bt_conn *conn, uint64_t id,
				       const struct bt_ots_obj_add_param *add_param,
				       struct bt_ots_obj_created_desc *created_desc);

#ifdef __cplusplus
}
#endif

#endif /* SQUIDSCRIPT_BLE_OTS_H */
