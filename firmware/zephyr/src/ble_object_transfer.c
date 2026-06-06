#include "ble_object_transfer.h"

#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <zephyr/fs/fs.h>
#include <zephyr/logging/log.h>

#include "app_store.h"
#include "vm_runtime.h"

LOG_MODULE_REGISTER(squidscript_ble_transfer, LOG_LEVEL_INF);

#define SQ_BLE_OTS_APP_ID_MAX      SQ_APP_STORE_APP_ID_MAX
#define SQ_BLE_OTS_PROFILE_ID_MAX  32
#define SQ_BLE_OTS_PATH_MAX        SQ_APP_STORE_PATH_MAX

#ifndef SQ_BLE_OTS_STAGING_DIR
#define SQ_BLE_OTS_STAGING_DIR     "/sq/tmp"
#endif

struct sq_ble_ots_session {
	bool active;
	char app_id[SQ_BLE_OTS_APP_ID_MAX];
	char profile_id[SQ_BLE_OTS_PROFILE_ID_MAX];
	char staging_path[SQ_BLE_OTS_PATH_MAX];
	size_t alloc_size;
	size_t bytes_received;
};

struct sq_ble_ots_pending_event {
	bool active;
	char app_id[SQ_BLE_OTS_APP_ID_MAX];
	char event[SQ_VM_RUNTIME_EVENT_LEN];
	char staging_path[SQ_BLE_OTS_PATH_MAX];
};

static struct sq_ble_ots_session sq_ble_ots_session;
static struct sq_ble_ots_pending_event sq_ble_ots_pending;

static int sq_ble_ots_format_staging_path(char *out, size_t out_len, const char *app_id,
					  const char *profile_id)
{
	int written;

	if (out == NULL || app_id == NULL || profile_id == NULL || out_len == 0) {
		return -EINVAL;
	}
	written = snprintf(out, out_len, "%s/ble-object-%s-%s.tmp", SQ_BLE_OTS_STAGING_DIR,
			   app_id, profile_id);
	if (written < 0 || (size_t)written >= out_len) {
		return -ENOSPC;
	}
	return 0;
}

static void sq_ble_ots_close_session_files(void)
{
	if (sq_ble_ots_session.staging_path[0] != '\0') {
		(void)fs_unlink(sq_ble_ots_session.staging_path);
		sq_ble_ots_session.staging_path[0] = '\0';
	}
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

static int sq_ble_ots_open_staging_file(struct sq_ble_ots_session *session)
{
	struct fs_file_t file;
	int result;

	fs_file_t_init(&file);
	result = fs_open(&file, session->staging_path, FS_O_CREATE | FS_O_WRITE | FS_O_TRUNC);
	if (result != 0) {
		return result;
	}
	return fs_close(&file);
}

static int sq_ble_ots_obj_created_internal(const char *name, size_t alloc_size)
{
	char app_id[SQ_BLE_OTS_APP_ID_MAX] = {0};
	char profile_id[SQ_BLE_OTS_PROFILE_ID_MAX] = {0};
	char extension[16] = {0};
	int result;

	if (sq_ble_ots_session.active) {
		return BT_GATT_OTS_OACP_RES_OBJ_LOCKED;
	}
	result = sq_ble_ots_parse_object_name(name, app_id, sizeof(app_id), profile_id,
					      sizeof(profile_id), extension, sizeof(extension));
	if (result != 0) {
		return result;
	}

	memset(&sq_ble_ots_session, 0, sizeof(sq_ble_ots_session));
	strncpy(sq_ble_ots_session.app_id, app_id,
		sizeof(sq_ble_ots_session.app_id) - 1);
	strncpy(sq_ble_ots_session.profile_id, profile_id,
		sizeof(sq_ble_ots_session.profile_id) - 1);

	result = sq_ble_ots_format_staging_path(sq_ble_ots_session.staging_path,
						sizeof(sq_ble_ots_session.staging_path), app_id,
						profile_id);
	if (result != 0) {
		return BT_GATT_OTS_OACP_RES_INV_PARAM;
	}

	result = sq_ble_ots_open_staging_file(&sq_ble_ots_session);
	if (result != 0) {
		sq_ble_ots_session.staging_path[0] = '\0';
		return result;
	}

	sq_ble_ots_session.active = true;
	sq_ble_ots_session.alloc_size = alloc_size;
	sq_ble_ots_session.bytes_received = 0;

	LOG_INF("obj_created app=%s profile=%s path=%s alloc=%zu", app_id, profile_id,
		sq_ble_ots_session.staging_path, alloc_size);
	return 0;
}

static int sq_ble_ots_obj_write_internal(const char *staging_path, const void *data, size_t len,
					off_t offset, size_t rem)
{
	struct fs_file_t file;
	ssize_t written;
	int result;

	if (!sq_ble_ots_session.active || staging_path == NULL) {
		return -EINVAL;
	}
	if (strcmp(staging_path, sq_ble_ots_session.staging_path) != 0) {
		return -EINVAL;
	}
	/* Reject writes that would push the object past its declared size, so a
	 * misbehaving client cannot overrun the staging file / fill the FS.
	 */
	if ((size_t)offset + len > sq_ble_ots_session.alloc_size) {
		return -EFBIG;
	}

	fs_file_t_init(&file);
	result = fs_open(&file, staging_path, FS_O_WRITE);
	if (result != 0) {
		return result;
	}
	if (offset > 0) {
		result = fs_seek(&file, offset, FS_SEEK_SET);
		if (result != 0) {
			(void)fs_close(&file);
			return result;
		}
	}
	written = fs_write(&file, data, len);
	(void)fs_close(&file);
	if (written < 0) {
		return (int)written;
	}
	if ((size_t)written != len) {
		return -EIO;
	}
	sq_ble_ots_session.bytes_received += len;

	if (rem == 0) {
		memset(&sq_ble_ots_pending, 0, sizeof(sq_ble_ots_pending));
		sq_ble_ots_pending.active = true;
		strncpy(sq_ble_ots_pending.app_id, sq_ble_ots_session.app_id,
			sizeof(sq_ble_ots_pending.app_id) - 1);
		strncpy(sq_ble_ots_pending.event, "ble.object.complete",
			sizeof(sq_ble_ots_pending.event) - 1);
		strncpy(sq_ble_ots_pending.staging_path, sq_ble_ots_session.staging_path,
			sizeof(sq_ble_ots_pending.staging_path) - 1);
		LOG_INF("obj_write complete: pending event app=%s", sq_ble_ots_pending.app_id);
	}

	return (int)written;
}

void sq_ble_ots_reset_session(void)
{
	if (sq_ble_ots_session.active) {
		LOG_INF("reset_session: clearing in-flight app=%s profile=%s",
			sq_ble_ots_session.app_id, sq_ble_ots_session.profile_id);
	}
	sq_ble_ots_close_session_files();
	memset(&sq_ble_ots_session, 0, sizeof(sq_ble_ots_session));

	/* Unlink any pending staging file regardless of the active flag: a drained
	 * (consumed) transfer leaves the file behind for the app to install from,
	 * and a disconnect must still clean it up.
	 */
	if (sq_ble_ots_pending.staging_path[0] != '\0') {
		(void)fs_unlink(sq_ble_ots_pending.staging_path);
	}
	memset(&sq_ble_ots_pending, 0, sizeof(sq_ble_ots_pending));
}

static void sq_ble_ots_abort_internal(void)
{
	if (sq_ble_ots_session.active) {
		LOG_INF("abort: clearing in-flight app=%s profile=%s",
			sq_ble_ots_session.app_id, sq_ble_ots_session.profile_id);
	}
	sq_ble_ots_close_session_files();
	memset(&sq_ble_ots_session, 0, sizeof(sq_ble_ots_session));
}

int sq_ble_ots_test_invoke_obj_created_with_name(const char *name, size_t alloc_size,
						 char *staging_path_out,
						 size_t staging_path_out_len)
{
	int result = sq_ble_ots_obj_created_internal(name, alloc_size);

	if (result != 0) {
		return result;
	}
	if (staging_path_out != NULL && staging_path_out_len > 0) {
		strncpy(staging_path_out, sq_ble_ots_session.staging_path,
			staging_path_out_len - 1);
		staging_path_out[staging_path_out_len - 1] = '\0';
	}
	return 0;
}

int sq_ble_ots_test_invoke_obj_write_with_path(const char *staging_path, const void *data,
					       size_t len, off_t offset, size_t rem)
{
	return sq_ble_ots_obj_write_internal(staging_path, data, len, offset, rem);
}

void sq_ble_ots_test_invoke_abort(void)
{
	sq_ble_ots_abort_internal();
}

bool sq_ble_ots_pending_is_complete(void)
{
	return sq_ble_ots_pending.active;
}

const char *sq_ble_ots_pending_app_id(void)
{
	return sq_ble_ots_pending.app_id;
}

const char *sq_ble_ots_pending_event_name(void)
{
	return sq_ble_ots_pending.event;
}

int sq_ble_ots_drain_pending_event(char *app_id_out, size_t app_id_cap, char *event_out,
				   size_t event_cap)
{
	if (!sq_ble_ots_pending.active) {
		return -ENOENT;
	}
	if (app_id_out != NULL && app_id_cap > 0) {
		strncpy(app_id_out, sq_ble_ots_pending.app_id, app_id_cap - 1);
		app_id_out[app_id_cap - 1] = '\0';
	}
	if (event_out != NULL && event_cap > 0) {
		strncpy(event_out, sq_ble_ots_pending.event, event_cap - 1);
		event_out[event_cap - 1] = '\0';
	}
	/* Consume-once: a polled caller would otherwise re-dispatch the same
	 * event every iteration. The app_id/event/staging_path stay populated so
	 * the caller can still deliver the staging path and clean it up
	 * (cleanup_staging / reset_session) after the handler has installed.
	 */
	sq_ble_ots_pending.active = false;
	return 0;
}

const char *sq_ble_ots_pending_staging_path(void)
{
	return sq_ble_ots_pending.staging_path;
}

void sq_ble_ots_cleanup_staging(void)
{
	if (sq_ble_ots_pending.staging_path[0] != '\0') {
		(void)fs_unlink(sq_ble_ots_pending.staging_path);
	}
	memset(&sq_ble_ots_pending, 0, sizeof(sq_ble_ots_pending));
}
