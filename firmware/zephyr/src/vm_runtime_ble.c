#include "vm_runtime_internal.h"

#include "app_store.h"
#include "ble_file_transfer_core.h"
#include "ble_profile_table.h"
#include "squidvm_ffi.h"

#if IS_ENABLED(CONFIG_BT)
#include <zephyr/kernel.h>

#include "ble_smoke_sm.h"
#endif

/*
 * Imperative BLE file-transfer service. service.ble.start(profile, config)
 * lowers to BUILTIN_SERVICE_BLE_START carrying the profile id; the VM calls
 * runtime_ble_start while the owning app is the running foreground app. The
 * profile config is the compile-time literal encoded in the SQBC BLE-profile
 * section, so the callback reads it back from the running app's reader, keyed
 * by id, and registers it into the routing table for runtime->current_app.
 * service.ble.stop() clears the running app's profile and aborts any in-flight
 * transfer. Advertising is gated on the profile table being non-empty.
 */

/* The reader fills a ~640-byte SqvmBleProfileTrigger. runtime_ble_start runs
 * deep inside VM builtin dispatch (main loop -> protocol -> VM execute ->
 * call_builtin -> FFI), so keep this transient struct off that stack to avoid
 * overflowing the VM work stack. Access is single-threaded (one VM dispatch at
 * a time), so a file-scope buffer is safe. The reader itself constructs the
 * result in place in this buffer rather than on its own stack.
 */
static SqvmBleProfileTrigger sq_ble_start_profile;

static int current_ble_app_slot(const struct sq_vm_runtime *runtime, uint8_t *out_slot)
{
	if (out_slot != NULL) {
		*out_slot = SQ_APP_REGISTRY_SLOT_INVALID;
	}
	if (runtime == NULL || out_slot == NULL || runtime->current_app[0] == '\0' ||
	    runtime->current_app_temp) {
		return -EINVAL;
	}
	if (sq_app_registry_slot_for_app(runtime->registry, runtime->current_app, out_slot) == 0) {
		return 0;
	}
	if (strcmp(runtime->current_app, "main") == 0) {
		*out_slot = SQ_APP_REGISTRY_SLOT_FALLBACK;
		return 0;
	}
	return -EINVAL;
}

#if IS_ENABLED(CONFIG_BT)
/* Advertising must not be started/stopped inside the VM builtin dispatch:
 * bt_le_adv_start blocks on an HCI command. Defer the begin/stop to the system
 * work queue, the same context the disconnect-restart path drives advertising
 * from.
 */
static void sq_ble_adv_sync_work(struct k_work *work)
{
	ARG_UNUSED(work);
	if (sq_ble_profile_table_count() > 0) {
		(void)sq_ble_smoke_sm_begin_advertising();
	} else {
		(void)sq_ble_smoke_sm_stop_advertising();
	}
}

K_WORK_DEFINE(sq_ble_adv_sync, sq_ble_adv_sync_work);
#endif

/* Sync advertising to the profile table. On non-BT builds (native_sim ztests)
 * there is no radio, so registration still updates the routing table while
 * advertising is a no-op.
 */
static void sq_vm_runtime_ble_advertising_sync(void)
{
#if IS_ENABLED(CONFIG_BT)
	(void)k_work_submit(&sq_ble_adv_sync);
#endif
}

int32_t runtime_ble_start(void *user_data, const uint8_t *id, size_t id_len)
{
	struct sq_vm_runtime *runtime = user_data;
	char want_id[SQVM_BLE_PROFILE_TEXT_CAP];
	uint8_t app_slot = SQ_APP_REGISTRY_SLOT_INVALID;
	size_t count = 0;
	bool found = false;
	SqvmStatus status;
	int result;

	if (runtime == NULL || runtime->backend == NULL || runtime->backend->read_sqbc == NULL ||
	    id == NULL || id_len == 0 || id_len >= SQVM_BLE_PROFILE_TEXT_CAP) {
		return -EINVAL;
	}
	if (current_ble_app_slot(runtime, &app_slot) != 0) {
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
		status = sqvm_trigger_ble_profile_read_from_reader(
			runtime->backend->user_data,
			(SqvmReadExactAtCallback)runtime->backend->read_sqbc,
			runtime->transfer.init_scratch, sizeof(runtime->transfer.init_scratch), i,
			&sq_ble_start_profile);
		if (status == SQVM_STATUS_OK &&
		    strncmp((const char *)sq_ble_start_profile.id, want_id,
			    SQVM_BLE_PROFILE_TEXT_CAP) == 0) {
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

	/* Foreground BLE receive has a single active route table. Rebuild it from
	 * the foreground app's SQBC so stale routes from a previous app/storage
	 * generation cannot shadow the current receiver.
	 */
	sq_ble_profile_table_reset();
	result = sq_ble_profile_table_add(
		app_slot, (const char *)sq_ble_start_profile.id,
		(const char (*)[SQVM_BLE_PROFILE_TEXT_CAP])sq_ble_start_profile.accept,
		(uint8_t)sq_ble_start_profile.accept_count, sq_ble_start_profile.events,
		(uint8_t)sq_ble_start_profile.event_count);
	if (result != 0) {
		return result;
	}
	sq_vm_runtime_ble_advertising_sync();
	return 0;
}

int32_t runtime_ble_stop(void *user_data)
{
	struct sq_vm_runtime *runtime = user_data;
	uint8_t app_slot = SQ_APP_REGISTRY_SLOT_INVALID;
	int result;

	if (runtime == NULL) {
		return -EINVAL;
	}
	result = current_ble_app_slot(runtime, &app_slot);
	if (result != 0) {
		return -EINVAL;
	}
	sq_ble_profile_table_remove_app_slot(app_slot);
	/* Discard any partially received file for this profile. A completed
	 * pending event is preserved by the abort/reset path for the consumer.
	 */
	sq_ble_file_transfer_abort();
	sq_vm_runtime_ble_advertising_sync();
	return 0;
}
