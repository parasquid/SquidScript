#ifndef SQUIDSCRIPT_VM_FS_STORAGE_H
#define SQUIDSCRIPT_VM_FS_STORAGE_H

#include "vm_storage.h"

#ifdef __cplusplus
extern "C" {
#endif

struct sq_vm_fs_storage {
	const char *sqbc_path;
	const char *state_path;
	size_t sqbc_read_count;
	size_t sqbc_session_id;
	size_t sqbc_total_read_len;
};

int sq_vm_fs_storage_release(struct sq_vm_fs_storage *storage);
bool sq_vm_fs_storage_is_open(const struct sq_vm_fs_storage *storage);
bool sq_vm_fs_storage_has_open_file(void);
size_t sq_vm_fs_storage_open_count(void);
size_t sq_vm_fs_storage_max_read_len(void);

struct sq_vm_storage_backend sq_vm_fs_storage_backend(struct sq_vm_fs_storage *storage);

#ifdef __cplusplus
}
#endif

#endif
