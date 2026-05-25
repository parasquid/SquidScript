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
	size_t sqbc_max_read_len;
	size_t sqbc_total_read_len;
};

struct sq_vm_storage_backend sq_vm_fs_storage_backend(struct sq_vm_fs_storage *storage);

#ifdef __cplusplus
}
#endif

#endif
