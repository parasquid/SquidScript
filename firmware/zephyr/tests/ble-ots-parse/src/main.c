#include <errno.h>
#include <string.h>

#include <zephyr/ztest.h>

#include "ble_object_transfer.h"

#define EXT_CAP 16

ZTEST_SUITE(ble_ots_parse, NULL, NULL, NULL, NULL, NULL);

ZTEST(ble_ots_parse, test_parses_extension_only_name)
{
	char ext[EXT_CAP] = {0};
	int result = sq_ble_ots_parse_object_name(".sqbc", ext, sizeof(ext));

	zassert_equal(result, 0, "expected 0, got %d", result);
	zassert_str_equal(ext, ".sqbc", "extension mismatch");
}

ZTEST(ble_ots_parse, test_rejects_routed_segments)
{
	char ext[EXT_CAP] = {0};
	int result = sq_ble_ots_parse_object_name("break-reminder/wallpaper/.sqbc", ext,
						  sizeof(ext));

	zassert_equal(result, BT_GATT_OTS_OACP_RES_INV_PARAM,
		      "expected INV_PARAM, got %d", result);
}

ZTEST(ble_ots_parse, test_rejects_empty_name)
{
	char ext[EXT_CAP] = {0};
	int result = sq_ble_ots_parse_object_name("", ext, sizeof(ext));

	zassert_equal(result, BT_GATT_OTS_OACP_RES_INV_PARAM,
		      "expected INV_PARAM, got %d", result);
}

ZTEST(ble_ots_parse, test_rejects_extension_not_starting_with_dot)
{
	char ext[EXT_CAP] = {0};
	int result = sq_ble_ots_parse_object_name("sqbc", ext, sizeof(ext));

	zassert_equal(result, BT_GATT_OTS_OACP_RES_INV_PARAM,
		      "expected INV_PARAM, got %d", result);
}

ZTEST(ble_ots_parse, test_rejects_extension_longer_than_buffer)
{
	char ext[4] = {0};
	int result = sq_ble_ots_parse_object_name(".thisisalongish", ext, sizeof(ext));

	zassert_equal(result, BT_GATT_OTS_OACP_RES_INV_PARAM,
		      "expected INV_PARAM, got %d", result);
}

ZTEST(ble_ots_parse, test_rejects_null_name)
{
	char ext[EXT_CAP] = {0};
	int result = sq_ble_ots_parse_object_name(NULL, ext, sizeof(ext));

	zassert_true(result < 0, "expected negative error from NULL name, got %d", result);
}
