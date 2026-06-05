#ifndef SQUIDSCRIPT_BLE_OTS_H
#define SQUIDSCRIPT_BLE_OTS_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include <zephyr/bluetooth/services/ots.h>

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

ssize_t sq_ble_ots_test_invoke_obj_write(struct bt_conn *conn, uint64_t id, const void *data,
					size_t len, off_t offset, size_t rem);

int sq_ble_ots_test_invoke_obj_created(struct bt_conn *conn, uint64_t id,
				       const struct bt_ots_obj_add_param *add_param,
				       struct bt_ots_obj_created_desc *created_desc);

#ifdef __cplusplus
}
#endif

#endif /* SQUIDSCRIPT_BLE_OTS_H */
