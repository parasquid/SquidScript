#include "vm_fs_storage.h"

#include <errno.h>
#include <zephyr/fs/fs.h>
#include <zephyr/kernel.h>

static struct {
	struct fs_file_t file;
	struct sq_vm_fs_storage *owner;
	size_t owner_session_id;
	size_t open_count;
	size_t max_read_len;
} sqbc_open_file;

static size_t sqbc_next_session_id = 1;

static uint64_t fs_storage_state_save_us_acc;
static uint64_t fs_storage_sqbc_read_us_acc;

uint64_t sq_vm_fs_storage_drain_state_save_us(void)
{
	uint64_t us = fs_storage_state_save_us_acc;
	fs_storage_state_save_us_acc = 0;
	return us;
}

uint64_t sq_vm_fs_storage_drain_sqbc_read_us(void)
{
	uint64_t us = fs_storage_sqbc_read_us_acc;
	fs_storage_sqbc_read_us_acc = 0;
	return us;
}

static int release_open_sqbc_file(void)
{
	sqbc_open_file.owner = NULL;
	sqbc_open_file.owner_session_id = 0;
	return fs_close(&sqbc_open_file.file);
}

int sq_vm_fs_storage_release(struct sq_vm_fs_storage *storage)
{
	if (storage == NULL) {
		return -EINVAL;
	}
	if (sqbc_open_file.owner != storage ||
	    sqbc_open_file.owner_session_id != storage->sqbc_session_id) {
		return 0;
	}

	return release_open_sqbc_file();
}

bool sq_vm_fs_storage_is_open(const struct sq_vm_fs_storage *storage)
{
	return storage != NULL && sqbc_open_file.owner == storage &&
	       sqbc_open_file.owner_session_id == storage->sqbc_session_id;
}

bool sq_vm_fs_storage_has_open_file(void)
{
	return sqbc_open_file.owner != NULL;
}

size_t sq_vm_fs_storage_open_count(void)
{
	return sqbc_open_file.open_count;
}

size_t sq_vm_fs_storage_max_read_len(void)
{
	return sqbc_open_file.max_read_len;
}

static int read_optional_file(const char *path, uint8_t *out, size_t out_len, size_t *len)
{
	struct fs_file_t file;
	int result;
	uint8_t overflow;
	ssize_t read;
	ssize_t extra = 0;

	if (path == NULL || out == NULL || len == NULL) {
		return -EINVAL;
	}

	*len = 0;
	fs_file_t_init(&file);
	result = fs_open(&file, path, FS_O_READ);
	if (result == -ENOENT) {
		return 0;
	}
	if (result != 0) {
		return result;
	}

	read = fs_read(&file, out, out_len);
	if (read >= 0 && (size_t)read == out_len) {
		extra = fs_read(&file, &overflow, sizeof(overflow));
	}
	result = fs_close(&file);
	if (read < 0) {
		return (int)read;
	}
	if (extra < 0) {
		return (int)extra;
	}
	if (extra > 0) {
		return -ENOSPC;
	}
	*len = (size_t)read;
	return result;
}

static int write_file(const char *path, const uint8_t *bytes, size_t len)
{
	struct fs_file_t file;
	int result;

	if (path == NULL || bytes == NULL) {
		return -EINVAL;
	}

	fs_file_t_init(&file);
	result = fs_open(&file, path, FS_O_CREATE | FS_O_WRITE | FS_O_TRUNC);
	if (result != 0) {
		return result;
	}

	ssize_t written = fs_write(&file, bytes, len);
	result = fs_close(&file);
	if (written < 0) {
		return (int)written;
	}
	if ((size_t)written != len) {
		return -EIO;
	}
	return result;
}

static int fs_storage_read_sqbc(void *user_data, size_t offset, uint8_t *out, size_t len)
{
	struct sq_vm_fs_storage *storage = user_data;
	ssize_t read;
	int result;

	if (storage == NULL || storage->sqbc_path == NULL || out == NULL) {
		return -EINVAL;
	}

	uint64_t t0 = k_cycle_get_64();

	if (sqbc_open_file.owner != storage ||
	    sqbc_open_file.owner_session_id != storage->sqbc_session_id) {
		if (sqbc_open_file.owner != NULL) {
			result = release_open_sqbc_file();
			if (result != 0) {
				fs_storage_sqbc_read_us_acc += k_cyc_to_us_floor64(k_cycle_get_64() - t0);
				return result;
			}
		}
		fs_file_t_init(&sqbc_open_file.file);
		result = fs_open(&sqbc_open_file.file, storage->sqbc_path, FS_O_READ);
		if (result != 0) {
			fs_storage_sqbc_read_us_acc += k_cyc_to_us_floor64(k_cycle_get_64() - t0);
			return result;
		}
		sqbc_open_file.owner = storage;
		sqbc_open_file.owner_session_id = storage->sqbc_session_id;
		sqbc_open_file.open_count++;
		sqbc_open_file.max_read_len = 0;
	}

	result = fs_seek(&sqbc_open_file.file, (off_t)offset, FS_SEEK_SET);
	if (result != 0) {
		(void)sq_vm_fs_storage_release(storage);
		fs_storage_sqbc_read_us_acc += k_cyc_to_us_floor64(k_cycle_get_64() - t0);
		return result;
	}

	read = fs_read(&sqbc_open_file.file, out, len);
	if (read < 0) {
		(void)sq_vm_fs_storage_release(storage);
		fs_storage_sqbc_read_us_acc += k_cyc_to_us_floor64(k_cycle_get_64() - t0);
		return (int)read;
	}
	if ((size_t)read != len) {
		(void)sq_vm_fs_storage_release(storage);
		fs_storage_sqbc_read_us_acc += k_cyc_to_us_floor64(k_cycle_get_64() - t0);
		return -EIO;
	}

	storage->sqbc_read_count++;
	if (len > sqbc_open_file.max_read_len) {
		sqbc_open_file.max_read_len = len;
	}
	storage->sqbc_total_read_len += len;
	fs_storage_sqbc_read_us_acc += k_cyc_to_us_floor64(k_cycle_get_64() - t0);
	return 0;
}

static int fs_storage_load_state(void *user_data, uint8_t *out, size_t out_len, size_t *len)
{
	struct sq_vm_fs_storage *storage = user_data;

	if (storage == NULL) {
		return -EINVAL;
	}
	return read_optional_file(storage->state_path, out, out_len, len);
}

static int fs_storage_save_state(void *user_data, const uint8_t *bytes, size_t len)
{
	struct sq_vm_fs_storage *storage = user_data;

	if (storage == NULL) {
		return -EINVAL;
	}
	uint64_t t0 = k_cycle_get_64();
	int result = write_file(storage->state_path, bytes, len);
	fs_storage_state_save_us_acc += k_cyc_to_us_floor64(k_cycle_get_64() - t0);
	return result;
}

static int fs_storage_reset_state(void *user_data)
{
	struct sq_vm_fs_storage *storage = user_data;
	struct fs_dirent entry;

	if (storage == NULL || storage->state_path == NULL) {
		return -EINVAL;
	}

	int result = fs_stat(storage->state_path, &entry);
	if (result == -ENOENT) {
		return 0;
	}
	if (result != 0) {
		return result;
	}

	result = fs_unlink(storage->state_path);
	if (result == -ENOENT) {
		return 0;
	}
	return result;
}

struct sq_vm_storage_backend sq_vm_fs_storage_backend(struct sq_vm_fs_storage *storage)
{
	if (storage != NULL && storage->sqbc_session_id == 0) {
		storage->sqbc_session_id = sqbc_next_session_id++;
		if (sqbc_next_session_id == 0) {
			sqbc_next_session_id = 1;
		}
	}
	return (struct sq_vm_storage_backend){
		.user_data = storage,
		.read_sqbc = fs_storage_read_sqbc,
		.load_state = fs_storage_load_state,
		.save_state = fs_storage_save_state,
		.reset_state = fs_storage_reset_state,
	};
}
