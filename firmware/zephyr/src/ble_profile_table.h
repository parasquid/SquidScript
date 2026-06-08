#ifndef SQUIDSCRIPT_BLE_PROFILE_TABLE_H
#define SQUIDSCRIPT_BLE_PROFILE_TABLE_H

#include <stddef.h>
#include <stdint.h>

#include "runtime_limits.h"
#include "squidvm_ffi.h"

/*
 * BLE foreground profile table. service.ble.start registers the current app's
 * active file-transfer profile here; the transport uses the table to match
 * uploaded file extensions to the foreground receiver.
 */

#ifdef __cplusplus
extern "C" {
#endif

struct sq_ble_profile_entry {
	uint8_t app_slot;
	char instance_id[SQVM_BLE_PROFILE_TEXT_CAP];
	char accept_exts[SQVM_BLE_PROFILE_ACCEPT_MAX][SQVM_BLE_PROFILE_TEXT_CAP];
	uint8_t accept_count;
	char complete_event[SQ_VM_RUNTIME_EVENT_LEN];
};

int sq_ble_profile_table_add(uint8_t app_slot, const char *instance_id,
			     const char (*accept_exts)[SQVM_BLE_PROFILE_TEXT_CAP],
			     uint8_t accept_count,
			     const SqvmBleProfileEventRoute *events, uint8_t event_count);

void sq_ble_profile_table_remove_app_slot(uint8_t app_slot);

void sq_ble_profile_table_reset(void);

size_t sq_ble_profile_table_count(void);

const struct sq_ble_profile_entry *sq_ble_profile_lookup(uint8_t app_slot,
							 const char *instance_id);

const struct sq_ble_profile_entry *sq_ble_profile_lookup_app_accepting_extension(
	uint8_t app_slot, const char *extension);

int sq_ble_profile_lookup_accepting_extension_result(
	const char *extension, const struct sq_ble_profile_entry **out);

const struct sq_ble_profile_entry *sq_ble_profile_lookup_accepting_extension(
	const char *extension);

#ifdef __cplusplus
}
#endif

#endif /* SQUIDSCRIPT_BLE_PROFILE_TABLE_H */
