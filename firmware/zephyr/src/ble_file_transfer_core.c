#include "ble_file_transfer_core.h"

#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <zephyr/fs/fs.h>
#include <zephyr/logging/log.h>

#include "app_store.h"
#include "ble_profile_table.h"
#include "sq_errno.h"
#include "vm_runtime.h"

LOG_MODULE_REGISTER(squidscript_ble_transfer, LOG_LEVEL_INF);

#define SQ_BLE_FILE_TRANSFER_PROFILE_ID_MAX  32
#define SQ_BLE_FILE_TRANSFER_PATH_MAX        SQ_APP_STORE_PATH_MAX
#define SQ_BLE_FILE_TRANSFER_NAME_MAX        96

#ifndef SQ_BLE_FILE_TRANSFER_STAGING_DIR
#define SQ_BLE_FILE_TRANSFER_STAGING_DIR     "/sq/tmp"
#endif

struct sq_ble_file_transfer_session {
	bool active;
	uint8_t app_slot;
	char instance_id[SQ_BLE_FILE_TRANSFER_PROFILE_ID_MAX];
	char complete_event[SQ_VM_RUNTIME_EVENT_LEN];
	char file_name[SQ_BLE_FILE_TRANSFER_NAME_MAX];
	char staging_path[SQ_BLE_FILE_TRANSFER_PATH_MAX];
	struct fs_file_t staging_file;
	bool staging_file_open;
	size_t alloc_size;
	size_t bytes_received;
};

struct sq_ble_file_transfer_pending_event {
	bool active;
	uint8_t app_slot;
	char instance_id[SQ_BLE_FILE_TRANSFER_PROFILE_ID_MAX];
	char event[SQ_VM_RUNTIME_EVENT_LEN];
	char file_name[SQ_BLE_FILE_TRANSFER_NAME_MAX];
	char staging_path[SQ_BLE_FILE_TRANSFER_PATH_MAX];
	size_t bytes_received;
	size_t total_bytes;
};

static struct sq_ble_file_transfer_session sq_ble_file_transfer_session;
static struct sq_ble_file_transfer_pending_event sq_ble_file_transfer_pending;
static const struct sq_app_registry *sq_ble_file_transfer_registry;
static const char *sq_ble_file_transfer_fallback_app_id;
static sq_ble_file_transfer_error_sink sq_ble_file_transfer_error_cb;
static void *sq_ble_file_transfer_error_user_data;

/* Framing state for the opcode transport: the file name arrives as a run of
 * name bytes (across NAME writes) followed by content bytes. */
static struct {
	bool active;
	bool name_done;
	size_t total_size;
	size_t name_len;
	size_t name_got;
	size_t content_off;
	char name[SQ_BLE_FILE_TRANSFER_NAME_MAX];
} sq_ble_framed;

void sq_ble_file_transfer_set_registry(const struct sq_app_registry *registry)
{
	sq_ble_file_transfer_registry = registry;
}

void sq_ble_file_transfer_set_fallback_app_id(const char *app_id)
{
	sq_ble_file_transfer_fallback_app_id = app_id;
}

void sq_ble_file_transfer_set_error_sink(sq_ble_file_transfer_error_sink sink, void *user_data)
{
	sq_ble_file_transfer_error_cb = sink;
	sq_ble_file_transfer_error_user_data = user_data;
}

static void sq_ble_file_transfer_record_invariant(const char *name, int code)
{
	char line[SQ_VM_RUNTIME_DEVICE_ERROR_LEN];

	if (sq_ble_file_transfer_error_cb == NULL || name == NULL) {
		return;
	}
	(void)snprintf(line, sizeof(line), "invariant.%s code=%d (%s)", name, code,
		       sq_errno_name(code));
	sq_ble_file_transfer_error_cb(sq_ble_file_transfer_error_user_data, line);
}

static const char *sq_ble_file_transfer_app_id_for_slot(uint8_t app_slot)
{
	if (app_slot == SQ_APP_REGISTRY_SLOT_FALLBACK) {
		return sq_ble_file_transfer_fallback_app_id;
	}
	return sq_app_registry_app_id_at(sq_ble_file_transfer_registry, app_slot);
}

static int sq_ble_file_transfer_format_staging_path(char *out, size_t out_len, uint8_t app_slot,
					  const char *profile_id)
{
	int written;

	if (out == NULL || app_slot == SQ_APP_REGISTRY_SLOT_INVALID || profile_id == NULL ||
	    out_len == 0) {
		return -EINVAL;
	}
	written = snprintf(out, out_len, "%s/ble-xfer-s%u-%s.tmp",
			   SQ_BLE_FILE_TRANSFER_STAGING_DIR, (unsigned int)app_slot,
			   profile_id);
	if (written < 0 || (size_t)written >= out_len) {
		return -ENOSPC;
	}
	return 0;
}

static void sq_ble_file_transfer_close_session_files(void)
{
	if (sq_ble_file_transfer_session.staging_file_open) {
		(void)fs_close(&sq_ble_file_transfer_session.staging_file);
		sq_ble_file_transfer_session.staging_file_open = false;
	}
	if (sq_ble_file_transfer_session.staging_path[0] != '\0') {
		(void)fs_unlink(sq_ble_file_transfer_session.staging_path);
		sq_ble_file_transfer_session.staging_path[0] = '\0';
	}
}

int sq_ble_file_transfer_parse_file_name(const char *name, char *extension_out, size_t extension_cap)
{
	const char *extension;
	size_t name_len;
	size_t extension_len;

	if (name == NULL || extension_out == NULL) {
		return -EINVAL;
	}
	name_len = strlen(name);
	if (name_len == 0 || name_len >= SQ_BLE_FILE_TRANSFER_NAME_MAX ||
	    strchr(name, '/') != NULL || strchr(name, '\\') != NULL) {
		return SQ_BLE_FILE_TRANSFER_RES_INV_PARAM;
	}
	extension = strrchr(name, '.');
	if (extension == NULL || extension[1] == '\0') {
		return SQ_BLE_FILE_TRANSFER_RES_INV_PARAM;
	}
	extension_len = strlen(extension);
	if (extension_len >= extension_cap) {
		return SQ_BLE_FILE_TRANSFER_RES_INV_PARAM;
	}

	memcpy(extension_out, extension, extension_len + 1);

	return 0;
}

static int sq_ble_file_transfer_open_staging_file(struct sq_ble_file_transfer_session *session)
{
	const char *slash;
	int result;

	/* Ensure the parent directory exists (e.g. /sq/tmp on a fresh device,
	 * before any other staged operation has created it).
	 */
	slash = strrchr(session->staging_path, '/');
	if (slash != NULL && slash != session->staging_path) {
		char dir[SQ_BLE_FILE_TRANSFER_PATH_MAX];
		size_t dir_len = (size_t)(slash - session->staging_path);

		if (dir_len < sizeof(dir)) {
			memcpy(dir, session->staging_path, dir_len);
			dir[dir_len] = '\0';
			result = fs_mkdir(dir);
			if (result != 0 && result != -EEXIST) {
				return result;
			}
		}
	}

	fs_file_t_init(&session->staging_file);
	result = fs_open(&session->staging_file, session->staging_path,
			 FS_O_CREATE | FS_O_WRITE | FS_O_TRUNC);
	if (result != 0) {
		return result;
	}
	session->staging_file_open = true;
	return 0;
}

static int sq_ble_file_transfer_begin_internal(const char *name, size_t alloc_size)
{
	char extension[16] = {0};
	const struct sq_ble_profile_entry *profile;
	const char *app_id;
	int result;

	if (sq_ble_file_transfer_session.active) {
		return SQ_BLE_FILE_TRANSFER_RES_BUSY;
	}
	result = sq_ble_file_transfer_parse_file_name(name, extension, sizeof(extension));
	if (result != 0) {
		return result;
	}
	result = sq_ble_profile_lookup_accepting_extension_result(extension, &profile);
	if (result == -EEXIST) {
		sq_ble_file_transfer_record_invariant("ble.route_ambiguous", -EEXIST);
		return SQ_BLE_FILE_TRANSFER_RES_ROUTE_AMBIGUOUS;
	}
	if (result != 0) {
		return SQ_BLE_FILE_TRANSFER_RES_INV_PARAM;
	}
	app_id = sq_ble_file_transfer_app_id_for_slot(profile->app_slot);
	if (app_id == NULL) {
		sq_ble_file_transfer_record_invariant("ble.route_stale", -ENOENT);
		return SQ_BLE_FILE_TRANSFER_RES_INV_PARAM;
	}

	memset(&sq_ble_file_transfer_session, 0, sizeof(sq_ble_file_transfer_session));
	sq_ble_file_transfer_session.app_slot = profile->app_slot;
	strncpy(sq_ble_file_transfer_session.instance_id, profile->instance_id,
		sizeof(sq_ble_file_transfer_session.instance_id) - 1);
	strncpy(sq_ble_file_transfer_session.complete_event, profile->complete_event,
		sizeof(sq_ble_file_transfer_session.complete_event) - 1);
	strncpy(sq_ble_file_transfer_session.file_name, name,
		sizeof(sq_ble_file_transfer_session.file_name) - 1);

	result = sq_ble_file_transfer_format_staging_path(
		sq_ble_file_transfer_session.staging_path,
		sizeof(sq_ble_file_transfer_session.staging_path), profile->app_slot,
		profile->instance_id);
	if (result != 0) {
		return SQ_BLE_FILE_TRANSFER_RES_INV_PARAM;
	}

	result = sq_ble_file_transfer_open_staging_file(&sq_ble_file_transfer_session);
	if (result != 0) {
		sq_ble_file_transfer_session.staging_path[0] = '\0';
		return result;
	}

	sq_ble_file_transfer_session.active = true;
	sq_ble_file_transfer_session.alloc_size = alloc_size;
	sq_ble_file_transfer_session.bytes_received = 0;

	LOG_INF("begin app=%s profile=%s path=%s alloc=%zu", app_id, profile->instance_id,
		sq_ble_file_transfer_session.staging_path, alloc_size);
	return 0;
}

static int sq_ble_file_transfer_write_internal(const char *staging_path, const void *data, size_t len,
					off_t offset, size_t rem)
{
	ssize_t written;
	int result;

	if (!sq_ble_file_transfer_session.active || staging_path == NULL) {
		return -EINVAL;
	}
	if (strcmp(staging_path, sq_ble_file_transfer_session.staging_path) != 0) {
		return -EINVAL;
	}
	/* Reject writes that would push the file past its declared size, so a
	 * misbehaving client cannot overrun the staging file / fill the FS.
	 */
	if ((size_t)offset + len > sq_ble_file_transfer_session.alloc_size) {
		return -EFBIG;
	}

	if (!sq_ble_file_transfer_session.staging_file_open) {
		return -EIO;
	}
	result = fs_seek(&sq_ble_file_transfer_session.staging_file, offset, FS_SEEK_SET);
	if (result != 0) {
		return result;
	}
	written = fs_write(&sq_ble_file_transfer_session.staging_file, data, len);
	if (written < 0) {
		return (int)written;
	}
	if ((size_t)written != len) {
		return -EIO;
	}
	sq_ble_file_transfer_session.bytes_received += len;

	if (rem == 0) {
		/* Fill all fields first, then publish by setting `active` LAST. The
		 * poll thread reads `active` (sq_ble_file_transfer_pending_is_complete) before
		 * draining, so on the single-core ESP32-C3 it either sees the
		 * pre-publish (inactive) state or the fully populated one -- never a
		 * half-written pending. A producer write callback and the poll-loop
		 * consumer run on different threads but the same core.
		 */
		memset(&sq_ble_file_transfer_pending, 0, sizeof(sq_ble_file_transfer_pending));
		sq_ble_file_transfer_pending.app_slot = sq_ble_file_transfer_session.app_slot;
		strncpy(sq_ble_file_transfer_pending.instance_id,
			sq_ble_file_transfer_session.instance_id,
			sizeof(sq_ble_file_transfer_pending.instance_id) - 1);
		strncpy(sq_ble_file_transfer_pending.event, sq_ble_file_transfer_session.complete_event,
			sizeof(sq_ble_file_transfer_pending.event) - 1);
		strncpy(sq_ble_file_transfer_pending.file_name, sq_ble_file_transfer_session.file_name,
			sizeof(sq_ble_file_transfer_pending.file_name) - 1);
		strncpy(sq_ble_file_transfer_pending.staging_path, sq_ble_file_transfer_session.staging_path,
			sizeof(sq_ble_file_transfer_pending.staging_path) - 1);
		sq_ble_file_transfer_pending.bytes_received = sq_ble_file_transfer_session.bytes_received;
		sq_ble_file_transfer_pending.total_bytes = sq_ble_file_transfer_session.alloc_size;
		if (sq_ble_file_transfer_session.staging_file_open) {
			(void)fs_close(&sq_ble_file_transfer_session.staging_file);
			sq_ble_file_transfer_session.staging_file_open = false;
		}
		sq_ble_file_transfer_pending.active = true;
		/* Ownership of the staging file moves to the pending event (the
		 * consumer pipeline). Detach it from the session so session teardown
		 * on disconnect (close_session_files / reset_session) does not unlink
		 * the handed-off file out from under the deferred install. */
		sq_ble_file_transfer_session.staging_path[0] = '\0';
		LOG_INF("write complete: pending event app_slot=%u",
			(unsigned int)sq_ble_file_transfer_pending.app_slot);
	}

	return (int)written;
}

void sq_ble_file_transfer_reset_session(void)
{
	if (sq_ble_file_transfer_session.active) {
		LOG_INF("reset_session: clearing in-flight app_slot=%u profile=%s",
			(unsigned int)sq_ble_file_transfer_session.app_slot,
			sq_ble_file_transfer_session.instance_id);
	}
	sq_ble_file_transfer_close_session_files();
	memset(&sq_ble_file_transfer_session, 0, sizeof(sq_ble_file_transfer_session));
	memset(&sq_ble_framed, 0, sizeof(sq_ble_framed));
}

static void sq_ble_file_transfer_abort_internal(void)
{
	if (sq_ble_file_transfer_session.active) {
		LOG_INF("abort: clearing in-flight app_slot=%u profile=%s",
			(unsigned int)sq_ble_file_transfer_session.app_slot,
			sq_ble_file_transfer_session.instance_id);
	}
	memset(&sq_ble_framed, 0, sizeof(sq_ble_framed));
	sq_ble_file_transfer_close_session_files();
	memset(&sq_ble_file_transfer_session, 0, sizeof(sq_ble_file_transfer_session));
}

/*
 * Public transport-facing API used by a BLE transport front-end (the custom
 * GATT service): set up a framed transfer, feed the file name and content,
 * and abort on cancel/error.
 */
void sq_ble_file_transfer_abort(void)
{
	sq_ble_file_transfer_abort_internal();
}

int sq_ble_file_transfer_begin_framed(size_t total_size, size_t name_len)
{
	if (name_len == 0 || name_len >= sizeof(sq_ble_framed.name)) {
		return SQ_BLE_FILE_TRANSFER_RES_INV_PARAM;
	}
	if (total_size == 0 || total_size > SQ_DEVICE_INSTALL_MAX_BYTES) {
		return SQ_BLE_FILE_TRANSFER_RES_INV_PARAM;
	}
	if (sq_ble_file_transfer_session.active || sq_ble_framed.active) {
		return SQ_BLE_FILE_TRANSFER_RES_BUSY;
	}
	memset(&sq_ble_framed, 0, sizeof(sq_ble_framed));
	sq_ble_framed.active = true;
	sq_ble_framed.total_size = total_size;
	sq_ble_framed.name_len = name_len;
	return 0;
}

int sq_ble_file_transfer_feed_name(const void *data, size_t len)
{
	int result;

	if (!sq_ble_framed.active || sq_ble_framed.name_done || data == NULL) {
		return -EINVAL;
	}
	if (sq_ble_framed.name_got + len > sq_ble_framed.name_len) {
		return SQ_BLE_FILE_TRANSFER_RES_INV_PARAM;
	}
	memcpy(sq_ble_framed.name + sq_ble_framed.name_got, data, len);
	sq_ble_framed.name_got += len;
	if (sq_ble_framed.name_got == sq_ble_framed.name_len) {
		sq_ble_framed.name[sq_ble_framed.name_len] = '\0';
		result = sq_ble_file_transfer_begin_internal(sq_ble_framed.name,
							 sq_ble_framed.total_size);
		if (result != 0) {
			memset(&sq_ble_framed, 0, sizeof(sq_ble_framed));
			return result;
		}
		sq_ble_framed.name_done = true;
	}
	return 0;
}

int sq_ble_file_transfer_feed_content(const void *data, size_t len)
{
	size_t rem;
	int result;

	if (!sq_ble_framed.active || !sq_ble_framed.name_done || data == NULL) {
		return -EINVAL;
	}
	if (sq_ble_framed.content_off + len > sq_ble_framed.total_size) {
		return -EFBIG;
	}
	rem = sq_ble_framed.total_size - (sq_ble_framed.content_off + len);
	result = sq_ble_file_transfer_write_internal(sq_ble_file_transfer_session.staging_path, data, len,
					       (off_t)sq_ble_framed.content_off, rem);
	if (result < 0) {
		return result;
	}
	sq_ble_framed.content_off += len;
	if (rem == 0) {
		sq_ble_framed.active = false;
	}
	return 0;
}

int sq_ble_file_transfer_test_invoke_begin_with_name(const char *name, size_t alloc_size,
						 char *staging_path_out,
						 size_t staging_path_out_len)
{
	int result = sq_ble_file_transfer_begin_internal(name, alloc_size);

	if (result != 0) {
		return result;
	}
	if (staging_path_out != NULL && staging_path_out_len > 0) {
		strncpy(staging_path_out, sq_ble_file_transfer_session.staging_path,
			staging_path_out_len - 1);
		staging_path_out[staging_path_out_len - 1] = '\0';
	}
	return 0;
}

int sq_ble_file_transfer_test_invoke_write_with_path(const char *staging_path, const void *data,
					       size_t len, off_t offset, size_t rem)
{
	return sq_ble_file_transfer_write_internal(staging_path, data, len, offset, rem);
}

void sq_ble_file_transfer_test_invoke_abort(void)
{
	sq_ble_file_transfer_abort_internal();
}

bool sq_ble_file_transfer_pending_is_complete(void)
{
	return sq_ble_file_transfer_pending.active;
}

const char *sq_ble_file_transfer_pending_app_id(void)
{
	return sq_ble_file_transfer_app_id_for_slot(sq_ble_file_transfer_pending.app_slot);
}

const char *sq_ble_file_transfer_pending_event_name(void)
{
	return sq_ble_file_transfer_pending.event;
}

int sq_ble_file_transfer_drain_pending_event(char *app_id_out, size_t app_id_cap, char *event_out,
				   size_t event_cap)
{
	if (!sq_ble_file_transfer_pending.active) {
		return -ENOENT;
	}
	if (app_id_out != NULL && app_id_cap > 0) {
		const char *app_id = sq_ble_file_transfer_pending_app_id();

		if (app_id == NULL) {
			return -EINVAL;
		}
		strncpy(app_id_out, app_id, app_id_cap - 1);
		app_id_out[app_id_cap - 1] = '\0';
	}
	if (event_out != NULL && event_cap > 0) {
		strncpy(event_out, sq_ble_file_transfer_pending.event, event_cap - 1);
		event_out[event_cap - 1] = '\0';
	}
	/* Consume-once: a polled caller would otherwise re-dispatch the same
	 * event every iteration. The app_id/event/staging_path stay populated so
	 * the caller can still deliver the staging path and clean it up
	 * (cleanup_staging / reset_session) after the handler has installed.
	 */
	sq_ble_file_transfer_pending.active = false;
	return 0;
}

const char *sq_ble_file_transfer_pending_staging_path(void)
{
	return sq_ble_file_transfer_pending.staging_path;
}

const char *sq_ble_file_transfer_pending_profile_id(void)
{
	return sq_ble_file_transfer_pending.instance_id;
}

const char *sq_ble_file_transfer_pending_file_name(void)
{
	return sq_ble_file_transfer_pending.file_name;
}

size_t sq_ble_file_transfer_pending_bytes_received(void)
{
	return sq_ble_file_transfer_pending.bytes_received;
}

size_t sq_ble_file_transfer_pending_total_bytes(void)
{
	return sq_ble_file_transfer_pending.total_bytes;
}

void sq_ble_file_transfer_cleanup_staging(void)
{
	if (sq_ble_file_transfer_pending.staging_path[0] != '\0') {
		(void)fs_unlink(sq_ble_file_transfer_pending.staging_path);
	}
	memset(&sq_ble_file_transfer_pending, 0, sizeof(sq_ble_file_transfer_pending));
}
