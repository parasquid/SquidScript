#include "vm_storage.h"

#include <errno.h>
#include <string.h>

static void clear_completion(SqvmStorageCompletion *completion)
{
	memset(completion, 0, sizeof(*completion));
}

int sq_vm_storage_complete_request(const struct sq_vm_storage_backend *backend,
				   const SqvmStorageRequest *request,
				   SqvmStorageCompletion *completion)
{
	if (backend == NULL || request == NULL || completion == NULL) {
		return -EINVAL;
	}

	clear_completion(completion);

	switch (request->kind) {
	case SQVM_STORAGE_REQUEST_NONE:
		return 0;
	case SQVM_STORAGE_REQUEST_SQBC_READ:
		if (backend->read_sqbc == NULL || request->len > SQVM_STORAGE_TRANSFER_CAPACITY) {
			return -EINVAL;
		}
		completion->has_len = true;
		completion->len = request->len;
		return backend->read_sqbc(backend->user_data, request->offset, completion->bytes,
					  request->len);
	case SQVM_STORAGE_REQUEST_STATE_LOAD: {
		if (backend->load_state == NULL) {
			return -EINVAL;
		}
		int result = backend->load_state(backend->user_data, completion->bytes,
						 sizeof(completion->bytes), &completion->len);
		if (result != 0) {
			return result;
		}
		completion->has_len = completion->len > 0;
		return 0;
	}
	case SQVM_STORAGE_REQUEST_STATE_SAVE:
		if (backend->save_state == NULL || request->len > SQVM_STORAGE_TRANSFER_CAPACITY) {
			return -EINVAL;
		}
		return backend->save_state(backend->user_data, request->bytes, request->len);
	case SQVM_STORAGE_REQUEST_STATE_RESET:
		if (backend->reset_state == NULL) {
			return -EINVAL;
		}
		return backend->reset_state(backend->user_data);
	default:
		return -EINVAL;
	}
}
