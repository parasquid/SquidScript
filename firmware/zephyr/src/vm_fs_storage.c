#include "vm_fs_storage.h"

#include <errno.h>
#include <zephyr/fs/fs.h>

static int read_exact(const char *path, size_t offset, uint8_t *out, size_t len)
{
	struct fs_file_t file;
	int result;

	if (path == NULL || out == NULL) {
		return -EINVAL;
	}

	fs_file_t_init(&file);
	result = fs_open(&file, path, FS_O_READ);
	if (result != 0) {
		return result;
	}

	result = fs_seek(&file, (off_t)offset, FS_SEEK_SET);
	if (result != 0) {
		(void)fs_close(&file);
		return result;
	}

	ssize_t read = fs_read(&file, out, len);
	result = fs_close(&file);
	if (read < 0) {
		return (int)read;
	}
	if ((size_t)read != len) {
		return -EIO;
	}
	return result;
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

	if (storage == NULL) {
		return -EINVAL;
	}
	int result = read_exact(storage->sqbc_path, offset, out, len);
	if (result == 0) {
		storage->sqbc_read_count++;
		if (len > storage->sqbc_max_read_len) {
			storage->sqbc_max_read_len = len;
		}
		storage->sqbc_total_read_len += len;
	}
	return result;
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
	return write_file(storage->state_path, bytes, len);
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
	return (struct sq_vm_storage_backend){
		.user_data = storage,
		.read_sqbc = fs_storage_read_sqbc,
		.load_state = fs_storage_load_state,
		.save_state = fs_storage_save_state,
		.reset_state = fs_storage_reset_state,
	};
}
