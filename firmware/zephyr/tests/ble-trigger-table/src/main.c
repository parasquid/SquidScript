#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include <zephyr/fs/fs.h>
#include <zephyr/kernel.h>
#include <zephyr/ztest.h>

#include "app_store.h"
#include "ble_profile_table.h"

static const SqvmBleProfileEventRoute complete_route[1] = {
	{.kind = "complete", .event = "ble.file.complete"},
};
static const char sqbc_accept[1][SQVM_BLE_PROFILE_TEXT_CAP] = {".sqbc"};

static struct sq_app_registry registry = {
	.count = 2,
	.apps = {
		{.app_id = "break-reminder", .sqbc_len = 128},
		{.app_id = "reader", .sqbc_len = 256},
	},
};

static void ble_trigger_table_before(void *fixture)
{
	(void)fixture;
	sq_ble_profile_table_reset();
}

ZTEST_SUITE(ble_trigger_table, NULL, NULL, NULL, ble_trigger_table_before, NULL);

ZTEST(ble_trigger_table, test_add_single_entry_succeeds)
{
	uint8_t app_slot = SQ_APP_REGISTRY_SLOT_INVALID;
	int result = sq_app_registry_slot_for_app(&registry, "break-reminder", &app_slot);

	zassert_equal(result, 0, "slot lookup expected 0, got %d", result);
	result = sq_ble_profile_table_add(app_slot, "wallpaper", sqbc_accept, 1, complete_route, 1);

	zassert_equal(result, 0, "expected 0, got %d", result);
	zassert_equal(sq_ble_profile_table_count(), 1, "expected count 1");
}

ZTEST(ble_trigger_table, test_add_two_entries_for_same_app_succeeds)
{
	uint8_t app_slot = SQ_APP_REGISTRY_SLOT_INVALID;
	int result;

	result = sq_app_registry_slot_for_app(&registry, "break-reminder", &app_slot);
	zassert_equal(result, 0);
	result = sq_ble_profile_table_add(app_slot, "wallpaper", sqbc_accept, 1, complete_route, 1);
	zassert_equal(result, 0, "first add expected 0, got %d", result);
	result = sq_ble_profile_table_add(app_slot, "ringtone", sqbc_accept, 1, complete_route, 1);
	zassert_equal(result, 0, "second add expected 0, got %d", result);
	zassert_equal(sq_ble_profile_table_count(), 2, "expected count 2");
}

ZTEST(ble_trigger_table, test_add_third_entry_fails_cap_enforcement)
{
	uint8_t app_a = 0;
	uint8_t app_b = 1;
	int result;

	result = sq_ble_profile_table_add(app_a, "p1", sqbc_accept, 1, complete_route, 1);
	zassert_equal(result, 0, "first add expected 0, got %d", result);
	result = sq_ble_profile_table_add(app_a, "p2", sqbc_accept, 1, complete_route, 1);
	zassert_equal(result, 0, "second add expected 0, got %d", result);
	result = sq_ble_profile_table_add(app_b, "p3", sqbc_accept, 1, complete_route, 1);
	zassert_equal(result, -EINVAL, "third add should fail with cap, got %d", result);
	zassert_equal(sq_ble_profile_table_count(), 2, "count should stay at 2 after cap reject");
}

ZTEST(ble_trigger_table, test_remove_app_clears_all_entries_for_that_app)
{
	uint8_t app_slot = 0;
	int result;

	result = sq_ble_profile_table_add(app_slot, "p1", sqbc_accept, 1, complete_route, 1);
	zassert_equal(result, 0);
	result = sq_ble_profile_table_add(app_slot, "p2", sqbc_accept, 1, complete_route, 1);
	zassert_equal(result, 0);
	zassert_equal(sq_ble_profile_table_count(), 2, "expected count 2 before remove");

	sq_ble_profile_table_remove_app_slot(app_slot);
	zassert_equal(sq_ble_profile_table_count(), 0, "expected count 0 after remove app-a");
}

ZTEST(ble_trigger_table, test_reset_clears_entire_table)
{
	uint8_t app_slot = 0;
	int result;

	result = sq_ble_profile_table_add(app_slot, "p1", sqbc_accept, 1, complete_route, 1);
	zassert_equal(result, 0);
	result = sq_ble_profile_table_add(app_slot, "p2", sqbc_accept, 1, complete_route, 1);
	zassert_equal(result, 0);
	zassert_equal(sq_ble_profile_table_count(), 2, "expected count 2");

	sq_ble_profile_table_reset();
	zassert_equal(sq_ble_profile_table_count(), 0, "expected count 0 after reset");
}

ZTEST(ble_trigger_table, test_lookup_hit_returns_entry)
{
	const struct sq_ble_profile_entry *entry;
	uint8_t app_slot = 0;
	int result;

	result = sq_ble_profile_table_add(app_slot, "wallpaper", sqbc_accept, 1, complete_route, 1);
	zassert_equal(result, 0);
	result = sq_ble_profile_table_add(app_slot, "ringtone", sqbc_accept, 1, complete_route, 1);
	zassert_equal(result, 0);

	entry = sq_ble_profile_lookup(app_slot, "wallpaper");
	zassert_not_null(entry, "expected hit for break-reminder/wallpaper");
	zassert_equal(entry->app_slot, app_slot, "app slot mismatch");
	zassert_str_equal(entry->instance_id, "wallpaper", "instance_id mismatch");
	zassert_str_equal(entry->complete_event, "ble.file.complete", "complete event mismatch");
}

ZTEST(ble_trigger_table, test_lookup_miss_returns_null)
{
	const struct sq_ble_profile_entry *entry;
	uint8_t app_slot = 0;
	int result;

	result = sq_ble_profile_table_add(app_slot, "wallpaper", sqbc_accept, 1, complete_route, 1);
	zassert_equal(result, 0);

	entry = sq_ble_profile_lookup(app_slot, "unknown");
	zassert_is_null(entry, "expected miss for break-reminder/unknown");
	entry = sq_ble_profile_lookup(1, "wallpaper");
	zassert_is_null(entry, "expected miss for unknown/wallpaper");
}

ZTEST(ble_trigger_table, test_lookup_reports_ambiguous_extension)
{
	const struct sq_ble_profile_entry *entry = NULL;
	int result;

	result = sq_ble_profile_table_add(0, "installed", sqbc_accept, 1, complete_route, 1);
	zassert_equal(result, 0);
	result = sq_ble_profile_table_add(SQ_APP_REGISTRY_SLOT_FALLBACK, "fallback",
					  sqbc_accept, 1, complete_route, 1);
	zassert_equal(result, 0);

	result = sq_ble_profile_lookup_accepting_extension_result(".sqbc", &entry);
	zassert_equal(result, -EEXIST, "ambiguous extension should report -EEXIST, got %d",
		      result);
	zassert_is_null(entry, "ambiguous lookup should not return an arbitrary route");
}

ZTEST(ble_trigger_table, test_reject_invalid_app_slot)
{
	int result = sq_ble_profile_table_add(SQ_APP_REGISTRY_SLOT_INVALID, "wallpaper",
					      sqbc_accept, 1, complete_route, 1);

	zassert_equal(result, -EINVAL, "expected -EINVAL for invalid app slot, got %d", result);
}

ZTEST(ble_trigger_table, test_reject_missing_complete_route)
{
	static const SqvmBleProfileEventRoute error_route[1] = {
		{.kind = "error", .event = "ble.file.error"},
	};
	int result = sq_ble_profile_table_add(0, "wallpaper", sqbc_accept, 1, error_route, 1);

	zassert_equal(result, -EINVAL, "expected -EINVAL for missing complete route, got %d", result);
}

ZTEST(ble_trigger_table, test_registry_slot_helpers_resolve_app_ids)
{
	uint8_t app_slot = SQ_APP_REGISTRY_SLOT_INVALID;
	const char *app_id;
	int result = sq_app_registry_slot_for_app(&registry, "reader", &app_slot);

	zassert_equal(result, 0, "slot lookup expected 0, got %d", result);
	zassert_equal(app_slot, 1, "reader slot mismatch");
	app_id = sq_app_registry_app_id_at(&registry, app_slot);
	zassert_not_null(app_id, "slot should resolve to app id");
	zassert_str_equal(app_id, "reader", "resolved app id mismatch");
	zassert_is_null(sq_app_registry_app_id_at(&registry, SQ_APP_REGISTRY_SLOT_INVALID),
			"invalid slot should not resolve");
}

ZTEST(ble_trigger_table, test_table_starts_empty)
{
	zassert_equal(sq_ble_profile_table_count(), 0, "table should start empty after before");
}
