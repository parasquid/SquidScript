#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include <zephyr/bluetooth/services/ots.h>
#include <zephyr/fs/fs.h>
#include <zephyr/kernel.h>
#include <zephyr/ztest.h>

#include "ble_ots.h"

static void ble_trigger_table_before(void *fixture)
{
	(void)fixture;
	sq_ble_profile_table_reset();
}

ZTEST_SUITE(ble_trigger_table, NULL, NULL, NULL, ble_trigger_table_before, NULL);

ZTEST(ble_trigger_table, test_add_single_entry_succeeds)
{
	int result = sq_ble_profile_table_add("break-reminder", "wallpaper", NULL, 0, NULL, 0);

	zassert_equal(result, 0, "expected 0, got %d", result);
	zassert_equal(sq_ble_profile_table_count(), 1, "expected count 1");
}

ZTEST(ble_trigger_table, test_add_two_entries_for_same_app_succeeds)
{
	int result;

	result = sq_ble_profile_table_add("break-reminder", "wallpaper", NULL, 0, NULL, 0);
	zassert_equal(result, 0, "first add expected 0, got %d", result);
	result = sq_ble_profile_table_add("break-reminder", "ringtone", NULL, 0, NULL, 0);
	zassert_equal(result, 0, "second add expected 0, got %d", result);
	zassert_equal(sq_ble_profile_table_count(), 2, "expected count 2");
}

ZTEST(ble_trigger_table, test_add_third_entry_fails_cap_enforcement)
{
	int result;

	result = sq_ble_profile_table_add("app-a", "p1", NULL, 0, NULL, 0);
	zassert_equal(result, 0, "first add expected 0, got %d", result);
	result = sq_ble_profile_table_add("app-a", "p2", NULL, 0, NULL, 0);
	zassert_equal(result, 0, "second add expected 0, got %d", result);
	result = sq_ble_profile_table_add("app-b", "p3", NULL, 0, NULL, 0);
	zassert_equal(result, -EINVAL, "third add should fail with cap, got %d", result);
	zassert_equal(sq_ble_profile_table_count(), 2, "count should stay at 2 after cap reject");
}

ZTEST(ble_trigger_table, test_remove_app_clears_all_entries_for_that_app)
{
	int result;

	result = sq_ble_profile_table_add("app-a", "p1", NULL, 0, NULL, 0);
	zassert_equal(result, 0);
	result = sq_ble_profile_table_add("app-a", "p2", NULL, 0, NULL, 0);
	zassert_equal(result, 0);
	zassert_equal(sq_ble_profile_table_count(), 2, "expected count 2 before remove");

	sq_ble_profile_table_remove_app("app-a");
	zassert_equal(sq_ble_profile_table_count(), 0, "expected count 0 after remove app-a");
}

ZTEST(ble_trigger_table, test_reset_clears_entire_table)
{
	int result;

	result = sq_ble_profile_table_add("app-a", "p1", NULL, 0, NULL, 0);
	zassert_equal(result, 0);
	result = sq_ble_profile_table_add("app-a", "p2", NULL, 0, NULL, 0);
	zassert_equal(result, 0);
	zassert_equal(sq_ble_profile_table_count(), 2, "expected count 2");

	sq_ble_profile_table_reset();
	zassert_equal(sq_ble_profile_table_count(), 0, "expected count 0 after reset");
}

ZTEST(ble_trigger_table, test_lookup_hit_returns_entry)
{
	const struct sq_ble_profile_entry *entry;
	int result;

	result = sq_ble_profile_table_add("break-reminder", "wallpaper", NULL, 0, NULL, 0);
	zassert_equal(result, 0);
	result = sq_ble_profile_table_add("break-reminder", "ringtone", NULL, 0, NULL, 0);
	zassert_equal(result, 0);

	entry = sq_ble_profile_lookup("break-reminder", "wallpaper");
	zassert_not_null(entry, "expected hit for break-reminder/wallpaper");
	zassert_str_equal(entry->profile_id, "wallpaper", "profile_id mismatch");
}

ZTEST(ble_trigger_table, test_lookup_miss_returns_null)
{
	const struct sq_ble_profile_entry *entry;
	int result;

	result = sq_ble_profile_table_add("break-reminder", "wallpaper", NULL, 0, NULL, 0);
	zassert_equal(result, 0);

	entry = sq_ble_profile_lookup("break-reminder", "unknown");
	zassert_is_null(entry, "expected miss for break-reminder/unknown");
	entry = sq_ble_profile_lookup("unknown", "wallpaper");
	zassert_is_null(entry, "expected miss for unknown/wallpaper");
}

ZTEST(ble_trigger_table, test_reject_null_app_id)
{
	int result = sq_ble_profile_table_add(NULL, "wallpaper", NULL, 0, NULL, 0);

	zassert_equal(result, -EINVAL, "expected -EINVAL for NULL app_id, got %d", result);
}

ZTEST(ble_trigger_table, test_reject_unsafe_app_id)
{
	int result = sq_ble_profile_table_add("../evil", "wallpaper", NULL, 0, NULL, 0);

	zassert_equal(result, -EINVAL, "expected -EINVAL for unsafe app_id, got %d", result);
}

ZTEST(ble_trigger_table, test_table_starts_empty)
{
	zassert_equal(sq_ble_profile_table_count(), 0, "table should start empty after before");
}
