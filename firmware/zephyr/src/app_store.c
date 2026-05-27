#include "app_store.h"

#include <errno.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>
#include <zephyr/devicetree.h>
#include <zephyr/fs/fs.h>

#if defined(CONFIG_FILE_SYSTEM_LITTLEFS) && DT_NODE_EXISTS(DT_NODELABEL(storage_partition))
#include <zephyr/fs/littlefs.h>
#include <zephyr/storage/flash_map.h>

FS_LITTLEFS_DECLARE_DEFAULT_CONFIG(sq_app_lfs_storage);

static struct fs_mount_t sq_app_store_target_mount = {
	.type = FS_LITTLEFS,
	.fs_data = &sq_app_lfs_storage,
	.storage_dev = (void *)PARTITION_ID(storage_partition),
	.mnt_point = "/sq",
};
#endif

static bool is_safe_app_id(const char *app_id)
{
	if (app_id == NULL || app_id[0] == '\0') {
		return false;
	}

	for (const char *cursor = app_id; *cursor != '\0'; cursor++) {
		char ch = *cursor;

		if ((ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') ||
		    (ch >= '0' && ch <= '9') || ch == '-' || ch == '_') {
			continue;
		}
		return false;
	}
	return true;
}

static bool is_safe_resource_char(char ch)
{
	return (ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') ||
	       (ch >= '0' && ch <= '9') || ch == '-' || ch == '_' || ch == '.';
}

static bool is_safe_resource_path_bytes(const uint8_t *resource_path, size_t resource_path_len)
{
	if (resource_path == NULL || resource_path_len == 0 || resource_path[0] == '/') {
		return false;
	}

	size_t segment_start = 0;
	for (size_t cursor = 0;; cursor++) {
		char ch = cursor < resource_path_len ? (char)resource_path[cursor] : '\0';

		if (ch == '/' || ch == '\0') {
			size_t len = cursor - segment_start;

			if (len == 0) {
				return false;
			}
			if ((len == 1 && resource_path[segment_start] == '.') ||
			    (len == 2 && resource_path[segment_start] == '.' &&
			     resource_path[segment_start + 1] == '.')) {
				return false;
			}
			if (cursor == resource_path_len) {
				return true;
			}
			segment_start = cursor + 1;
			continue;
		}

		if (!is_safe_resource_char(ch)) {
			return false;
		}
	}
}

static bool is_safe_resource_path(const char *resource_path)
{
	if (resource_path == NULL) {
		return false;
	}
	return is_safe_resource_path_bytes((const uint8_t *)resource_path, strlen(resource_path));
}

static int ensure_directory(const char *path)
{
	struct fs_dirent entry;
	int result = fs_stat(path, &entry);

	if (result == 0) {
		return entry.type == FS_DIR_ENTRY_DIR ? 0 : -ENOTDIR;
	}
	if (result == -ENOENT) {
		return fs_mkdir(path);
	}
	return result;
}

static int join_path2(char *out, size_t out_len, const char *mount_point, const char *child)
{
	if (out == NULL || mount_point == NULL || child == NULL) {
		return -EINVAL;
	}

	int written = snprintf(out, out_len, "%s/%s", mount_point, child);
	if (written < 0 || (size_t)written >= out_len) {
		return -ENAMETOOLONG;
	}
	return 0;
}

static int format_app_path(char *out, size_t out_len, const char *mount_point,
			   const char *app_id, const char *suffix)
{
	if (out == NULL || mount_point == NULL || app_id == NULL || suffix == NULL) {
		return -EINVAL;
	}

	int written = snprintf(out, out_len, "%s/apps/%s/%s", mount_point, app_id, suffix);
	if (written < 0 || (size_t)written >= out_len) {
		return -ENAMETOOLONG;
	}
	return 0;
}

static int format_resource_path(char *out, size_t out_len, const char *mount_point,
				const char *app_id, const char *resource_path)
{
	if (out == NULL || mount_point == NULL || app_id == NULL || resource_path == NULL) {
		return -EINVAL;
	}

	int written = snprintf(out, out_len, "%s/apps/%s/resources/%s", mount_point, app_id,
			       resource_path);
	if (written < 0 || (size_t)written >= out_len) {
		return -ENAMETOOLONG;
	}
	return 0;
}

static int format_resource_path_bytes(char *out, size_t out_len, const char *mount_point,
				      const char *app_id, const uint8_t *resource_path,
				      size_t resource_path_len)
{
	if (out == NULL || mount_point == NULL || app_id == NULL || resource_path == NULL) {
		return -EINVAL;
	}

	int written = snprintf(out, out_len, "%s/apps/%s/resources/", mount_point, app_id);
	if (written < 0 || (size_t)written >= out_len) {
		return -ENAMETOOLONG;
	}
	size_t prefix_len = (size_t)written;
	if (resource_path_len >= out_len - prefix_len) {
		return -ENAMETOOLONG;
	}
	memcpy(&out[prefix_len], resource_path, resource_path_len);
	out[prefix_len + resource_path_len] = '\0';
	return 0;
}

static int format_app_dir(char *out, size_t out_len, const char *mount_point, const char *app_id)
{
	if (out == NULL || mount_point == NULL || app_id == NULL) {
		return -EINVAL;
	}

	int written = snprintf(out, out_len, "%s/apps/%s", mount_point, app_id);
	if (written < 0 || (size_t)written >= out_len) {
		return -ENAMETOOLONG;
	}
	return 0;
}

static int format_state_path(char *out, size_t out_len, const char *mount_point,
			     const char *app_id)
{
	if (out == NULL || mount_point == NULL || app_id == NULL) {
		return -EINVAL;
	}

	int written = snprintf(out, out_len, "%s/state/%s.state", mount_point, app_id);
	if (written < 0 || (size_t)written >= out_len) {
		return -ENAMETOOLONG;
	}
	return 0;
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

static int ensure_resource_parent_dirs(const char *mount_point, const char *app_id,
				       const char *resource_path)
{
	char dir[SQ_APP_STORE_PATH_MAX];
	int written = snprintf(dir, sizeof(dir), "%s/apps/%s/resources", mount_point, app_id);

	if (written < 0 || (size_t)written >= sizeof(dir)) {
		return -ENAMETOOLONG;
	}

	int result = ensure_directory(dir);
	if (result != 0) {
		return result;
	}

	const char *segment = resource_path;
	const char *slash = strchr(segment, '/');
	while (slash != NULL) {
		size_t segment_len = (size_t)(slash - segment);
		size_t dir_len = strlen(dir);

		if (dir_len + 1 + segment_len >= sizeof(dir)) {
			return -ENAMETOOLONG;
		}

		dir[dir_len] = '/';
		memcpy(&dir[dir_len + 1], segment, segment_len);
		dir[dir_len + 1 + segment_len] = '\0';

		result = ensure_directory(dir);
		if (result != 0) {
			return result;
		}

		segment = slash + 1;
		slash = strchr(segment, '/');
	}

	return 0;
}

int sq_app_store_prepare_filesystem(const char *mount_point)
{
	char path[SQ_APP_STORE_PATH_MAX];
	int result;

	result = join_path2(path, sizeof(path), mount_point, "apps");
	if (result != 0) {
		return result;
	}
	result = ensure_directory(path);
	if (result != 0) {
		return result;
	}

	result = join_path2(path, sizeof(path), mount_point, "state");
	if (result != 0) {
		return result;
	}
	result = ensure_directory(path);
	if (result != 0) {
		return result;
	}

	result = join_path2(path, sizeof(path), mount_point, "tmp");
	if (result != 0) {
		return result;
	}
	result = ensure_directory(path);
	if (result != 0) {
		return result;
	}

	result = join_path2(path, sizeof(path), mount_point, "system");
	if (result != 0) {
		return result;
	}
	return ensure_directory(path);
}

int sq_app_store_mount_target_filesystem(void)
{
#if defined(CONFIG_FILE_SYSTEM_LITTLEFS) && DT_NODE_EXISTS(DT_NODELABEL(storage_partition))
	int result = fs_mount(&sq_app_store_target_mount);
	if (result == -EALREADY) {
		result = 0;
	}
	if (result != 0) {
		return result;
	}
	return sq_app_store_prepare_filesystem(sq_app_store_target_mount.mnt_point);
#else
	return -ENOTSUP;
#endif
}

const char *sq_app_store_mount_point(void)
{
#if defined(CONFIG_FILE_SYSTEM_LITTLEFS) && DT_NODE_EXISTS(DT_NODELABEL(storage_partition))
	return sq_app_store_target_mount.mnt_point;
#else
	return NULL;
#endif
}

int sq_app_store_vm_storage_for_app(const char *mount_point, const char *app_id,
				    struct sq_app_store_vm_storage *storage)
{
	if (storage == NULL || mount_point == NULL || !is_safe_app_id(app_id)) {
		return -EINVAL;
	}

	memset(storage, 0, sizeof(*storage));

	int result = format_app_path(storage->sqbc_path, sizeof(storage->sqbc_path), mount_point,
				     app_id, "main.sqbc");
	if (result != 0) {
		return result;
	}
	result = format_state_path(storage->state_path, sizeof(storage->state_path), mount_point,
				   app_id);
	if (result != 0) {
		return result;
	}

	storage->fs_storage.sqbc_path = storage->sqbc_path;
	storage->fs_storage.state_path = storage->state_path;
	return 0;
}

int sq_app_store_sqbc_path(const char *mount_point, const char *app_id, char *out,
			   size_t out_len)
{
	if (mount_point == NULL || !is_safe_app_id(app_id)) {
		return -EINVAL;
	}
	return format_app_path(out, out_len, mount_point, app_id, "main.sqbc");
}

int sq_app_store_install_app(const char *mount_point, const char *app_id, const uint8_t *sqbc,
			     size_t sqbc_len)
{
	char path[SQ_APP_STORE_PATH_MAX];
	int result;

	if (mount_point == NULL || !is_safe_app_id(app_id) || sqbc == NULL || sqbc_len == 0) {
		return -EINVAL;
	}

	result = sq_app_store_prepare_filesystem(mount_point);
	if (result != 0) {
		return result;
	}

	result = format_app_dir(path, sizeof(path), mount_point, app_id);
	if (result != 0) {
		return result;
	}
	result = ensure_directory(path);
	if (result != 0) {
		return result;
	}

	result = format_app_path(path, sizeof(path), mount_point, app_id, "main.sqbc");
	if (result != 0) {
		return result;
	}
	return write_file(path, sqbc, sqbc_len);
}

int sq_app_store_begin_staged_install(const char *mount_point, const char *app_id,
				      char *staging_path, size_t staging_path_len)
{
	char app_dir[SQ_APP_STORE_PATH_MAX];
	struct fs_file_t file;
	int result;

	if (mount_point == NULL || !is_safe_app_id(app_id) || staging_path == NULL) {
		return -EINVAL;
	}

	result = sq_app_store_prepare_filesystem(mount_point);
	if (result != 0) {
		return result;
	}
	result = format_app_dir(app_dir, sizeof(app_dir), mount_point, app_id);
	if (result != 0) {
		return result;
	}
	result = ensure_directory(app_dir);
	if (result != 0) {
		return result;
	}
	result = format_app_path(staging_path, staging_path_len, mount_point, app_id,
				 "main.sqbc.tmp");
	if (result != 0) {
		return result;
	}

	fs_file_t_init(&file);
	result = fs_open(&file, staging_path, FS_O_CREATE | FS_O_WRITE | FS_O_TRUNC);
	if (result != 0) {
		return result;
	}
	return fs_close(&file);
}

int sq_app_store_begin_temp_run(const char *mount_point, char *staging_path,
				size_t staging_path_len)
{
	struct fs_file_t file;
	int result;

	if (mount_point == NULL || staging_path == NULL) {
		return -EINVAL;
	}

	result = sq_app_store_prepare_filesystem(mount_point);
	if (result != 0) {
		return result;
	}

	result = join_path2(staging_path, staging_path_len, mount_point, "tmp/temp-run.sqbc.tmp");
	if (result != 0) {
		return result;
	}

	fs_file_t_init(&file);
	result = fs_open(&file, staging_path, FS_O_CREATE | FS_O_WRITE | FS_O_TRUNC);
	if (result != 0) {
		return result;
	}
	return fs_close(&file);
}

int sq_app_store_begin_staged_resource(const char *mount_point, char *staging_path,
				       size_t staging_path_len)
{
	struct fs_file_t file;
	int result;

	if (mount_point == NULL || staging_path == NULL) {
		return -EINVAL;
	}

	result = sq_app_store_prepare_filesystem(mount_point);
	if (result != 0) {
		return result;
	}
	result = join_path2(staging_path, staging_path_len, mount_point, "tmp/resource.tmp");
	if (result != 0) {
		return result;
	}

	fs_file_t_init(&file);
	result = fs_open(&file, staging_path, FS_O_CREATE | FS_O_WRITE | FS_O_TRUNC);
	if (result != 0) {
		return result;
	}
	return fs_close(&file);
}

int sq_app_store_write_staged_chunk(const char *staging_path, size_t offset,
				    const uint8_t *bytes, size_t len)
{
	struct fs_file_t file;
	int result;
	ssize_t written;

	if (staging_path == NULL || bytes == NULL) {
		return -EINVAL;
	}

	fs_file_t_init(&file);
	result = fs_open(&file, staging_path, FS_O_WRITE);
	if (result != 0) {
		return result;
	}
	result = fs_seek(&file, (off_t)offset, FS_SEEK_SET);
	if (result != 0) {
		fs_close(&file);
		return result;
	}
	written = fs_write(&file, bytes, len);
	result = fs_close(&file);
	if (written < 0) {
		return (int)written;
	}
	if ((size_t)written != len) {
		return -EIO;
	}
	return result;
}

int sq_app_store_commit_staged_install(const char *mount_point, const char *app_id,
				       const char *staging_path)
{
	char final_path[SQ_APP_STORE_PATH_MAX];
	int result;

	if (mount_point == NULL || !is_safe_app_id(app_id) || staging_path == NULL) {
		return -EINVAL;
	}

	result = format_app_path(final_path, sizeof(final_path), mount_point, app_id, "main.sqbc");
	if (result != 0) {
		return result;
	}

	result = fs_unlink(final_path);
	if (result != 0 && result != -ENOENT) {
		return result;
	}
	return fs_rename(staging_path, final_path);
}

int sq_app_store_commit_staged_resource(const char *mount_point, const char *app_id,
					const char *resource_path, const char *staging_path)
{
	char path[SQ_APP_STORE_PATH_MAX];
	struct fs_file_t main_sqbc;
	int result;

	if (mount_point == NULL || !is_safe_app_id(app_id) ||
	    !is_safe_resource_path(resource_path) || staging_path == NULL) {
		return -EINVAL;
	}

	result = format_app_path(path, sizeof(path), mount_point, app_id, "main.sqbc");
	if (result != 0) {
		return result;
	}
	fs_file_t_init(&main_sqbc);
	result = fs_open(&main_sqbc, path, FS_O_READ);
	if (result != 0) {
		return result == -EISDIR ? -ENOENT : result;
	}
	result = fs_close(&main_sqbc);
	if (result != 0) {
		return result;
	}

	result = ensure_resource_parent_dirs(mount_point, app_id, resource_path);
	if (result != 0) {
		return result;
	}
	result = sq_app_store_resource_path(mount_point, app_id, resource_path, path,
					    sizeof(path));
	if (result != 0) {
		return result;
	}
	result = fs_unlink(path);
	if (result != 0 && result != -ENOENT) {
		return result;
	}
	return fs_rename(staging_path, path);
}

int sq_app_store_resource_path(const char *mount_point, const char *app_id,
			       const char *resource_path, char *out, size_t out_len)
{
	if (mount_point == NULL || !is_safe_app_id(app_id) ||
	    !is_safe_resource_path(resource_path)) {
		return -EINVAL;
	}

	return format_resource_path(out, out_len, mount_point, app_id, resource_path);
}

int sq_app_store_resource_path_bytes(const char *mount_point, const char *app_id,
				     const uint8_t *resource_path, size_t resource_path_len,
				     char *out, size_t out_len)
{
	if (mount_point == NULL || !is_safe_app_id(app_id) ||
	    !is_safe_resource_path_bytes(resource_path, resource_path_len)) {
		return -EINVAL;
	}

	return format_resource_path_bytes(out, out_len, mount_point, app_id, resource_path,
					  resource_path_len);
}

int sq_app_store_device_config_path(const char *mount_point, char *out, size_t out_len)
{
	char system_dir[SQ_APP_STORE_PATH_MAX];
	int result;

	if (mount_point == NULL || out == NULL) {
		return -EINVAL;
	}
	result = join_path2(system_dir, sizeof(system_dir), mount_point, "system");
	if (result != 0) {
		return result;
	}
	return join_path2(out, out_len, system_dir, "device-config.sqdc");
}

int sq_app_store_install_resource(const char *mount_point, const char *app_id,
				  const char *resource_path, const uint8_t *bytes, size_t len)
{
	char path[SQ_APP_STORE_PATH_MAX];
	struct fs_file_t main_sqbc;
	int result;

	if (mount_point == NULL || !is_safe_app_id(app_id) ||
	    !is_safe_resource_path(resource_path) || bytes == NULL || len == 0) {
		return -EINVAL;
	}

	result = format_app_path(path, sizeof(path), mount_point, app_id, "main.sqbc");
	if (result != 0) {
		return result;
	}
	fs_file_t_init(&main_sqbc);
	result = fs_open(&main_sqbc, path, FS_O_READ);
	if (result != 0) {
		return result == -EISDIR ? -ENOENT : result;
	}
	result = fs_close(&main_sqbc);
	if (result != 0) {
		return result;
	}

	result = sq_app_store_prepare_filesystem(mount_point);
	if (result != 0) {
		return result;
	}
	result = ensure_resource_parent_dirs(mount_point, app_id, resource_path);
	if (result != 0) {
		return result;
	}
	result = sq_app_store_resource_path(mount_point, app_id, resource_path, path,
					    sizeof(path));
	if (result != 0) {
		return result;
	}
	return write_file(path, bytes, len);
}

int sq_app_store_scan_registry(const char *mount_point, struct sq_app_registry *registry)
{
	char path[SQ_APP_STORE_PATH_MAX];
	struct fs_dir_t dir;
	struct fs_dirent entry;
	int result;

	if (mount_point == NULL || registry == NULL) {
		return -EINVAL;
	}

	memset(registry, 0, sizeof(*registry));

	result = join_path2(path, sizeof(path), mount_point, "apps");
	if (result != 0) {
		return result;
	}

	fs_dir_t_init(&dir);
	result = fs_opendir(&dir, path);
	if (result != 0) {
		return result;
	}

	while (true) {
		result = fs_readdir(&dir, &entry);
		if (result != 0) {
			(void)fs_closedir(&dir);
			return result;
		}
		if (entry.name[0] == '\0') {
			break;
		}
		if (entry.type != FS_DIR_ENTRY_DIR || !is_safe_app_id(entry.name)) {
			continue;
		}

		struct sq_app_registry_entry *record = NULL;
		if (registry->count < SQ_APP_STORE_MAX_APPS) {
			record = &registry->apps[registry->count];
			strncpy(record->app_id, entry.name, sizeof(record->app_id) - 1u);
			record->app_id[sizeof(record->app_id) - 1u] = '\0';
		}
		result = format_app_path(path, sizeof(path), mount_point, entry.name,
					 "main.sqbc");
		if (result != 0) {
			(void)fs_closedir(&dir);
			return result;
		}
		result = fs_stat(path, &entry);
		if (result == -ENOENT) {
			continue;
		}
		if (result != 0) {
			(void)fs_closedir(&dir);
			return result;
		}
		if (entry.type != FS_DIR_ENTRY_FILE) {
			continue;
		}
		if (record == NULL) {
			(void)fs_closedir(&dir);
			return -ENOSPC;
		}

		record->sqbc_len = entry.size;
		registry->count++;
	}

	return fs_closedir(&dir);
}

static int delete_files_under(char *path, size_t path_cap, bool *deleted_any)
{
	struct fs_dirent entry;
	struct fs_dir_t dir;
	int result;

	fs_dir_t_init(&dir);
	result = fs_opendir(&dir, path);
	if (result == -ENOENT) {
		return 0;
	}
	if (result != 0) {
		return result;
	}
	while (true) {
		result = fs_readdir(&dir, &entry);
		if (result != 0) {
			(void)fs_closedir(&dir);
			return result;
		}
		if (entry.name[0] == '\0') {
			break;
		}
		if (strcmp(entry.name, ".") == 0 || strcmp(entry.name, "..") == 0) {
			continue;
		}

		size_t path_len = strlen(path);
		size_t name_len = strlen(entry.name);
		if (path_len + 1u + name_len >= path_cap) {
			(void)fs_closedir(&dir);
			return -ENAMETOOLONG;
		}
		path[path_len] = '/';
		memcpy(&path[path_len + 1u], entry.name, name_len + 1u);

		if (entry.type == FS_DIR_ENTRY_FILE) {
			result = fs_unlink(path);
			if (result == -ENOENT) {
				path[path_len] = '\0';
				continue;
			}
			if (result != 0) {
				path[path_len] = '\0';
				(void)fs_closedir(&dir);
				return result;
			}
			path[path_len] = '\0';
			*deleted_any = true;
		} else {
			result = delete_files_under(path, path_cap, deleted_any);
			if (result != 0) {
				path[path_len] = '\0';
				(void)fs_closedir(&dir);
				return result;
			}
			result = fs_unlink(path);
			if (result == -ENOENT) {
				path[path_len] = '\0';
				continue;
			}
			if (result != 0) {
				path[path_len] = '\0';
				(void)fs_closedir(&dir);
				return result;
			}
			path[path_len] = '\0';
			*deleted_any = true;
		}
	}
	return fs_closedir(&dir);
}

int sq_app_store_format_filesystem(const char *mount_point)
{
	char path[SQ_APP_STORE_PATH_MAX];
	int result;

	if (mount_point == NULL) {
		return -EINVAL;
	}
	for (size_t i = 0; i < 3; i++) {
		const char *name = i == 0 ? "apps" : (i == 1 ? "state" : "tmp");
		result = join_path2(path, sizeof(path), mount_point, name);
		if (result != 0) {
			return result;
		}
		do {
			bool deleted_any = false;
			result = delete_files_under(path, sizeof(path), &deleted_any);
			if (result != 0) {
				return result;
			}
			if (!deleted_any) {
				break;
			}
		} while (true);
	}
	return sq_app_store_prepare_filesystem(mount_point);
}

const struct sq_app_registry_entry *sq_app_registry_find(const struct sq_app_registry *registry,
							const char *app_id)
{
	if (registry == NULL || !is_safe_app_id(app_id)) {
		return NULL;
	}

	for (size_t i = 0; i < registry->count; i++) {
		if (strcmp(registry->apps[i].app_id, app_id) == 0) {
			return &registry->apps[i];
		}
	}
	return NULL;
}

struct sq_vm_storage_backend
sq_app_store_vm_storage_backend(struct sq_app_store_vm_storage *storage)
{
	if (storage == NULL) {
		return (struct sq_vm_storage_backend){0};
	}
	return sq_vm_fs_storage_backend(&storage->fs_storage);
}
