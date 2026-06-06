#include "ble_profile_table.h"

#include <errno.h>
#include <string.h>

#include "app_store.h"

static struct sq_ble_profile_entry sq_ble_profile_table[SQ_VM_RUNTIME_BLE_PROFILE_MAX];
static size_t sq_ble_profile_table_count_static;

int sq_ble_profile_table_add(const char *app_id, const char *profile_id,
			     const char (*accept_exts)[SQVM_BLE_PROFILE_TEXT_CAP],
			     uint8_t accept_count,
			     const SqvmBleProfileEventRoute *events, uint8_t event_count)
{
	struct sq_ble_profile_entry *entry;

	if (app_id == NULL || profile_id == NULL) {
		return -EINVAL;
	}
	if (!sq_app_store_is_safe_app_id(app_id)) {
		return -EINVAL;
	}
	if (accept_exts != NULL && accept_count > SQVM_BLE_PROFILE_ACCEPT_MAX) {
		return -EINVAL;
	}
	if (events != NULL && event_count > SQVM_BLE_PROFILE_EVENT_MAX) {
		return -EINVAL;
	}
	if (sq_ble_profile_table_count_static >= SQ_VM_RUNTIME_BLE_PROFILE_MAX) {
		return -EINVAL;
	}

	entry = &sq_ble_profile_table[sq_ble_profile_table_count_static];
	memset(entry, 0, sizeof(*entry));
	strncpy(entry->app_id, app_id, sizeof(entry->app_id) - 1);
	strncpy(entry->profile_id, profile_id, sizeof(entry->profile_id) - 1);
	if (accept_exts != NULL && accept_count > 0) {
		for (uint8_t i = 0; i < accept_count; i++) {
			strncpy(entry->accept_exts[i], accept_exts[i],
				sizeof(entry->accept_exts[i]) - 1);
		}
		entry->accept_count = accept_count;
	}
	if (events != NULL && event_count > 0) {
		memcpy(entry->events, events, sizeof(SqvmBleProfileEventRoute) * event_count);
		entry->event_count = event_count;
	}
	sq_ble_profile_table_count_static++;
	return 0;
}

void sq_ble_profile_table_remove_app(const char *app_id)
{
	if (app_id == NULL) {
		return;
	}
	for (size_t i = 0; i < sq_ble_profile_table_count_static; ) {
		if (strcmp(sq_ble_profile_table[i].app_id, app_id) == 0) {
			for (size_t j = i; j < sq_ble_profile_table_count_static - 1; j++) {
				sq_ble_profile_table[j] = sq_ble_profile_table[j + 1];
			}
			memset(&sq_ble_profile_table[sq_ble_profile_table_count_static - 1], 0,
			       sizeof(sq_ble_profile_table[0]));
			sq_ble_profile_table_count_static--;
		} else {
			i++;
		}
	}
}

void sq_ble_profile_table_reset(void)
{
	memset(sq_ble_profile_table, 0, sizeof(sq_ble_profile_table));
	sq_ble_profile_table_count_static = 0;
}

size_t sq_ble_profile_table_count(void)
{
	return sq_ble_profile_table_count_static;
}

const struct sq_ble_profile_entry *sq_ble_profile_lookup(const char *app_id,
							 const char *profile_id)
{
	if (app_id == NULL || profile_id == NULL) {
		return NULL;
	}
	for (size_t i = 0; i < sq_ble_profile_table_count_static; i++) {
		if (strcmp(sq_ble_profile_table[i].app_id, app_id) == 0 &&
		    strcmp(sq_ble_profile_table[i].profile_id, profile_id) == 0) {
			return &sq_ble_profile_table[i];
		}
	}
	return NULL;
}
