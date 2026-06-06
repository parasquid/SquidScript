#include "ble_l2cap_probe.h"

#include <zephyr/sys/util.h>

#if IS_ENABLED(CONFIG_SQUIDSCRIPT_BLE_L2CAP_PROBE)

#include <zephyr/bluetooth/conn.h>
#include <zephyr/bluetooth/l2cap.h>
#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/net_buf.h>
#include <string.h>

LOG_MODULE_REGISTER(squidscript_ble_l2cap_probe, LOG_LEVEL_INF);

/* Zephyr bt_ots transfers object content over an L2CAP CoC on PSM 0x0025; the
 * probe reuses that PSM so the host-side driver is identical to the eventual
 * OTS data path.
 */
#ifndef SQ_BLE_L2CAP_PROBE_PSM
#define SQ_BLE_L2CAP_PROBE_PSM 0x0025
#endif

#define SQ_BLE_L2CAP_PROBE_MTU 256

NET_BUF_POOL_DEFINE(sq_l2cap_probe_pool, 4, BT_L2CAP_SDU_BUF_SIZE(SQ_BLE_L2CAP_PROBE_MTU), 8, NULL);

static struct bt_l2cap_le_chan sq_l2cap_probe_chan;
static size_t sq_l2cap_probe_total;

static struct net_buf *sq_l2cap_probe_alloc_buf(struct bt_l2cap_chan *chan)
{
	ARG_UNUSED(chan);
	return net_buf_alloc(&sq_l2cap_probe_pool, K_NO_WAIT);
}

static int sq_l2cap_probe_recv(struct bt_l2cap_chan *chan, struct net_buf *buf)
{
	ARG_UNUSED(chan);
	sq_l2cap_probe_total += buf->len;
	LOG_INF("L2CAP CoC recv %u bytes (total %zu)", buf->len, sq_l2cap_probe_total);
	return 0;
}

static void sq_l2cap_probe_connected(struct bt_l2cap_chan *chan)
{
	ARG_UNUSED(chan);
	sq_l2cap_probe_total = 0;
	LOG_INF("L2CAP CoC channel connected");
}

static void sq_l2cap_probe_disconnected(struct bt_l2cap_chan *chan)
{
	ARG_UNUSED(chan);
	LOG_INF("L2CAP CoC channel disconnected (total %zu bytes)", sq_l2cap_probe_total);
}

static const struct bt_l2cap_chan_ops sq_l2cap_probe_ops = {
	.alloc_buf = sq_l2cap_probe_alloc_buf,
	.recv = sq_l2cap_probe_recv,
	.connected = sq_l2cap_probe_connected,
	.disconnected = sq_l2cap_probe_disconnected,
};

static int sq_l2cap_probe_accept(struct bt_conn *conn, struct bt_l2cap_server *server,
				 struct bt_l2cap_chan **chan)
{
	ARG_UNUSED(conn);
	ARG_UNUSED(server);
	memset(&sq_l2cap_probe_chan, 0, sizeof(sq_l2cap_probe_chan));
	sq_l2cap_probe_chan.chan.ops = &sq_l2cap_probe_ops;
	sq_l2cap_probe_chan.rx.mtu = SQ_BLE_L2CAP_PROBE_MTU;
	*chan = &sq_l2cap_probe_chan.chan;
	return 0;
}

static struct bt_l2cap_server sq_l2cap_probe_server = {
	.psm = SQ_BLE_L2CAP_PROBE_PSM,
	.sec_level = BT_SECURITY_L1,
	.accept = sq_l2cap_probe_accept,
};

int sq_ble_l2cap_probe_init(void)
{
	int result = bt_l2cap_server_register(&sq_l2cap_probe_server);

	if (result != 0) {
		LOG_ERR("L2CAP CoC probe server register failed: %d", result);
		return result;
	}
	LOG_INF("L2CAP CoC probe server registered on PSM 0x%04x", SQ_BLE_L2CAP_PROBE_PSM);
	return 0;
}

#else /* !CONFIG_SQUIDSCRIPT_BLE_L2CAP_PROBE */

int sq_ble_l2cap_probe_init(void)
{
	return 0;
}

#endif
