#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include <zephyr/bluetooth/services/ots.h>
#include <zephyr/kernel.h>
#include <zephyr/ztest.h>

#include "ble_ots.h"

#define APP_ID_CAP 32
#define PROFILE_ID_CAP 32
#define EXT_CAP 16

ZTEST_SUITE(ble_ots_parse, NULL, NULL, NULL, NULL, NULL);

ZTEST(ble_ots_parse, test_parses_valid_three_segment_name)
{
	char app_id[APP_ID_CAP] = {0};
	char profile_id[PROFILE_ID_CAP] = {0};
	char ext[EXT_CAP] = {0};
	int result = sq_ble_ots_parse_object_name("break-reminder/wallpaper/.sqbc",
						 app_id, sizeof(app_id), profile_id,
						 sizeof(profile_id), ext, sizeof(ext));

	zassert_equal(result, 0, "expected 0, got %d result, app_id='%s' profile_id='%s' ext='%s'",
		      result, app_id, profile_id, ext);
	zassert_str_equal(app_id, "break-reminder", "app_id mismatch");
	zassert_str_equal(profile_id, "wallpaper", "profile_id mismatch");
	zassert_str_equal(ext, ".sqbc", "ext mismatch");
}

ZTEST(ble_ots_parse, test_rejects_name_without_any_slash)
{
	char app_id[APP_ID_CAP] = {0};
	char profile_id[PROFILE_ID_CAP] = {0};
	char ext[EXT_CAP] = {0};
	int result = sq_ble_ots_parse_object_name("noslash", app_id, sizeof(app_id),
						  profile_id, sizeof(profile_id), ext,
						  sizeof(ext));

	zassert_equal(result, BT_GATT_OTS_OACP_RES_INV_PARAM,
		      "expected INV_PARAM, got %d", result);
}

ZTEST(ble_ots_parse, test_rejects_name_with_only_one_slash)
{
	char app_id[APP_ID_CAP] = {0};
	char profile_id[PROFILE_ID_CAP] = {0};
	char ext[EXT_CAP] = {0};
	int result = sq_ble_ots_parse_object_name("break-reminder/wallpaper", app_id,
						  sizeof(app_id), profile_id,
						  sizeof(profile_id), ext, sizeof(ext));

	zassert_equal(result, BT_GATT_OTS_OACP_RES_INV_PARAM,
		      "expected INV_PARAM, got %d", result);
}

ZTEST(ble_ots_parse, test_rejects_name_with_too_many_slashes)
{
	char app_id[APP_ID_CAP] = {0};
	char profile_id[PROFILE_ID_CAP] = {0};
	char ext[EXT_CAP] = {0};
	int result = sq_ble_ots_parse_object_name("a/b/c/d.sqbc", app_id, sizeof(app_id),
						  profile_id, sizeof(profile_id), ext,
						  sizeof(ext));

	zassert_equal(result, BT_GATT_OTS_OACP_RES_INV_PARAM,
		      "expected INV_PARAM, got %d", result);
}

ZTEST(ble_ots_parse, test_rejects_empty_app_id_segment)
{
	char app_id[APP_ID_CAP] = {0};
	char profile_id[PROFILE_ID_CAP] = {0};
	char ext[EXT_CAP] = {0};
	int result = sq_ble_ots_parse_object_name("/wallpaper.sqbc", app_id, sizeof(app_id),
						  profile_id, sizeof(profile_id), ext,
						  sizeof(ext));

	zassert_equal(result, BT_GATT_OTS_OACP_RES_INV_PARAM,
		      "expected INV_PARAM, got %d", result);
}

ZTEST(ble_ots_parse, test_rejects_empty_profile_id_segment)
{
	char app_id[APP_ID_CAP] = {0};
	char profile_id[PROFILE_ID_CAP] = {0};
	char ext[EXT_CAP] = {0};
	int result = sq_ble_ots_parse_object_name("app/.sqbc", app_id, sizeof(app_id),
						  profile_id, sizeof(profile_id), ext,
						  sizeof(ext));

	zassert_equal(result, BT_GATT_OTS_OACP_RES_INV_PARAM,
		      "expected INV_PARAM, got %d", result);
}

ZTEST(ble_ots_parse, test_rejects_empty_extension_segment)
{
	char app_id[APP_ID_CAP] = {0};
	char profile_id[PROFILE_ID_CAP] = {0};
	char ext[EXT_CAP] = {0};
	int result = sq_ble_ots_parse_object_name("app/wallpaper/", app_id, sizeof(app_id),
						  profile_id, sizeof(profile_id), ext,
						  sizeof(ext));

	zassert_equal(result, BT_GATT_OTS_OACP_RES_INV_PARAM,
		      "expected INV_PARAM, got %d", result);
}

ZTEST(ble_ots_parse, test_rejects_extension_not_starting_with_dot)
{
	char app_id[APP_ID_CAP] = {0};
	char profile_id[PROFILE_ID_CAP] = {0};
	char ext[EXT_CAP] = {0};
	int result = sq_ble_ots_parse_object_name("app/wallpapersqbc", app_id, sizeof(app_id),
						  profile_id, sizeof(profile_id), ext,
						  sizeof(ext));

	zassert_equal(result, BT_GATT_OTS_OACP_RES_INV_PARAM,
		      "expected INV_PARAM, got %d", result);
}

ZTEST(ble_ots_parse, test_rejects_unsafe_app_id_with_slash)
{
	char app_id[APP_ID_CAP] = {0};
	char profile_id[PROFILE_ID_CAP] = {0};
	char ext[EXT_CAP] = {0};
	int result = sq_ble_ots_parse_object_name("evil/app/wallpaper/.sqbc", app_id,
						  sizeof(app_id), profile_id,
						  sizeof(profile_id), ext, sizeof(ext));

	zassert_equal(result, BT_GATT_OTS_OACP_RES_INV_PARAM,
		      "expected INV_PARAM, got %d", result);
}

ZTEST(ble_ots_parse, test_rejects_unsafe_app_id_with_dot)
{
	char app_id[APP_ID_CAP] = {0};
	char profile_id[PROFILE_ID_CAP] = {0};
	char ext[EXT_CAP] = {0};
	int result = sq_ble_ots_parse_object_name("evil.app/wallpaper/.sqbc", app_id,
						  sizeof(app_id), profile_id,
						  sizeof(profile_id), ext, sizeof(ext));

	zassert_equal(result, BT_GATT_OTS_OACP_RES_INV_PARAM,
		      "expected INV_PARAM, got %d", result);
}

ZTEST(ble_ots_parse, test_rejects_app_id_longer_than_buffer)
{
	char app_id[8] = {0};
	char profile_id[PROFILE_ID_CAP] = {0};
	char ext[EXT_CAP] = {0};
	int result = sq_ble_ots_parse_object_name(
		"verylongappname-shorterthan-nine/wallpaper/.sqbc", app_id, sizeof(app_id),
		profile_id, sizeof(profile_id), ext, sizeof(ext));

	zassert_equal(result, BT_GATT_OTS_OACP_RES_INV_PARAM,
		      "expected INV_PARAM, got %d", result);
}

ZTEST(ble_ots_parse, test_rejects_extension_longer_than_buffer)
{
	char app_id[APP_ID_CAP] = {0};
	char profile_id[PROFILE_ID_CAP] = {0};
	char ext[4] = {0};
	int result = sq_ble_ots_parse_object_name("app/wallpaper/.thisisalongish", app_id,
						  sizeof(app_id), profile_id,
						  sizeof(profile_id), ext, sizeof(ext));

	zassert_equal(result, BT_GATT_OTS_OACP_RES_INV_PARAM,
		      "expected INV_PARAM, got %d", result);
}

ZTEST(ble_ots_parse, test_rejects_null_name)
{
	char app_id[APP_ID_CAP] = {0};
	char profile_id[PROFILE_ID_CAP] = {0};
	char ext[EXT_CAP] = {0};
	int result = sq_ble_ots_parse_object_name(NULL, app_id, sizeof(app_id), profile_id,
						  sizeof(profile_id), ext, sizeof(ext));

	zassert_true(result < 0, "expected negative error from NULL name, got %d", result);
}

ZTEST(ble_ots_parse, test_accepts_underscore_and_dash_in_app_id)
{
	char app_id[APP_ID_CAP] = {0};
	char profile_id[PROFILE_ID_CAP] = {0};
	char ext[EXT_CAP] = {0};
	int result = sq_ble_ots_parse_object_name("break_reminder-2/wallpaper/.sqbc",
						 app_id, sizeof(app_id), profile_id,
						 sizeof(profile_id), ext, sizeof(ext));

	zassert_equal(result, 0, "expected 0, got %d", result);
	zassert_str_equal(app_id, "break_reminder-2");
	zassert_str_equal(profile_id, "wallpaper");
	zassert_str_equal(ext, ".sqbc");
}
