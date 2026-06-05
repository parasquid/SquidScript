#include "ble_ots.h"

#include <errno.h>
#include <string.h>

#include <zephyr/bluetooth/services/ots.h>
#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

#include "app_store.h"

LOG_MODULE_REGISTER(squidscript_ble_ots, LOG_LEVEL_INF);

#define BT_OTS_OACP_FEAT_CREATE_WRITE                                                       \
	(BIT(BT_OTS_OACP_FEAT_CREATE) | BIT(BT_OTS_OACP_FEAT_WRITE))

static struct bt_ots *sq_ble_ots_instance;
static bool sq_ble_ots_initialized;

static int sq_ble_ots_obj_created_cb(struct bt_ots *ots, struct bt_conn *conn, uint64_t id,
				     const struct bt_ots_obj_add_param *add_param,
				     struct bt_ots_obj_created_desc *created_desc)
{
	ARG_UNUSED(ots);
	ARG_UNUSED(conn);
	ARG_UNUSED(id);
	ARG_UNUSED(add_param);
	ARG_UNUSED(created_desc);
	LOG_DBG("obj_created stub");
	return -ENOTSUP;
}

static int sq_ble_ots_obj_deleted_cb(struct bt_ots *ots, struct bt_conn *conn, uint64_t id)
{
	ARG_UNUSED(ots);
	ARG_UNUSED(conn);
	ARG_UNUSED(id);
	LOG_DBG("obj_deleted stub");
	return 0;
}

static void sq_ble_ots_obj_selected_cb(struct bt_ots *ots, struct bt_conn *conn, uint64_t id)
{
	ARG_UNUSED(ots);
	ARG_UNUSED(conn);
	ARG_UNUSED(id);
	LOG_DBG("obj_selected stub");
}

static ssize_t sq_ble_ots_obj_read_cb(struct bt_ots *ots, struct bt_conn *conn, uint64_t id,
				      void **data, size_t len, off_t offset)
{
	ARG_UNUSED(ots);
	ARG_UNUSED(conn);
	ARG_UNUSED(id);
	ARG_UNUSED(data);
	ARG_UNUSED(len);
	ARG_UNUSED(offset);
	LOG_DBG("obj_read stub");
	return -ENOTSUP;
}

static ssize_t sq_ble_ots_obj_write_cb(struct bt_ots *ots, struct bt_conn *conn, uint64_t id,
				       const void *data, size_t len, off_t offset, size_t rem)
{
	ARG_UNUSED(ots);
	ARG_UNUSED(conn);
	ARG_UNUSED(id);
	ARG_UNUSED(data);
	ARG_UNUSED(offset);
	ARG_UNUSED(rem);
	LOG_DBG("obj_write stub len=%zu rem=%zu", len, rem);
	return (ssize_t)len;
}

static void sq_ble_ots_obj_name_written_cb(struct bt_ots *ots, struct bt_conn *conn, uint64_t id,
					   const char *cur_name, const char *new_name)
{
	ARG_UNUSED(ots);
	ARG_UNUSED(conn);
	ARG_UNUSED(id);
	ARG_UNUSED(cur_name);
	ARG_UNUSED(new_name);
	LOG_DBG("obj_name_written stub");
}

static int sq_ble_ots_obj_cal_checksum_cb(struct bt_ots *ots, struct bt_conn *conn, uint64_t id,
					  off_t offset, size_t len, void **data)
{
	ARG_UNUSED(ots);
	ARG_UNUSED(conn);
	ARG_UNUSED(id);
	ARG_UNUSED(offset);
	ARG_UNUSED(len);
	ARG_UNUSED(data);
	LOG_DBG("obj_cal_checksum stub");
	return -ENOTSUP;
}

static struct bt_ots_cb sq_ble_ots_callbacks = {
	.obj_created = sq_ble_ots_obj_created_cb,
	.obj_deleted = sq_ble_ots_obj_deleted_cb,
	.obj_selected = sq_ble_ots_obj_selected_cb,
	.obj_read = sq_ble_ots_obj_read_cb,
	.obj_write = sq_ble_ots_obj_write_cb,
	.obj_name_written = sq_ble_ots_obj_name_written_cb,
	.obj_cal_checksum = sq_ble_ots_obj_cal_checksum_cb,
};

int sq_ble_ots_init(void)
{
	struct bt_ots_init_param init_param = {
		.features = {
			.oacp = BT_OTS_OACP_FEAT_CREATE_WRITE,
		},
		.cb = &sq_ble_ots_callbacks,
	};
	int result;

	if (sq_ble_ots_initialized) {
		return 0;
	}
	sq_ble_ots_instance = bt_ots_free_instance_get();
	if (sq_ble_ots_instance == NULL) {
		LOG_ERR("no free OTS instance");
		return -ENODEV;
	}
	result = bt_ots_init(sq_ble_ots_instance, &init_param);
	if (result != 0) {
		LOG_ERR("bt_ots_init failed: %d", result);
		return result;
	}
	sq_ble_ots_initialized = true;
	LOG_INF("BLE OTS service registered");
	return 0;
}

void *sq_ble_ots_svc_decl_get(void)
{
	if (!sq_ble_ots_initialized) {
		return NULL;
	}
	return bt_ots_svc_decl_get(sq_ble_ots_instance);
}

int sq_ble_ots_parse_object_name(const char *name, char *app_id_out, size_t app_id_cap,
				 char *profile_id_out, size_t profile_id_cap,
				 char *extension_out, size_t extension_cap)
{
	const char *p;
	const char *q;
	const char *extension;
	size_t app_len;
	size_t prof_len;
	size_t extension_len;

	if (name == NULL || app_id_out == NULL || profile_id_out == NULL ||
	    extension_out == NULL) {
		return -EINVAL;
	}

	p = strchr(name, '/');
	if (p == NULL) {
		return BT_GATT_OTS_OACP_RES_INV_PARAM;
	}
	q = strchr(p + 1, '/');
	if (q == NULL) {
		return BT_GATT_OTS_OACP_RES_INV_PARAM;
	}
	if (strchr(q + 1, '/') != NULL) {
		return BT_GATT_OTS_OACP_RES_INV_PARAM;
	}

	app_len = (size_t)(p - name);
	prof_len = (size_t)(q - (p + 1));
	extension = q + 1;
	extension_len = strlen(extension);

	if (app_len == 0 || prof_len == 0 || extension_len == 0) {
		return BT_GATT_OTS_OACP_RES_INV_PARAM;
	}
	if (app_len >= app_id_cap || prof_len >= profile_id_cap ||
	    extension_len >= extension_cap) {
		return BT_GATT_OTS_OACP_RES_INV_PARAM;
	}
	if (extension[0] != '.') {
		return BT_GATT_OTS_OACP_RES_INV_PARAM;
	}

	memcpy(app_id_out, name, app_len);
	app_id_out[app_len] = '\0';
	if (!sq_app_store_is_safe_app_id(app_id_out)) {
		return BT_GATT_OTS_OACP_RES_INV_PARAM;
	}

	memcpy(profile_id_out, p + 1, prof_len);
	profile_id_out[prof_len] = '\0';

	memcpy(extension_out, extension, extension_len + 1);

	return 0;
}

ssize_t sq_ble_ots_test_invoke_obj_write(struct bt_conn *conn, uint64_t id, const void *data,
					size_t len, off_t offset, size_t rem)
{
	return sq_ble_ots_obj_write_cb(sq_ble_ots_instance, conn, id, data, len, offset, rem);
}

int sq_ble_ots_test_invoke_obj_created(struct bt_conn *conn, uint64_t id,
				       const struct bt_ots_obj_add_param *add_param,
				       struct bt_ots_obj_created_desc *created_desc)
{
	return sq_ble_ots_obj_created_cb(sq_ble_ots_instance, conn, id, add_param, created_desc);
}
