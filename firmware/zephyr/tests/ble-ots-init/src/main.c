#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include <zephyr/bluetooth/bluetooth.h>
#include <zephyr/bluetooth/services/ots.h>
#include <zephyr/kernel.h>
#include <zephyr/ztest.h>

#include "ble_ots.h"

ZTEST_SUITE(ble_ots_init, NULL, NULL, NULL, NULL, NULL);

ZTEST(ble_ots_init, test_ots_service_registers_via_init)
{
	int result = sq_ble_ots_init();

	zassert_equal(result, 0, "expected 0 from sq_ble_ots_init, got %d", result);
}

ZTEST(ble_ots_init, test_ots_service_decl_handle_is_non_null)
{
	int result = sq_ble_ots_init();

	zassert_equal(result, 0, "expected 0 from sq_ble_ots_init, got %d", result);
	zassert_not_null(sq_ble_ots_svc_decl_get(),
			 "expected non-NULL OTS service declaration handle");
}

ZTEST(ble_ots_init, test_init_is_idempotent)
{
	zassert_equal(sq_ble_ots_init(), 0);
	zassert_equal(sq_ble_ots_init(), 0, "second sq_ble_ots_init should be a no-op");
	zassert_not_null(sq_ble_ots_svc_decl_get());
}

ZTEST(ble_ots_init, test_obj_created_stub_rejects_when_no_profile_armed)
{
	int result;

	zassert_equal(sq_ble_ots_init(), 0);
	result = sq_ble_ots_test_invoke_obj_created(NULL, 0, NULL, NULL);
	zassert_true(result < 0, "expected negative error from obj_created stub, got %d",
		     result);
}

ZTEST(ble_ots_init, test_obj_write_stub_accepts)
{
	zassert_equal(sq_ble_ots_init(), 0);
	zassert_equal(sq_ble_ots_test_invoke_obj_write(NULL, 0, "data", 4, 0, 0), 4,
		      "obj_write stub should return len on success");
}
