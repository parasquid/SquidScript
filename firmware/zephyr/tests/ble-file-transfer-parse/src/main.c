#include <errno.h>
#include <string.h>

#include <zephyr/ztest.h>

#include "ble_file_transfer_core.h"

#define EXT_CAP 16

ZTEST_SUITE(ble_file_transfer_parse, NULL, NULL, NULL, NULL, NULL);

ZTEST(ble_file_transfer_parse, test_parses_extension_only_name)
{
	char ext[EXT_CAP] = {0};
	int result = sq_ble_file_transfer_parse_file_name(".sqbc", ext, sizeof(ext));

	zassert_equal(result, 0, "expected 0, got %d", result);
	zassert_str_equal(ext, ".sqbc", "extension mismatch");
}

ZTEST(ble_file_transfer_parse, test_rejects_routed_segments)
{
	char ext[EXT_CAP] = {0};
	int result = sq_ble_file_transfer_parse_file_name("break-reminder/wallpaper/.sqbc", ext,
						  sizeof(ext));

	zassert_equal(result, SQ_BLE_FILE_TRANSFER_RES_INV_PARAM,
		      "expected INV_PARAM, got %d", result);
}

ZTEST(ble_file_transfer_parse, test_rejects_empty_name)
{
	char ext[EXT_CAP] = {0};
	int result = sq_ble_file_transfer_parse_file_name("", ext, sizeof(ext));

	zassert_equal(result, SQ_BLE_FILE_TRANSFER_RES_INV_PARAM,
		      "expected INV_PARAM, got %d", result);
}

ZTEST(ble_file_transfer_parse, test_rejects_extension_not_starting_with_dot)
{
	char ext[EXT_CAP] = {0};
	int result = sq_ble_file_transfer_parse_file_name("sqbc", ext, sizeof(ext));

	zassert_equal(result, SQ_BLE_FILE_TRANSFER_RES_INV_PARAM,
		      "expected INV_PARAM, got %d", result);
}

ZTEST(ble_file_transfer_parse, test_rejects_extension_longer_than_buffer)
{
	char ext[4] = {0};
	int result = sq_ble_file_transfer_parse_file_name(".thisisalongish", ext, sizeof(ext));

	zassert_equal(result, SQ_BLE_FILE_TRANSFER_RES_INV_PARAM,
		      "expected INV_PARAM, got %d", result);
}

ZTEST(ble_file_transfer_parse, test_rejects_null_name)
{
	char ext[EXT_CAP] = {0};
	int result = sq_ble_file_transfer_parse_file_name(NULL, ext, sizeof(ext));

	zassert_true(result < 0, "expected negative error from NULL name, got %d", result);
}
