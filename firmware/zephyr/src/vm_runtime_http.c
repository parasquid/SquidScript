#include "vm_runtime_internal.h"

#include "http_upload.h"

static SqvmHttpProfileTrigger sq_http_start_profile;

static int current_http_app_id(const struct sq_vm_runtime *runtime, char *out, size_t out_len)
{
	if (runtime == NULL || out == NULL || out_len == 0 || runtime->current_app[0] == '\0' ||
	    runtime->current_app_temp) {
		return -EINVAL;
	}
	strncpy(out, runtime->current_app, out_len - 1);
	out[out_len - 1] = '\0';
	return out[0] == '\0' ? -EINVAL : 0;
}

int32_t runtime_http_start(void *user_data, const uint8_t *id, size_t id_len)
{
	struct sq_vm_runtime *runtime = user_data;
	char want_id[SQVM_HTTP_PROFILE_TEXT_CAP];
	char app_id[SQ_APP_STORE_APP_ID_MAX];
	size_t count = 0;
	bool found = false;
	SqvmStatus status;
	int result;

	if (runtime == NULL || runtime->backend == NULL || runtime->backend->read_sqbc == NULL ||
	    id == NULL || id_len == 0 || id_len >= SQVM_HTTP_PROFILE_TEXT_CAP) {
		return -EINVAL;
	}
	result = current_http_app_id(runtime, app_id, sizeof(app_id));
	if (result != 0) {
		return result;
	}
	memcpy(want_id, id, id_len);
	want_id[id_len] = '\0';

	result = sq_vm_runtime_transfer_acquire(runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH);
	if (result != 0) {
		return result;
	}
	status = sqvm_trigger_http_profile_count_from_reader(
		runtime->backend->user_data, (SqvmReadExactAtCallback)runtime->backend->read_sqbc,
		runtime->transfer.init_scratch, sizeof(runtime->transfer.init_scratch), &count);
	for (size_t i = 0; status == SQVM_STATUS_OK && i < count && !found; i++) {
		status = sqvm_trigger_http_profile_read_from_reader(
			runtime->backend->user_data,
			(SqvmReadExactAtCallback)runtime->backend->read_sqbc,
			runtime->transfer.init_scratch, sizeof(runtime->transfer.init_scratch), i,
			&sq_http_start_profile);
		if (status == SQVM_STATUS_OK &&
		    strncmp((const char *)sq_http_start_profile.id, want_id,
			    SQVM_HTTP_PROFILE_TEXT_CAP) == 0) {
			found = true;
		}
	}
	(void)sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH);
	if (status != SQVM_STATUS_OK) {
		return -EINVAL;
	}
	if (!found) {
		return -ENOENT;
	}
	return sq_http_upload_start_profile(
		app_id, (const char *)sq_http_start_profile.id,
		(const char (*)[SQVM_HTTP_PROFILE_TEXT_CAP])sq_http_start_profile.accept,
		sq_http_start_profile.accept_count, sq_http_start_profile.events,
		sq_http_start_profile.event_count);
}

int32_t runtime_http_stop(void *user_data)
{
	struct sq_vm_runtime *runtime = user_data;
	char app_id[SQ_APP_STORE_APP_ID_MAX];
	int result;

	result = current_http_app_id(runtime, app_id, sizeof(app_id));
	if (result != 0) {
		return result;
	}
	return sq_http_upload_stop_app(app_id);
}
