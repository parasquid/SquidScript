#ifndef SQUIDSCRIPT_APP_STORE_H
#define SQUIDSCRIPT_APP_STORE_H

#include "vm_fs_storage.h"

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define SQ_APP_STORE_PATH_MAX 128
#define SQ_APP_STORE_APP_ID_MAX 48
#define SQ_APP_STORE_MAX_APPS 12

struct fs_mount_t;

struct sq_app_registry_entry {
	char app_id[SQ_APP_STORE_APP_ID_MAX];
	size_t sqbc_len;
};

struct sq_app_registry {
	size_t count;
	struct sq_app_registry_entry apps[SQ_APP_STORE_MAX_APPS];
};

struct sq_app_store_vm_storage {
	struct sq_vm_fs_storage fs_storage;
	char sqbc_path[SQ_APP_STORE_PATH_MAX];
	char state_path[SQ_APP_STORE_PATH_MAX];
};

int sq_app_store_prepare_filesystem(const char *mount_point);

int sq_app_store_mount_target_filesystem(void);

const char *sq_app_store_mount_point(void);

int sq_app_store_vm_storage_for_app(const char *mount_point, const char *app_id,
				    struct sq_app_store_vm_storage *storage);

int sq_app_store_install_app(const char *mount_point, const char *app_id, const uint8_t *sqbc,
			     size_t sqbc_len);

int sq_app_store_begin_staged_install(const char *mount_point, const char *app_id,
				      char *staging_path, size_t staging_path_len);

int sq_app_store_begin_temp_run(const char *mount_point, char *staging_path,
				size_t staging_path_len);

int sq_app_store_begin_staged_resource(const char *mount_point, char *staging_path,
				       size_t staging_path_len);

int sq_app_store_write_staged_chunk(const char *staging_path, size_t offset,
				    const uint8_t *bytes, size_t len);

int sq_app_store_commit_staged_install(const char *mount_point, const char *app_id,
				       const char *staging_path);

int sq_app_store_commit_staged_resource(const char *mount_point, const char *app_id,
					const char *resource_path, const char *staging_path);

int sq_app_store_resource_path(const char *mount_point, const char *app_id,
			       const char *resource_path, char *out, size_t out_len);

int sq_app_store_device_config_path(const char *mount_point, char *out, size_t out_len);

int sq_app_store_install_resource(const char *mount_point, const char *app_id,
				  const char *resource_path, const uint8_t *bytes,
				  size_t len);

int sq_app_store_scan_registry(const char *mount_point, struct sq_app_registry *registry);

int sq_app_store_format_filesystem(const char *mount_point);

const struct sq_app_registry_entry *sq_app_registry_find(const struct sq_app_registry *registry,
							const char *app_id);

struct sq_vm_storage_backend
sq_app_store_vm_storage_backend(struct sq_app_store_vm_storage *storage);

#ifdef __cplusplus
}
#endif

#endif
