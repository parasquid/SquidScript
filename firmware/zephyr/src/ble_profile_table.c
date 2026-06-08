#include "ble_profile_table.h"

#include <errno.h>
#include <string.h>

#include "app_store.h"

static struct sq_ble_profile_entry sq_ble_profile_table[SQ_VM_RUNTIME_BLE_PROFILE_MAX];
static size_t sq_ble_profile_table_count_static;

static bool text_equals(const uint8_t *text, size_t text_cap, const char *want)
{
	size_t want_len;

	if (text == NULL || want == NULL) {
		return false;
	}
	want_len = strlen(want);
	if (want_len >= text_cap) {
		return false;
	}
	return memcmp(text, want, want_len) == 0 && text[want_len] == '\0';
}

static int copy_text(char *out, size_t out_len, const char *value)
{
	size_t len;

	if (out == NULL || out_len == 0 || value == NULL) {
		return -EINVAL;
	}
	for (len = 0; len < out_len && value[len] != '\0'; len++) {
	}
	if (len == 0 || len >= out_len) {
		return -EINVAL;
	}
	memcpy(out, value, len);
	out[len] = '\0';
	return 0;
}

static int copy_complete_event(char *out, size_t out_len,
			       const SqvmBleProfileEventRoute *events, uint8_t event_count)
{
	if (out == NULL || events == NULL || event_count == 0 ||
	    event_count > SQVM_BLE_PROFILE_EVENT_MAX) {
		return -EINVAL;
	}
	for (uint8_t i = 0; i < event_count; i++) {
		if (!text_equals(events[i].kind, sizeof(events[i].kind), "complete")) {
			continue;
		}
		return copy_text(out, out_len, (const char *)events[i].event);
	}
	return -EINVAL;
}

int sq_ble_profile_table_add(uint8_t app_slot, const char *instance_id,
			     const char (*accept_exts)[SQVM_BLE_PROFILE_TEXT_CAP],
			     uint8_t accept_count,
			     const SqvmBleProfileEventRoute *events, uint8_t event_count)
{
	struct sq_ble_profile_entry *entry;

	if (app_slot == SQ_APP_REGISTRY_SLOT_INVALID || instance_id == NULL ||
	    accept_exts == NULL || accept_count == 0 ||
	    accept_count > SQVM_BLE_PROFILE_ACCEPT_MAX) {
		return -EINVAL;
	}
	if (sq_ble_profile_table_count_static >= SQ_VM_RUNTIME_BLE_PROFILE_MAX) {
		return -EINVAL;
	}

	entry = &sq_ble_profile_table[sq_ble_profile_table_count_static];
	memset(entry, 0, sizeof(*entry));
	entry->app_slot = app_slot;
	if (copy_text(entry->instance_id, sizeof(entry->instance_id), instance_id) != 0 ||
	    copy_complete_event(entry->complete_event, sizeof(entry->complete_event), events,
				event_count) != 0) {
		memset(entry, 0, sizeof(*entry));
		return -EINVAL;
	}
	for (uint8_t i = 0; i < accept_count; i++) {
		if (accept_exts[i][0] != '.' ||
		    copy_text(entry->accept_exts[i], sizeof(entry->accept_exts[i]),
			      accept_exts[i]) != 0) {
			memset(entry, 0, sizeof(*entry));
			return -EINVAL;
		}
	}
	entry->accept_count = accept_count;
	sq_ble_profile_table_count_static++;
	return 0;
}

void sq_ble_profile_table_remove_app_slot(uint8_t app_slot)
{
	if (app_slot == SQ_APP_REGISTRY_SLOT_INVALID) {
		return;
	}
	for (size_t i = 0; i < sq_ble_profile_table_count_static;) {
		if (sq_ble_profile_table[i].app_slot == app_slot) {
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

const struct sq_ble_profile_entry *sq_ble_profile_lookup(uint8_t app_slot,
							 const char *instance_id)
{
	if (app_slot == SQ_APP_REGISTRY_SLOT_INVALID || instance_id == NULL) {
		return NULL;
	}
	for (size_t i = 0; i < sq_ble_profile_table_count_static; i++) {
		if (sq_ble_profile_table[i].app_slot == app_slot &&
		    strcmp(sq_ble_profile_table[i].instance_id, instance_id) == 0) {
			return &sq_ble_profile_table[i];
		}
	}
	return NULL;
}

const struct sq_ble_profile_entry *sq_ble_profile_lookup_app_accepting_extension(
	uint8_t app_slot, const char *extension)
{
	if (app_slot == SQ_APP_REGISTRY_SLOT_INVALID || extension == NULL) {
		return NULL;
	}
	for (size_t i = 0; i < sq_ble_profile_table_count_static; i++) {
		const struct sq_ble_profile_entry *entry = &sq_ble_profile_table[i];

		if (entry->app_slot != app_slot) {
			continue;
		}
		for (uint8_t j = 0; j < entry->accept_count; j++) {
			if (strcmp(entry->accept_exts[j], extension) == 0) {
				return entry;
			}
		}
	}
	return NULL;
}

const struct sq_ble_profile_entry *sq_ble_profile_lookup_accepting_extension(
	const char *extension)
{
	const struct sq_ble_profile_entry *match = NULL;

	return sq_ble_profile_lookup_accepting_extension_result(extension, &match) == 0 ? match :
											    NULL;
}

int sq_ble_profile_lookup_accepting_extension_result(
	const char *extension, const struct sq_ble_profile_entry **out)
{
	const struct sq_ble_profile_entry *match = NULL;

	if (out == NULL) {
		return -EINVAL;
	}
	*out = NULL;
	if (extension == NULL) {
		return -EINVAL;
	}
	for (size_t i = 0; i < sq_ble_profile_table_count_static; i++) {
		const struct sq_ble_profile_entry *entry = &sq_ble_profile_table[i];

		for (uint8_t j = 0; j < entry->accept_count; j++) {
			if (strcmp(entry->accept_exts[j], extension) != 0) {
				continue;
			}
			if (match != NULL) {
				return -EEXIST;
			}
			match = entry;
		}
	}
	if (match == NULL) {
		return -ENOENT;
	}
	*out = match;
	return 0;
}
