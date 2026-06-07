#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include <zephyr/kernel.h>
#include <zephyr/ztest.h>

#include "ble_smoke.h"
#include "ble_smoke_sm.h"

#define SQ_TEST_ADV_CALL_LOG_MAX 8

static int sq_test_adv_start_calls;
static int sq_test_adv_stop_calls;
static int sq_test_adv_start_return;
static int sq_test_adv_stop_return;
static int sq_test_adv_start_fail_after;
static int sq_test_adv_stop_fail_after;
static int sq_test_adv_call_log_count;
static char sq_test_adv_call_log[SQ_TEST_ADV_CALL_LOG_MAX][8];

static void sq_test_record_call(const char *name)
{
	if (sq_test_adv_call_log_count >= SQ_TEST_ADV_CALL_LOG_MAX) {
		return;
	}
	strncpy(sq_test_adv_call_log[sq_test_adv_call_log_count], name,
		sizeof(sq_test_adv_call_log[0]) - 1);
	sq_test_adv_call_log[sq_test_adv_call_log_count]
		[sizeof(sq_test_adv_call_log[0]) - 1] = '\0';
	sq_test_adv_call_log_count++;
}

static int sq_test_adv_start(void)
{
	sq_test_adv_start_calls++;
	sq_test_record_call("start");
	if (sq_test_adv_start_fail_after > 0 &&
	    sq_test_adv_start_calls > sq_test_adv_start_fail_after) {
		return -EAGAIN;
	}
	return sq_test_adv_start_return;
}

static int sq_test_adv_stop(void)
{
	sq_test_adv_stop_calls++;
	sq_test_record_call("stop");
	if (sq_test_adv_stop_fail_after > 0 &&
	    sq_test_adv_stop_calls > sq_test_adv_stop_fail_after) {
		return -EBUSY;
	}
	return sq_test_adv_stop_return;
}

static const struct sq_ble_smoke_adv_api sq_test_api = {
	.start = sq_test_adv_start,
	.stop = sq_test_adv_stop,
};

static void sq_test_reset(void)
{
	sq_test_adv_start_calls = 0;
	sq_test_adv_stop_calls = 0;
	sq_test_adv_start_return = 0;
	sq_test_adv_stop_return = 0;
	sq_test_adv_start_fail_after = 0;
	sq_test_adv_stop_fail_after = 0;
	sq_test_adv_call_log_count = 0;
	memset(sq_test_adv_call_log, 0, sizeof(sq_test_adv_call_log));
	sq_ble_smoke_sm_reset();
	sq_ble_smoke_sm_install_api(&sq_test_api);
}

ZTEST_SUITE(ble_smoke, NULL, NULL, NULL, NULL, NULL);

ZTEST(ble_smoke, test_initial_state_is_idle)
{
	sq_test_reset();
	zassert_equal(sq_ble_smoke_sm_get_state(), SQ_BLE_SMOKE_STATE_IDLE);
}

ZTEST(ble_smoke, test_begin_advertising_transitions_to_advertising_on_success)
{
	sq_test_reset();
	zassert_equal(sq_ble_smoke_sm_begin_advertising(), 0);
	zassert_equal(sq_ble_smoke_sm_get_state(), SQ_BLE_SMOKE_STATE_ADVERTISING);
	zassert_equal(sq_test_adv_start_calls, 1);
	zassert_equal(sq_test_adv_stop_calls, 0);
}

ZTEST(ble_smoke, test_begin_advertising_stays_idle_on_failure)
{
	sq_test_reset();
	sq_test_adv_start_return = -EAGAIN;
	zassert_equal(sq_ble_smoke_sm_begin_advertising(), -EAGAIN);
	zassert_equal(sq_ble_smoke_sm_get_state(), SQ_BLE_SMOKE_STATE_IDLE);
	zassert_equal(sq_test_adv_start_calls, 1);
}

ZTEST(ble_smoke, test_stop_advertising_returns_to_idle)
{
	sq_test_reset();
	zassert_equal(sq_ble_smoke_sm_begin_advertising(), 0);
	zassert_equal(sq_ble_smoke_sm_get_state(), SQ_BLE_SMOKE_STATE_ADVERTISING);
	zassert_equal(sq_ble_smoke_sm_stop_advertising(), 0);
	zassert_equal(sq_ble_smoke_sm_get_state(), SQ_BLE_SMOKE_STATE_IDLE);
	zassert_equal(sq_test_adv_stop_calls, 1);
}

ZTEST(ble_smoke, test_stop_advertising_when_already_stopped_is_ok)
{
	sq_test_reset();
	sq_test_adv_stop_return = -EALREADY;
	zassert_equal(sq_ble_smoke_sm_stop_advertising(), 0);
	zassert_equal(sq_ble_smoke_sm_get_state(), SQ_BLE_SMOKE_STATE_IDLE);
}

ZTEST(ble_smoke, test_disconnect_schedules_restart_work)
{
	sq_test_reset();
	zassert_equal(sq_ble_smoke_sm_begin_advertising(), 0);
	zassert_equal(sq_ble_smoke_sm_handle_disconnect(), 0);
	zassert_equal(sq_ble_smoke_sm_get_state(), SQ_BLE_SMOKE_STATE_RESTART_PENDING);
}

ZTEST(ble_smoke, test_restart_work_calls_stop_before_start)
{
	sq_test_reset();
	zassert_equal(sq_ble_smoke_sm_begin_advertising(), 0);
	zassert_equal(sq_ble_smoke_sm_handle_disconnect(), 0);
	zassert_equal(sq_ble_smoke_sm_run_restart(), 0);
	zassert_equal(sq_test_adv_stop_calls, 1);
	zassert_equal(sq_test_adv_start_calls, 2);
	zassert_equal(sq_test_adv_call_log_count, 3);
	zassert_str_equal(sq_test_adv_call_log[0], "start");
	zassert_str_equal(sq_test_adv_call_log[1], "stop");
	zassert_str_equal(sq_test_adv_call_log[2], "start");
	zassert_equal(sq_ble_smoke_sm_get_state(), SQ_BLE_SMOKE_STATE_ADVERTISING);
}

ZTEST(ble_smoke, test_restart_work_transitions_to_idle_on_start_failure)
{
	sq_test_reset();
	zassert_equal(sq_ble_smoke_sm_begin_advertising(), 0);
	zassert_equal(sq_ble_smoke_sm_handle_disconnect(), 0);
	sq_test_adv_start_return = -EAGAIN;
	zassert_equal(sq_ble_smoke_sm_run_restart(), 0);
	zassert_equal(sq_ble_smoke_sm_get_state(), SQ_BLE_SMOKE_STATE_IDLE);
}

ZTEST(ble_smoke, test_restart_work_transitions_to_advertising_on_already_stop)
{
	sq_test_reset();
	zassert_equal(sq_ble_smoke_sm_begin_advertising(), 0);
	zassert_equal(sq_ble_smoke_sm_handle_disconnect(), 0);
	sq_test_adv_stop_return = -EALREADY;
	zassert_equal(sq_ble_smoke_sm_run_restart(), 0);
	zassert_equal(sq_test_adv_start_calls, 2);
	zassert_equal(sq_ble_smoke_sm_get_state(), SQ_BLE_SMOKE_STATE_ADVERTISING);
}

ZTEST(ble_smoke, test_repeated_disconnects_keep_one_pending_restart_state)
{
	sq_test_reset();
	zassert_equal(sq_ble_smoke_sm_begin_advertising(), 0);
	zassert_equal(sq_ble_smoke_sm_handle_disconnect(), 0);
	zassert_equal(sq_ble_smoke_sm_handle_disconnect(), 0);
	zassert_equal(sq_ble_smoke_sm_get_state(), SQ_BLE_SMOKE_STATE_RESTART_PENDING);
	zassert_equal(sq_ble_smoke_sm_run_restart(), 0);
	zassert_equal(sq_test_adv_stop_calls, 1);
	zassert_equal(sq_test_adv_start_calls, 2);
	zassert_equal(sq_ble_smoke_sm_get_state(), SQ_BLE_SMOKE_STATE_ADVERTISING);
}

ZTEST(ble_smoke, test_reset_clears_pending_restart)
{
	sq_test_reset();
	zassert_equal(sq_ble_smoke_sm_begin_advertising(), 0);
	zassert_equal(sq_ble_smoke_sm_handle_disconnect(), 0);
	zassert_equal(sq_ble_smoke_sm_get_state(), SQ_BLE_SMOKE_STATE_RESTART_PENDING);
	sq_ble_smoke_sm_reset();
	zassert_equal(sq_ble_smoke_sm_get_state(), SQ_BLE_SMOKE_STATE_IDLE);
}
