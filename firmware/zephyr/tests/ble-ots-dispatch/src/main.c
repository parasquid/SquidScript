#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include <zephyr/bluetooth/services/ots.h>
#include <zephyr/fs/fs.h>
#include <zephyr/kernel.h>
#include <zephyr/ztest.h>

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

ZTEST(ble_ots_dispatch, test_drain_clears_pending_slot)
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

	result = sq_ble_ots_drain_pending_event(drained_app_id, sizeof(drained_app_id),
					       drained_event, sizeof(drained_event));
	zassert_equal(result, -ENOENT, "second drain should return -ENOENT, got %d", result);
}

ZTEST(ble_ots_dispatch, test_drain_unlinks_staging_file)
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
	zassert_false(staging_file_exists(staging_path),
		      "staging file should be unlinked after drain");
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
