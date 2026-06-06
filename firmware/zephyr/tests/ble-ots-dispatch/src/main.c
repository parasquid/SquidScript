#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include <zephyr/bluetooth/services/ots.h>
#include <zephyr/fs/fs.h>
#include <zephyr/kernel.h>
#include <zephyr/sys/util.h>
#include <zephyr/ztest.h>

#include "app_store.h"
#include "ble_ots.h"

static struct fs_mount_t test_fs_mount = {
	.type = FS_NATIVE_MOUNT,
	.mnt_point = "/sqtest",
	.fs_data = TEST_FS_DIR,
};

static int mount_test_fs(void)
{
	int result = fs_mount(&test_fs_mount);

	return result == -EALREADY ? 0 : result;
}

static int unmount_test_fs(void)
{
	int result = fs_unmount(&test_fs_mount);

	return result == -EINVAL ? 0 : result;
}

static int format_test_fs(void)
{
	int result = fs_mkdir("/sqtest/tmp");

	return (result == 0 || result == -EEXIST) ? 0 : result;
}

static bool staging_file_exists(const char *path)
{
	struct fs_dirent entry;
	int result = fs_stat(path, &entry);

	return result == 0;
}

static const uint8_t e2e_sqbc_magic[] = {'S', 'Q', 'B', 'C'};

/* Deterministic SQBC content: magic followed by (pos & 0xFF) so a byte-exact
 * readback after a multi-chunk transfer + streaming install detects truncation.
 */
static uint8_t e2e_generated_byte_at(size_t pos)
{
	return pos < sizeof(e2e_sqbc_magic) ? e2e_sqbc_magic[pos] : (uint8_t)(pos & 0xFFu);
}

static void *ble_ots_dispatch_setup(void)
{
	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_fs(), 0, "format failed");
	return NULL;
}

static void ble_ots_dispatch_before(void *fixture)
{
	(void)fixture;
	sq_ble_ots_reset_session();
	zassert_equal(unmount_test_fs(), 0, "unmount failed");
	zassert_equal(mount_test_fs(), 0, "remount failed");
	zassert_equal(format_test_fs(), 0, "format failed");
	zassert_equal(sq_ble_ots_init(), 0, "sq_ble_ots_init failed");
}

static void ble_ots_dispatch_teardown(void *fixture)
{
	(void)fixture;
	sq_ble_ots_reset_session();
	(void)unmount_test_fs();
}

ZTEST_SUITE(ble_ots_dispatch, NULL, ble_ots_dispatch_setup, ble_ots_dispatch_before, NULL,
	    ble_ots_dispatch_teardown);

ZTEST(ble_ots_dispatch, test_completed_write_populates_pending_slot)
{
	char staging_path[128] = {0};
	const uint8_t chunk[] = {'S', 'Q', 'B', 'C'};
	int result;

	result = sq_ble_ots_test_invoke_obj_created_with_name("break-reminder/wallpaper/.sqbc",
							      4096, staging_path,
							      sizeof(staging_path));
	zassert_equal(result, 0, "obj_created failed: %d", result);

	result = sq_ble_ots_test_invoke_obj_write_with_path(staging_path, chunk, sizeof(chunk), 0,
							    0);
	zassert_equal(result, (int)sizeof(chunk), "obj_write final expected %zu, got %d",
		      sizeof(chunk), result);

	zassert_true(sq_ble_ots_pending_is_complete(), "pending slot should be complete");
	zassert_str_equal(sq_ble_ots_pending_app_id(), "break-reminder");
	zassert_str_equal(sq_ble_ots_pending_event_name(), "ble.object.complete");
}

ZTEST(ble_ots_dispatch, test_drain_returns_app_id_and_event)
{
	char staging_path[128] = {0};
	const uint8_t chunk[] = {'S', 'Q', 'B', 'C'};
	char drained_app_id[SQ_APP_STORE_APP_ID_MAX] = {0};
	char drained_event[SQ_VM_RUNTIME_EVENT_LEN] = {0};
	int result;

	result = sq_ble_ots_test_invoke_obj_created_with_name("break-reminder/wallpaper/.sqbc",
							      4096, staging_path,
							      sizeof(staging_path));
	zassert_equal(result, 0, "obj_created failed: %d", result);
	result = sq_ble_ots_test_invoke_obj_write_with_path(staging_path, chunk, sizeof(chunk), 0,
							    0);
	zassert_equal(result, (int)sizeof(chunk));

	result = sq_ble_ots_drain_pending_event(drained_app_id, sizeof(drained_app_id),
					       drained_event, sizeof(drained_event));
	zassert_equal(result, 0, "drain expected 0, got %d", result);
	zassert_str_equal(drained_app_id, "break-reminder", "drained app_id mismatch");
	zassert_str_equal(drained_event, "ble.object.complete", "drained event mismatch");
}

ZTEST(ble_ots_dispatch, test_drain_is_consume_once)
{
	char staging_path[128] = {0};
	const uint8_t chunk[] = {'S', 'Q', 'B', 'C'};
	char app_id[SQ_APP_STORE_APP_ID_MAX] = {0};
	char event[SQ_VM_RUNTIME_EVENT_LEN] = {0};
	int result;

	result = sq_ble_ots_test_invoke_obj_created_with_name("break-reminder/wallpaper/.sqbc",
							      4096, staging_path,
							      sizeof(staging_path));
	zassert_equal(result, 0);
	result = sq_ble_ots_test_invoke_obj_write_with_path(staging_path, chunk, sizeof(chunk), 0,
							    0);
	zassert_equal(result, (int)sizeof(chunk));
	zassert_true(sq_ble_ots_pending_is_complete());

	/* First drain succeeds and consumes the event so a poll loop won't refire it. */
	result = sq_ble_ots_drain_pending_event(app_id, sizeof(app_id), event, sizeof(event));
	zassert_equal(result, 0);
	zassert_false(sq_ble_ots_pending_is_complete(), "drain should consume the pending event");

	/* Second drain (without cleanup) is empty. */
	result = sq_ble_ots_drain_pending_event(app_id, sizeof(app_id), event, sizeof(event));
	zassert_equal(result, -ENOENT, "second drain should return -ENOENT, got %d", result);

	/* The staging path is retained after drain so the handler can still install. */
	zassert_true(sq_ble_ots_pending_staging_path()[0] != '\0',
		     "staging path should survive drain for the install step");
	zassert_true(staging_file_exists(sq_ble_ots_pending_staging_path()),
		     "staging file should survive drain");

	sq_ble_ots_cleanup_staging();
}

ZTEST(ble_ots_dispatch, test_cleanup_clears_pending_slot)
{
	char staging_path[128] = {0};
	const uint8_t chunk[] = {'S', 'Q', 'B', 'C'};
	char drained_app_id[SQ_APP_STORE_APP_ID_MAX] = {0};
	char drained_event[SQ_VM_RUNTIME_EVENT_LEN] = {0};
	int result;

	result = sq_ble_ots_test_invoke_obj_created_with_name("break-reminder/wallpaper/.sqbc",
							      4096, staging_path,
							      sizeof(staging_path));
	zassert_equal(result, 0);
	result = sq_ble_ots_test_invoke_obj_write_with_path(staging_path, chunk, sizeof(chunk), 0,
							    0);
	zassert_equal(result, (int)sizeof(chunk));

	result = sq_ble_ots_drain_pending_event(drained_app_id, sizeof(drained_app_id),
					       drained_event, sizeof(drained_event));
	zassert_equal(result, 0);

	sq_ble_ots_cleanup_staging();
	zassert_false(sq_ble_ots_pending_is_complete(),
		      "cleanup_staging should clear pending slot");

	result = sq_ble_ots_drain_pending_event(drained_app_id, sizeof(drained_app_id),
					       drained_event, sizeof(drained_event));
	zassert_equal(result, -ENOENT, "drain after cleanup should return -ENOENT, got %d", result);
}

ZTEST(ble_ots_dispatch, test_cleanup_staging_unlinks_file)
{
	char staging_path[128] = {0};
	const uint8_t chunk[] = {'S', 'Q', 'B', 'C'};
	char drained_app_id[SQ_APP_STORE_APP_ID_MAX] = {0};
	char drained_event[SQ_VM_RUNTIME_EVENT_LEN] = {0};
	int result;

	result = sq_ble_ots_test_invoke_obj_created_with_name("break-reminder/wallpaper/.sqbc",
							      4096, staging_path,
							      sizeof(staging_path));
	zassert_equal(result, 0);
	result = sq_ble_ots_test_invoke_obj_write_with_path(staging_path, chunk, sizeof(chunk), 0,
							    0);
	zassert_equal(result, (int)sizeof(chunk));
	zassert_true(staging_file_exists(staging_path), "staging file should exist before drain");

	result = sq_ble_ots_drain_pending_event(drained_app_id, sizeof(drained_app_id),
					       drained_event, sizeof(drained_event));
	zassert_equal(result, 0);
	zassert_true(staging_file_exists(staging_path),
		     "staging file should still exist after drain (event handler runs first)");

	sq_ble_ots_cleanup_staging();
	zassert_false(staging_file_exists(staging_path),
		      "staging file should be unlinked after cleanup_staging");
}

ZTEST(ble_ots_dispatch, test_reset_session_clears_pending_slot)
{
	char staging_path[128] = {0};
	const uint8_t chunk[] = {'S', 'Q', 'B', 'C'};
	char drained_app_id[SQ_APP_STORE_APP_ID_MAX] = {0};
	char drained_event[SQ_VM_RUNTIME_EVENT_LEN] = {0};
	int result;

	result = sq_ble_ots_test_invoke_obj_created_with_name("break-reminder/wallpaper/.sqbc",
							      4096, staging_path,
							      sizeof(staging_path));
	zassert_equal(result, 0);
	result = sq_ble_ots_test_invoke_obj_write_with_path(staging_path, chunk, sizeof(chunk), 0,
							    0);
	zassert_equal(result, (int)sizeof(chunk));
	zassert_true(sq_ble_ots_pending_is_complete());

	sq_ble_ots_reset_session();
	zassert_false(sq_ble_ots_pending_is_complete(),
		      "reset_session should clear pending slot");

	result = sq_ble_ots_drain_pending_event(drained_app_id, sizeof(drained_app_id),
					       drained_event, sizeof(drained_event));
	zassert_equal(result, -ENOENT, "drain after reset should return -ENOENT, got %d", result);
}

ZTEST(ble_ots_dispatch, test_end_to_end_ots_to_app_install)
{
	char staging_path[128] = {0};
	const uint8_t valid_sqbc[] = {'S', 'Q', 'B', 'C', 0x01, 0x02, 0x03, 0x04};
	char drained_app_id[SQ_APP_STORE_APP_ID_MAX] = {0};
	char drained_event[SQ_VM_RUNTIME_EVENT_LEN] = {0};
	char installed_path[SQ_APP_STORE_APP_FILE_PATH_MAX];
	struct fs_file_t verify;
	char readback[8] = {0};
	ssize_t bytes_read;
	int result;

	result = sq_ble_ots_test_invoke_obj_created_with_name("installed-app/wallpaper/.sqbc",
							      4096, staging_path,
							      sizeof(staging_path));
	zassert_equal(result, 0, "obj_created failed: %d", result);

	result = sq_ble_ots_test_invoke_obj_write_with_path(staging_path, valid_sqbc,
							    sizeof(valid_sqbc), 0, 0);
	zassert_equal(result, (int)sizeof(valid_sqbc));

	result = sq_ble_ots_drain_pending_event(drained_app_id, sizeof(drained_app_id),
					       drained_event, sizeof(drained_event));
	zassert_equal(result, 0);
	zassert_str_equal(drained_app_id, "installed-app");
	zassert_str_equal(drained_event, "ble.object.complete");

	const char *delivered_path = sq_ble_ots_pending_staging_path();

	zassert_equal(sq_app_store_install_from_file_ref(test_fs_mount.mnt_point, "installed-app",
							delivered_path),
		      0, "install_from_file_ref should succeed with valid SQBC magic");

	zassert_true(snprintf(installed_path, sizeof(installed_path),
			      "%s/apps/installed-app/main.sqbc", test_fs_mount.mnt_point) > 0);
	fs_file_t_init(&verify);
	zassert_equal(fs_open(&verify, installed_path, FS_O_READ), 0,
		      "expected installed SQBC at %s", installed_path);
	bytes_read = fs_read(&verify, readback, sizeof(readback));
	(void)fs_close(&verify);
	zassert_equal(bytes_read, (ssize_t)sizeof(valid_sqbc));
	zassert_mem_equal(readback, valid_sqbc, sizeof(valid_sqbc));

	sq_ble_ots_cleanup_staging();
	zassert_false(staging_file_exists(delivered_path),
		      "staging file should be unlinked after cleanup");
}

/* End-to-end with a multi-chunk, >1 KiB payload: exercises the BLE staging
 * write loop (several obj_write calls at increasing offsets) plus the streaming
 * install, and asserts the installed app is byte-exact. Guards against the
 * 1 KiB install truncation regression through the full OTS->install path.
 */
ZTEST(ble_ots_dispatch, test_end_to_end_multichunk_large_payload)
{
	const size_t total_len = 4096; /* over the former 1 KiB install cap */
	const size_t chunk_len = 512;
	char staging_path[128] = {0};
	uint8_t chunk[512];
	char drained_app_id[SQ_APP_STORE_APP_ID_MAX] = {0};
	char drained_event[SQ_VM_RUNTIME_EVENT_LEN] = {0};
	char installed_path[SQ_APP_STORE_APP_FILE_PATH_MAX];
	struct fs_file_t verify;
	const char *delivered_path;
	size_t offset = 0;
	size_t verify_pos = 0;
	int result;

	result = sq_ble_ots_test_invoke_obj_created_with_name("installed-app/wallpaper/.sqbc",
							      total_len, staging_path,
							      sizeof(staging_path));
	zassert_equal(result, 0, "obj_created failed: %d", result);

	while (offset < total_len) {
		size_t this_len = MIN(chunk_len, total_len - offset);
		size_t rem = total_len - offset - this_len;

		for (size_t i = 0; i < this_len; i++) {
			chunk[i] = e2e_generated_byte_at(offset + i);
		}
		result = sq_ble_ots_test_invoke_obj_write_with_path(staging_path, chunk, this_len,
								    (off_t)offset, rem);
		zassert_equal(result, (int)this_len, "obj_write chunk at %zu failed: %d", offset,
			      result);
		offset += this_len;
	}

	zassert_true(sq_ble_ots_pending_is_complete(), "pending slot should be complete");
	result = sq_ble_ots_drain_pending_event(drained_app_id, sizeof(drained_app_id),
						drained_event, sizeof(drained_event));
	zassert_equal(result, 0, "drain expected 0, got %d", result);
	zassert_str_equal(drained_app_id, "installed-app");

	delivered_path = sq_ble_ots_pending_staging_path();
	zassert_equal(sq_app_store_install_from_file_ref(test_fs_mount.mnt_point, "installed-app",
							 delivered_path),
		      0, "streaming install of multi-chunk payload should succeed");

	zassert_true(snprintf(installed_path, sizeof(installed_path),
			      "%s/apps/installed-app/main.sqbc", test_fs_mount.mnt_point) > 0);
	fs_file_t_init(&verify);
	zassert_equal(fs_open(&verify, installed_path, FS_O_READ), 0,
		      "expected installed SQBC at %s", installed_path);
	for (;;) {
		ssize_t got = fs_read(&verify, chunk, sizeof(chunk));

		zassert_true(got >= 0, "read error %d", (int)got);
		if (got == 0) {
			break;
		}
		for (ssize_t i = 0; i < got; i++) {
			zassert_equal(chunk[i], e2e_generated_byte_at(verify_pos),
				      "installed byte %zu mismatch", verify_pos);
			verify_pos++;
		}
	}
	(void)fs_close(&verify);
	zassert_equal(verify_pos, total_len, "installed length mismatch: got %zu want %zu",
		      verify_pos, total_len);

	sq_ble_ots_cleanup_staging();
}
