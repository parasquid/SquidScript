#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include <zephyr/fs/fs.h>
#include <zephyr/kernel.h>
#include <zephyr/sys/util.h>
#include <zephyr/ztest.h>

#include "app_store.h"

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

static int format_test_app_store(void)
{
	return sq_app_store_format_filesystem(test_fs_mount.mnt_point);
}

static int write_staging_file(const char *path, const uint8_t *bytes, size_t len)
{
	struct fs_file_t file;
	ssize_t written;
	int result;

	fs_file_t_init(&file);
	result = fs_open(&file, path, FS_O_CREATE | FS_O_WRITE | FS_O_TRUNC);
	if (result != 0) {
		return result;
	}
	written = fs_write(&file, bytes, len);
	(void)fs_close(&file);
	if (written < 0) {
		return (int)written;
	}
	if ((size_t)written != len) {
		return -EIO;
	}
	return 0;
}

static const uint8_t sqbc_magic[] = {'S', 'Q', 'B', 'C'};

/* Deterministic SQBC content: 'SQBC' magic followed by (pos & 0xFF) for each
 * later byte, so a byte-exact readback can detect any truncation or corruption.
 */
static uint8_t generated_byte_at(size_t pos)
{
	return pos < sizeof(sqbc_magic) ? sqbc_magic[pos] : (uint8_t)(pos & 0xFFu);
}

static int write_generated_sqbc(const char *path, size_t total_len)
{
	struct fs_file_t file;
	uint8_t buf[256];
	size_t written_total = 0;
	int result;

	fs_file_t_init(&file);
	result = fs_open(&file, path, FS_O_CREATE | FS_O_WRITE | FS_O_TRUNC);
	if (result != 0) {
		return result;
	}
	while (written_total < total_len) {
		size_t chunk = MIN(sizeof(buf), total_len - written_total);
		ssize_t written;

		for (size_t i = 0; i < chunk; i++) {
			buf[i] = generated_byte_at(written_total + i);
		}
		written = fs_write(&file, buf, chunk);
		if (written < 0) {
			(void)fs_close(&file);
			return (int)written;
		}
		if ((size_t)written != chunk) {
			(void)fs_close(&file);
			return -EIO;
		}
		written_total += chunk;
	}
	return fs_close(&file);
}

static void verify_installed_matches_generated(const char *installed_path, size_t total_len)
{
	struct fs_file_t verify;
	uint8_t buf[256];
	size_t pos = 0;

	fs_file_t_init(&verify);
	zassert_equal(fs_open(&verify, installed_path, FS_O_READ), 0,
		      "expected installed SQBC at %s", installed_path);
	for (;;) {
		ssize_t got = fs_read(&verify, buf, sizeof(buf));

		zassert_true(got >= 0, "read error %d", (int)got);
		if (got == 0) {
			break;
		}
		for (ssize_t i = 0; i < got; i++) {
			zassert_equal(buf[i], generated_byte_at(pos),
				      "installed byte %zu mismatch", pos);
			pos++;
		}
	}
	(void)fs_close(&verify);
	zassert_equal(pos, total_len, "installed length mismatch: got %zu want %zu", pos,
		      total_len);
}

static void *ble_app_install_setup(void)
{
	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	return NULL;
}

static void ble_app_install_before(void *fixture)
{
	(void)fixture;
	zassert_equal(unmount_test_fs(), 0, "unmount failed");
	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
}

static void ble_app_install_teardown(void *fixture)
{
	(void)fixture;
	(void)unmount_test_fs();
}

ZTEST_SUITE(ble_app_install, NULL, ble_app_install_setup, ble_app_install_before, NULL,
	    ble_app_install_teardown);

ZTEST(ble_app_install, test_rejects_null_mount_point)
{
	int result = sq_app_store_install_from_file_ref(NULL, "valid-app", "/sqtest/staging.sqbc");

	zassert_equal(result, -EINVAL, "expected -EINVAL for NULL mount_point, got %d", result);
}

ZTEST(ble_app_install, test_rejects_null_staging_path)
{
	int result = sq_app_store_install_from_file_ref(test_fs_mount.mnt_point, "valid-app", NULL);

	zassert_equal(result, -EINVAL, "expected -EINVAL for NULL staging_path, got %d", result);
}

ZTEST(ble_app_install, test_rejects_invalid_app_id_with_slash)
{
	int result = sq_app_store_install_from_file_ref(test_fs_mount.mnt_point, "../evil",
							"/sqtest/staging.sqbc");

	zassert_equal(result, -EINVAL, "expected -EINVAL for unsafe app_id, got %d", result);
}

ZTEST(ble_app_install, test_rejects_empty_app_id)
{
	int result = sq_app_store_install_from_file_ref(test_fs_mount.mnt_point, "",
							"/sqtest/staging.sqbc");

	zassert_equal(result, -EINVAL, "expected -EINVAL for empty app_id, got %d", result);
}

ZTEST(ble_app_install, test_rejects_missing_staging_file)
{
	int result = sq_app_store_install_from_file_ref(test_fs_mount.mnt_point, "valid-app",
							"/sqtest/does-not-exist.sqbc");

	zassert_equal(result, -EINVAL, "expected -EINVAL for missing staging file, got %d", result);
}

ZTEST(ble_app_install, test_rejects_file_with_wrong_magic)
{
	const uint8_t bad_magic[] = {'X', 'X', 'X', 'X', 0x00, 0x00};
	char staging_path[SQ_APP_STORE_PATH_MAX];
	int result;

	zassert_true(snprintf(staging_path, sizeof(staging_path), "%s/bad.sqbc",
			      test_fs_mount.mnt_point) > 0);
	zassert_equal(write_staging_file(staging_path, bad_magic, sizeof(bad_magic)), 0);

	result = sq_app_store_install_from_file_ref(test_fs_mount.mnt_point, "valid-app",
						    staging_path);

	zassert_equal(result, -EINVAL,
		      "expected -EINVAL for file without SQBC magic, got %d", result);
}

ZTEST(ble_app_install, test_rejects_file_shorter_than_magic)
{
	const uint8_t too_short[] = {'S', 'Q'};
	char staging_path[SQ_APP_STORE_PATH_MAX];
	int result;

	zassert_true(snprintf(staging_path, sizeof(staging_path), "%s/short.sqbc",
			      test_fs_mount.mnt_point) > 0);
	zassert_equal(write_staging_file(staging_path, too_short, sizeof(too_short)), 0);

	result = sq_app_store_install_from_file_ref(test_fs_mount.mnt_point, "valid-app",
						    staging_path);

	zassert_equal(result, -EINVAL,
		      "expected -EINVAL for file shorter than magic, got %d", result);
}

ZTEST(ble_app_install, test_installs_valid_sqbc_magic)
{
	const uint8_t valid_sqbc[] = {'S', 'Q', 'B', 'C', 0x00, 0x00, 0x00, 0x00};
	char staging_path[SQ_APP_STORE_PATH_MAX];
	char installed_path[SQ_APP_STORE_APP_FILE_PATH_MAX];
	struct fs_file_t verify;
	char readback[8] = {0};
	ssize_t bytes_read;
	int result;

	zassert_true(snprintf(staging_path, sizeof(staging_path), "%s/valid.sqbc",
			      test_fs_mount.mnt_point) > 0);
	zassert_equal(write_staging_file(staging_path, valid_sqbc, sizeof(valid_sqbc)), 0);

	result = sq_app_store_install_from_file_ref(test_fs_mount.mnt_point, "installed-app",
						    staging_path);

	zassert_equal(result, 0, "expected 0 for valid SQBC, got %d", result);
	zassert_true(snprintf(installed_path, sizeof(installed_path),
			      "%s/apps/installed-app/main.sqbc",
			      test_fs_mount.mnt_point) > 0);
	fs_file_t_init(&verify);
	zassert_equal(fs_open(&verify, installed_path, FS_O_READ), 0,
		      "expected installed SQBC at %s", installed_path);
	bytes_read = fs_read(&verify, readback, sizeof(readback));
	(void)fs_close(&verify);
	zassert_equal(bytes_read, (ssize_t)sizeof(valid_sqbc), "installed file length mismatch");
	zassert_mem_equal(readback, valid_sqbc, sizeof(valid_sqbc), "installed file content mismatch");
}

ZTEST(ble_app_install, test_installed_file_overwrites_existing)
{
	const uint8_t first_sqbc[] = {'S', 'Q', 'B', 'C', 0x01, 0x02};
	const uint8_t second_sqbc[] = {'S', 'Q', 'B', 'C', 0x03, 0x04, 0x05};
	char staging_path[SQ_APP_STORE_PATH_MAX];
	char installed_path[SQ_APP_STORE_APP_FILE_PATH_MAX];
	struct fs_file_t verify;
	char readback[8] = {0};
	ssize_t bytes_read;

	zassert_true(snprintf(staging_path, sizeof(staging_path), "%s/overwrite.sqbc",
			      test_fs_mount.mnt_point) > 0);
	zassert_equal(write_staging_file(staging_path, first_sqbc, sizeof(first_sqbc)), 0);
	zassert_equal(sq_app_store_install_from_file_ref(test_fs_mount.mnt_point, "overwrite-app",
						       staging_path),
		      0);
	zassert_equal(write_staging_file(staging_path, second_sqbc, sizeof(second_sqbc)), 0);
	zassert_equal(sq_app_store_install_from_file_ref(test_fs_mount.mnt_point, "overwrite-app",
						       staging_path),
		      0);

	zassert_true(snprintf(installed_path, sizeof(installed_path),
			      "%s/apps/overwrite-app/main.sqbc",
			      test_fs_mount.mnt_point) > 0);
	fs_file_t_init(&verify);
	zassert_equal(fs_open(&verify, installed_path, FS_O_READ), 0);
	bytes_read = fs_read(&verify, readback, sizeof(readback));
	(void)fs_close(&verify);
	zassert_equal(bytes_read, (ssize_t)sizeof(second_sqbc));
	zassert_mem_equal(readback, second_sqbc, sizeof(second_sqbc));
}

/* Regression: a file larger than the old 1024-byte scratch buffer must install
 * byte-exact, not silently truncated. Fails before the streaming rewrite of
 * sq_app_store_install_from_file_ref.
 */
ZTEST(ble_app_install, test_installs_large_sqbc_without_truncation)
{
	const size_t big_len = 4096; /* well over the former 1 KiB cap */
	char staging_path[SQ_APP_STORE_PATH_MAX];
	char installed_path[SQ_APP_STORE_APP_FILE_PATH_MAX];

	zassert_true(snprintf(staging_path, sizeof(staging_path), "%s/large.sqbc",
			      test_fs_mount.mnt_point) > 0);
	zassert_equal(write_generated_sqbc(staging_path, big_len), 0);

	zassert_equal(sq_app_store_install_from_file_ref(test_fs_mount.mnt_point, "large-app",
							 staging_path),
		      0, "expected large SQBC to install");

	zassert_true(snprintf(installed_path, sizeof(installed_path),
			      "%s/apps/large-app/main.sqbc", test_fs_mount.mnt_point) > 0);
	verify_installed_matches_generated(installed_path, big_len);
}

/* A file above SQ_DEVICE_INSTALL_MAX_BYTES is rejected with -EFBIG and nothing
 * is committed.
 */
ZTEST(ble_app_install, test_rejects_oversized_sqbc)
{
	char staging_path[SQ_APP_STORE_PATH_MAX];
	char installed_path[SQ_APP_STORE_APP_FILE_PATH_MAX];
	struct fs_file_t verify;
	int result;

	zassert_true(snprintf(staging_path, sizeof(staging_path), "%s/oversized.sqbc",
			      test_fs_mount.mnt_point) > 0);
	zassert_equal(write_generated_sqbc(staging_path, SQ_DEVICE_INSTALL_MAX_BYTES + 1), 0);

	result = sq_app_store_install_from_file_ref(test_fs_mount.mnt_point, "oversized-app",
						    staging_path);
	zassert_equal(result, -EFBIG, "expected -EFBIG for oversized file, got %d", result);

	zassert_true(snprintf(installed_path, sizeof(installed_path),
			      "%s/apps/oversized-app/main.sqbc", test_fs_mount.mnt_point) > 0);
	fs_file_t_init(&verify);
	zassert_not_equal(fs_open(&verify, installed_path, FS_O_READ), 0,
			  "oversized file must not be committed");
}
