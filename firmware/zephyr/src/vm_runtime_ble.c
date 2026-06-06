#include "vm_runtime_internal.h"

#include "ble_object_transfer.h"
#include "ble_profile_table.h"
#include "squidvm_ffi.h"

#if IS_ENABLED(CONFIG_BT)
#include "ble_smoke_sm.h"
#endif

/*
 * Imperative BLE object-receive service. service.ble.start(profile, config)
 * lowers to BUILTIN_SERVICE_BLE_START carrying the profile id; the VM calls
 * runtime_ble_start while the owning app is the running foreground app. The
 * profile config is the compile-time literal encoded in the SQBC BLE-profile
 * section, so the callback reads it back from the running app's reader, keyed
 * by id, and registers it into the routing table for runtime->current_app.
 * service.ble.stop() clears the running app's profile and aborts any in-flight
 * transfer. Advertising is gated on the profile table being non-empty.
 */

/* Advertising follows the profile table: advertise while >=1 profile is
 * registered, stop once the last one is cleared. On non-BT builds (native_sim
 * ztests) there is no radio, so registration still updates the routing table
 * but advertising is a no-op.
 */
static int sq_vm_runtime_ble_advertising_sync(void)
{
#if IS_ENABLED(CONFIG_BT)
	if (sq_ble_profile_table_count() > 0) {
		return sq_ble_smoke_sm_begin_advertising();
	}
	return sq_ble_smoke_sm_stop_advertising();
#else
	return 0;
#endif
}

int32_t runtime_ble_start(void *user_data, const uint8_t *id, size_t id_len)
{
	struct sq_vm_runtime *runtime = user_data;
	char want_id[SQVM_BLE_PROFILE_TEXT_CAP];
	SqvmBleProfileTrigger profile;
	size_t count = 0;
	bool found = false;
	SqvmStatus status;
	int result;

	if (runtime == NULL || runtime->backend == NULL || runtime->backend->read_sqbc == NULL ||
	    id == NULL || id_len == 0 || id_len >= SQVM_BLE_PROFILE_TEXT_CAP ||
	    runtime->current_app[0] == '\0') {
		return -EINVAL;
	}
	memcpy(want_id, id, id_len);
	want_id[id_len] = '\0';

	result = sq_vm_runtime_transfer_acquire(runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH);
	if (result != 0) {
		return result;
	}
	status = sqvm_trigger_ble_profile_count_from_reader(
		runtime->backend->user_data, (SqvmReadExactAtCallback)runtime->backend->read_sqbc,
		runtime->transfer.init_scratch, sizeof(runtime->transfer.init_scratch), &count);
	for (size_t i = 0; status == SQVM_STATUS_OK && i < count && !found; i++) {
		memset(&profile, 0, sizeof(profile));
		status = sqvm_trigger_ble_profile_read_from_reader(
			runtime->backend->user_data,
			(SqvmReadExactAtCallback)runtime->backend->read_sqbc,
			runtime->transfer.init_scratch, sizeof(runtime->transfer.init_scratch), i,
			&profile);
		if (status == SQVM_STATUS_OK &&
		    strncmp((const char *)profile.id, want_id, SQVM_BLE_PROFILE_TEXT_CAP) == 0) {
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

	/* Idempotent set/replace: drop this app's prior profile, then (re)add the
	 * resolved one. A second start with the same config is a no-op replace.
	 */
	sq_ble_profile_table_remove_app(runtime->current_app);
	result = sq_ble_profile_table_add(
		runtime->current_app, (const char *)profile.profile,
		(const char (*)[SQVM_BLE_PROFILE_TEXT_CAP])profile.accept,
		(uint8_t)profile.accept_count, profile.events, (uint8_t)profile.event_count);
	if (result != 0) {
		return result;
	}
	(void)sq_vm_runtime_ble_advertising_sync();
	return 0;
}

int32_t runtime_ble_stop(void *user_data)
{
	struct sq_vm_runtime *runtime = user_data;

	if (runtime == NULL || runtime->current_app[0] == '\0') {
		return -EINVAL;
	}
	sq_ble_profile_table_remove_app(runtime->current_app);
	/* Discard any partially received object for this profile. A completed
	 * pending event is preserved by the abort/reset path for the consumer.
	 */
	sq_ble_transfer_abort();
	(void)sq_vm_runtime_ble_advertising_sync();
	return 0;
}
