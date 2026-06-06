#ifndef SQUIDSCRIPT_BLE_L2CAP_PROBE_H
#define SQUIDSCRIPT_BLE_L2CAP_PROBE_H

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Throwaway L2CAP connection-oriented-channel sink for the transport spike.
 * Only compiled when CONFIG_SQUIDSCRIPT_BLE_L2CAP_PROBE=y. Registers a CoC
 * server on a fixed PSM and logs the bytes it receives so a host (e.g. a Linux
 * BlueZ raw L2CAP socket) can confirm it can drive LE CoC against this device.
 * Returns 0 on success, or 0 as a no-op when the probe is disabled.
 */
int sq_ble_l2cap_probe_init(void);

#ifdef __cplusplus
}
#endif

#endif /* SQUIDSCRIPT_BLE_L2CAP_PROBE_H */
