#ifndef SQUIDSCRIPT_VM_STORAGE_H
#define SQUIDSCRIPT_VM_STORAGE_H

#include <stddef.h>
#include <stdint.h>

#include "squidvm_ffi.h"

#ifdef __cplusplus
extern "C" {
#endif

struct sq_vm_storage_backend {
	void *user_data;
	int (*read_sqbc)(void *user_data, size_t offset, uint8_t *out, size_t len);
	int (*load_state)(void *user_data, uint8_t *out, size_t out_len, size_t *len);
	int (*save_state)(void *user_data, const uint8_t *bytes, size_t len);
	int (*reset_state)(void *user_data);
};

int sq_vm_storage_complete_request(const struct sq_vm_storage_backend *backend,
				   const SqvmStorageRequest *request,
				   SqvmStorageCompletion *completion);

#ifdef __cplusplus
}
#endif

#endif
