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
	char app_id[SQ_APP_STORE_APP_ID_MAX];
	char profile_id[SQVM_BLE_PROFILE_TEXT_CAP];
	char accept_exts[SQVM_BLE_PROFILE_ACCEPT_MAX][SQVM_BLE_PROFILE_TEXT_CAP];
	uint8_t accept_count;
	SqvmBleProfileEventRoute events[SQVM_BLE_PROFILE_EVENT_MAX];
	uint8_t event_count;
};

int sq_ble_profile_table_add(const char *app_id, const char *profile_id,
			     const char (*accept_exts)[SQVM_BLE_PROFILE_TEXT_CAP],
			     uint8_t accept_count,
			     const SqvmBleProfileEventRoute *events, uint8_t event_count);

void sq_ble_profile_table_remove_app(const char *app_id);

void sq_ble_profile_table_reset(void);

size_t sq_ble_profile_table_count(void);

const struct sq_ble_profile_entry *sq_ble_profile_lookup(const char *app_id,
							 const char *profile_id);

const struct sq_ble_profile_entry *sq_ble_profile_lookup_app_accepting_extension(
	const char *app_id, const char *extension);

const struct sq_ble_profile_entry *sq_ble_profile_lookup_accepting_extension(
	const char *extension);

#ifdef __cplusplus
}
#endif

#endif /* SQUIDSCRIPT_BLE_PROFILE_TABLE_H */
