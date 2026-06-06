#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include <zephyr/fs/fs.h>
#include <zephyr/kernel.h>
#include <zephyr/ztest.h>

#include "ble_object_transfer.h"

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
	int result;

	result = fs_mkdir("/sqtest/tmp");
	if (result != 0 && result != -EEXIST) {
		return result;
	}
	return 0;
}

static bool staging_file_exists(const char *path)
{
	struct fs_dirent entry;
	int result = fs_stat(path, &entry);

	return result == 0;
}

static void *ble_ots_staging_setup(void)
{
	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_fs(), 0, "format failed");
	return NULL;
}

static void ble_ots_staging_before(void *fixture)
{
	(void)fixture;
	sq_ble_ots_reset_session();
	zassert_equal(unmount_test_fs(), 0, "unmount failed");
	zassert_equal(mount_test_fs(), 0, "remount failed");
	zassert_equal(format_test_fs(), 0, "format failed");
}

static void ble_ots_staging_teardown(void *fixture)
{
	(void)fixture;
	sq_ble_ots_reset_session();
	(void)unmount_test_fs();
}

ZTEST_SUITE(ble_ots_staging, NULL, ble_ots_staging_setup, ble_ots_staging_before, NULL,
	    ble_ots_staging_teardown);

ZTEST(ble_ots_staging, test_obj_created_opens_staging_file)
{
	char staging_path[128] = {0};
	int result;

	result = sq_ble_ots_test_invoke_obj_created_with_name("break-reminder/wallpaper/.sqbc",
							      4096, staging_path,
							      sizeof(staging_path));
	zassert_equal(result, 0, "expected 0 from obj_created, got %d", result);
	zassert_not_null(staging_path, "staging_path should be populated");
	zassert_true(staging_path[0] != '\0', "staging_path should not be empty");
	zassert_true(staging_file_exists(staging_path), "staging file should exist at %s",
		     staging_path);
}

ZTEST(ble_ots_staging, test_obj_write_writes_chunks_to_staging_file)
{
	char staging_path[128] = {0};
	const uint8_t chunk1[] = {'S', 'Q', 'B', 'C'};
	const uint8_t chunk2[] = {0x01, 0x02, 0x03, 0x04};
	struct fs_file_t verify;
	uint8_t readback[8] = {0};
	ssize_t bytes_read;
	int result;

	result = sq_ble_ots_test_invoke_obj_created_with_name("break-reminder/wallpaper/.sqbc",
							      4096, staging_path,
							      sizeof(staging_path));
	zassert_equal(result, 0, "obj_created failed: %d", result);

	result = sq_ble_ots_test_invoke_obj_write_with_path(staging_path, chunk1,
							    sizeof(chunk1), 0, sizeof(chunk2));
	zassert_equal(result, (int)sizeof(chunk1), "obj_write chunk1 expected %zu, got %d",
		      sizeof(chunk1), result);

	result = sq_ble_ots_test_invoke_obj_write_with_path(staging_path, chunk2,
							    sizeof(chunk2), sizeof(chunk1), 0);
	zassert_equal(result, (int)sizeof(chunk2), "obj_write chunk2 expected %zu, got %d",
		      sizeof(chunk2), result);

	fs_file_t_init(&verify);
	zassert_equal(fs_open(&verify, staging_path, FS_O_READ), 0, "open verify failed");
	bytes_read = fs_read(&verify, readback, sizeof(readback));
	(void)fs_close(&verify);
	zassert_equal(bytes_read, (ssize_t)(sizeof(chunk1) + sizeof(chunk2)));
	zassert_mem_equal(readback, chunk1, sizeof(chunk1));
	zassert_mem_equal(readback + sizeof(chunk1), chunk2, sizeof(chunk2));
}

ZTEST(ble_ots_staging, test_reset_session_unlinks_staging_file)
{
	char staging_path[128] = {0};
	int result;

	result = sq_ble_ots_test_invoke_obj_created_with_name("break-reminder/wallpaper/.sqbc",
							      4096, staging_path,
							      sizeof(staging_path));
	zassert_equal(result, 0, "obj_created failed: %d", result);
	zassert_true(staging_file_exists(staging_path), "staging file should exist before reset");

	sq_ble_ots_reset_session();
	zassert_false(staging_file_exists(staging_path),
		      "staging file should be unlinked after reset_session");
}

ZTEST(ble_ots_staging, test_second_create_while_busy_returns_obj_locked)
{
	char staging_path_a[128] = {0};
	char staging_path_b[128] = {0};
	int result;

	result = sq_ble_ots_test_invoke_obj_created_with_name("app-a/wallpaper-a/.sqbc",
							      4096, staging_path_a,
							      sizeof(staging_path_a));
	zassert_equal(result, 0, "first obj_created failed: %d", result);

	result = sq_ble_ots_test_invoke_obj_created_with_name("app-b/wallpaper-b/.sqbc",
							      4096, staging_path_b,
							      sizeof(staging_path_b));
	zassert_equal(result, BT_GATT_OTS_OACP_RES_OBJ_LOCKED,
		      "expected OBJ_LOCKED on second create, got %d", result);
}

ZTEST(ble_ots_staging, test_abort_unlinks_and_clears_in_flight_session)
{
	char staging_path[128] = {0};
	int result;

	result = sq_ble_ots_test_invoke_obj_created_with_name("break-reminder/wallpaper/.sqbc",
							      4096, staging_path,
							      sizeof(staging_path));
	zassert_equal(result, 0, "obj_created failed: %d", result);
	zassert_true(staging_file_exists(staging_path), "staging file should exist before abort");

	sq_ble_ots_test_invoke_abort();
	zassert_false(staging_file_exists(staging_path),
		      "staging file should be unlinked after abort");

	result = sq_ble_ots_test_invoke_obj_created_with_name("break-reminder/wallpaper/.sqbc",
							      4096, staging_path,
							      sizeof(staging_path));
	zassert_equal(result, 0, "create after abort should succeed, got %d", result);
}

ZTEST(ble_ots_staging, test_write_beyond_declared_size_rejected)
{
	char staging_path[128] = {0};
	uint8_t chunk[16] = {0};
	int result;

	result = sq_ble_ots_test_invoke_obj_created_with_name("break-reminder/wallpaper/.sqbc", 8,
							      staging_path, sizeof(staging_path));
	zassert_equal(result, 0, "obj_created failed: %d", result);

	/* Declared size is 8; a 16-byte write from offset 0 overruns it. */
	result = sq_ble_ots_test_invoke_obj_write_with_path(staging_path, chunk, sizeof(chunk), 0,
							    0);
	zassert_equal(result, -EFBIG, "expected -EFBIG writing past declared size, got %d",
		      result);

	/* A write that fits exactly is accepted. */
	result = sq_ble_ots_test_invoke_obj_write_with_path(staging_path, chunk, 8, 0, 0);
	zassert_equal(result, 8, "expected exact-size write to succeed, got %d", result);
}

ZTEST(ble_ots_staging, test_reset_after_completed_transfer_does_not_error)
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
	zassert_equal(result, (int)sizeof(chunk));
	sq_ble_ots_test_invoke_abort();

	sq_ble_ots_reset_session();
}
