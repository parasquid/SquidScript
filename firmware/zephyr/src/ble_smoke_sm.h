#ifndef SQUIDSCRIPT_BLE_SMOKE_SM_H
#define SQUIDSCRIPT_BLE_SMOKE_SM_H

#include <stdint.h>

struct sq_ble_smoke_adv_api {
	int (*start)(void);
	int (*stop)(void);
};

enum sq_ble_smoke_state {
	SQ_BLE_SMOKE_STATE_IDLE = 0,
	SQ_BLE_SMOKE_STATE_ADVERTISING,
	SQ_BLE_SMOKE_STATE_RESTART_PENDING,
};

void sq_ble_smoke_sm_install_api(const struct sq_ble_smoke_adv_api *api);
void sq_ble_smoke_sm_reset(void);
int sq_ble_smoke_sm_begin_advertising(void);
int sq_ble_smoke_sm_stop_advertising(void);
int sq_ble_smoke_sm_handle_disconnect(void);
int sq_ble_smoke_sm_run_restart(void);
enum sq_ble_smoke_state sq_ble_smoke_sm_get_state(void);

#endif
