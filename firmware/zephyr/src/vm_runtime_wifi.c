#include "vm_runtime_internal.h"

#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
#include <zephyr/net/dhcpv4.h>
#include <zephyr/net/dhcpv4_server.h>
#include <zephyr/net/net_if.h>
#include <zephyr/net/net_mgmt.h>
#include <zephyr/net/net_ip.h>
#include <zephyr/net/wifi_mgmt.h>
#if IS_ENABLED(CONFIG_WIFI_NM)
#include <zephyr/net/wifi_nm.h>
#endif
#endif

#define SQ_VM_RUNTIME_WIFI_SCAN_TIMEOUT_MS 8000
#define SQ_VM_RUNTIME_WIFI_CONNECT_TIMEOUT_MS 15000
#define SQ_VM_RUNTIME_WIFI_DISCONNECT_TIMEOUT_MS 5000
#define SQ_VM_RUNTIME_WIFI_AP_IP "192.168.4.1"
#define SQ_VM_RUNTIME_WIFI_AP_NETMASK "255.255.255.0"
#define SQ_VM_RUNTIME_WIFI_AP_DHCP_POOL_START_OFFSET 10

#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
static struct sq_vm_runtime *runtime_wifi_scan_active_runtime;
#endif

static void runtime_wifi_error_operation(SqvmWifiOperation *out, const char *error)
{
	memset(out, 0, sizeof(*out));
	out->active = false;
	out->done = true;
	out->ok = false;
	out->state = (const uint8_t *)"error";
	out->state_len = strlen("error");
	out->error = (const uint8_t *)error;
	out->error_len = strlen(error);
}

int sq_vm_runtime_wifi_format_bssid(const uint8_t *mac, size_t mac_len, char *out, size_t out_len)
{
	if (mac == NULL || out == NULL) {
		return -EINVAL;
	}
	if (mac_len < 6) {
		return -EINVAL;
	}
	if (out_len < SQ_VM_RUNTIME_WIFI_BSSID_LEN) {
		return -ENOSPC;
	}
	int written = snprintf(out, out_len, "%02x:%02x:%02x:%02x:%02x:%02x", mac[0], mac[1],
			       mac[2], mac[3], mac[4], mac[5]);
	return written == SQ_VM_RUNTIME_WIFI_BSSID_LEN - 1 ? 0 : -EIO;
}

void sq_vm_runtime_wifi_note_ap_sta_connected(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL) {
		return;
	}
	runtime->wifi_ap_sta_connected_events++;
	if (runtime->wifi_ap_clients < INT32_MAX) {
		runtime->wifi_ap_clients++;
	}
}

void sq_vm_runtime_wifi_note_ap_sta_disconnected(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL) {
		return;
	}
	runtime->wifi_ap_sta_disconnected_events++;
	if (runtime->wifi_ap_clients > 0) {
		runtime->wifi_ap_clients--;
	}
}

const char *sq_vm_runtime_wifi_service_state_text(enum sq_vm_runtime_wifi_service_state state)
{
	switch (state) {
	case SQ_VM_RUNTIME_WIFI_SERVICE_IDLE:
		return "idle";
	case SQ_VM_RUNTIME_WIFI_SERVICE_SCANNING:
		return "scanning";
	case SQ_VM_RUNTIME_WIFI_SERVICE_CONNECTING:
		return "connecting";
	case SQ_VM_RUNTIME_WIFI_SERVICE_CONNECTED:
		return "connected";
	case SQ_VM_RUNTIME_WIFI_SERVICE_DISCONNECTING:
		return "disconnecting";
	case SQ_VM_RUNTIME_WIFI_SERVICE_AP_STARTING:
		return "apStarting";
	case SQ_VM_RUNTIME_WIFI_SERVICE_AP_STARTED:
		return "apStarted";
	case SQ_VM_RUNTIME_WIFI_SERVICE_AP_STOPPING:
		return "apStopping";
	case SQ_VM_RUNTIME_WIFI_SERVICE_ERROR:
		return "error";
	default:
		return "unknown";
	}
}

void sq_vm_runtime_wifi_service_begin(struct sq_vm_runtime *runtime,
				      enum sq_vm_runtime_wifi_op_kind kind,
				      enum sq_vm_runtime_wifi_service_state state,
				      int64_t timeout_ms)
{
	if (runtime == NULL) {
		return;
	}
	runtime->wifi_service_state = state;
	runtime->wifi_op_kind = kind;
	runtime->wifi_op_active = true;
	runtime->wifi_op_done = false;
	runtime->wifi_op_cancelled = false;
	runtime->wifi_op_ok = true;
	runtime->wifi_op_error = NULL;
	runtime->wifi_op_deadline_ms = k_uptime_get() + timeout_ms;
}

void sq_vm_runtime_wifi_service_finish(struct sq_vm_runtime *runtime,
				       enum sq_vm_runtime_wifi_service_state state,
				       bool ok, const char *error)
{
	if (runtime == NULL) {
		return;
	}
	runtime->wifi_service_state = state;
	runtime->wifi_op_active = true;
	runtime->wifi_op_done = true;
	runtime->wifi_op_cancelled = false;
	runtime->wifi_op_ok = ok;
	runtime->wifi_op_error = ok ? NULL : error;
}

void sq_vm_runtime_wifi_service_cancel(struct sq_vm_runtime *runtime,
				       enum sq_vm_runtime_wifi_service_state state)
{
	if (runtime == NULL) {
		return;
	}
	runtime->wifi_service_state = state;
	runtime->wifi_op_active = true;
	runtime->wifi_op_done = true;
	runtime->wifi_op_cancelled = true;
	runtime->wifi_op_ok = true;
	runtime->wifi_op_error = NULL;
}

bool sq_vm_runtime_wifi_service_busy(const struct sq_vm_runtime *runtime)
{
	return runtime != NULL && runtime->wifi_op_active && !runtime->wifi_op_done;
}

static bool runtime_wifi_valid_profile_name(const uint8_t *profile, size_t profile_len)
{
	if (profile == NULL || profile_len == 0 ||
	    profile_len > SQ_VM_RUNTIME_WIFI_PROFILE_NAME_BYTES) {
		return false;
	}
	for (size_t i = 0; i < profile_len; i++) {
		uint8_t ch = profile[i];
		if (!((ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') ||
		      (ch >= '0' && ch <= '9') || ch == '-' || ch == '_')) {
			return false;
		}
	}
	return true;
}

int sq_vm_runtime_set_wifi_profile(struct sq_vm_runtime *runtime, const uint8_t *profile,
				   size_t profile_len, const uint8_t *ssid, size_t ssid_len,
				   const uint8_t *password, size_t password_len)
{
	if (runtime == NULL || !runtime_wifi_valid_profile_name(profile, profile_len) ||
	    ssid == NULL || ssid_len == 0 || ssid_len > SQ_VM_RUNTIME_WIFI_PROFILE_SSID_BYTES ||
	    password == NULL || password_len > SQ_VM_RUNTIME_WIFI_PROFILE_PASSWORD_BYTES) {
		return -EINVAL;
	}
	memset(runtime->wifi_profile, 0, sizeof(runtime->wifi_profile));
	memset(runtime->wifi_profile_ssid, 0, sizeof(runtime->wifi_profile_ssid));
	memset(runtime->wifi_profile_password, 0, sizeof(runtime->wifi_profile_password));
	memcpy(runtime->wifi_profile, profile, profile_len);
	runtime->wifi_profile_len = profile_len;
	memcpy(runtime->wifi_profile_ssid, ssid, ssid_len);
	runtime->wifi_profile_ssid_len = ssid_len;
	memcpy(runtime->wifi_profile_password, password, password_len);
	runtime->wifi_profile_password_len = password_len;
	return 0;
}

#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
static const char *runtime_wifi_public_service_state_text(
	enum sq_vm_runtime_wifi_service_state state)
{
	switch (state) {
	case SQ_VM_RUNTIME_WIFI_SERVICE_SCANNING:
		return "configuring";
	case SQ_VM_RUNTIME_WIFI_SERVICE_CONNECTING:
	case SQ_VM_RUNTIME_WIFI_SERVICE_AP_STARTING:
		return "starting";
	case SQ_VM_RUNTIME_WIFI_SERVICE_CONNECTED:
	case SQ_VM_RUNTIME_WIFI_SERVICE_AP_STARTED:
		return "started";
	case SQ_VM_RUNTIME_WIFI_SERVICE_DISCONNECTING:
	case SQ_VM_RUNTIME_WIFI_SERVICE_AP_STOPPING:
		return "stopping";
	case SQ_VM_RUNTIME_WIFI_SERVICE_ERROR:
		return "error";
	case SQ_VM_RUNTIME_WIFI_SERVICE_IDLE:
	default:
		return "idle";
	}
}

static const char *runtime_wifi_op_kind_text(enum sq_vm_runtime_wifi_op_kind kind)
{
	switch (kind) {
	case SQ_VM_RUNTIME_WIFI_OP_START_AP:
		return "startAP";
	case SQ_VM_RUNTIME_WIFI_OP_STOP_AP:
		return "stopAP";
	case SQ_VM_RUNTIME_WIFI_OP_CONNECT:
		return "connect";
	case SQ_VM_RUNTIME_WIFI_OP_DISCONNECT:
		return "disconnect";
	case SQ_VM_RUNTIME_WIFI_OP_SCAN:
		return "scan";
	case SQ_VM_RUNTIME_WIFI_OP_NONE:
	default:
		return NULL;
	}
}

static void copy_text(char *out, size_t out_len, const char *text)
{
	if (out == NULL || out_len == 0) {
		return;
	}
	if (text == NULL) {
		text = "";
	}
	size_t len = strlen(text);
	if (len >= out_len) {
		len = out_len - 1;
	}
	memcpy(out, text, len);
	out[len] = '\0';
}

static void runtime_wifi_reset_scan(struct sq_vm_runtime *runtime)
{
	struct sq_vm_runtime_wifi_scan_scratch *scan = &runtime->wifi_scan;

	memset(scan, 0, sizeof(*scan));
	runtime->wifi_scan_count = 0;
	runtime->wifi_scan_status = 0;
	runtime->wifi_scan_collecting = false;
	runtime->wifi_scan_done = false;
}

static struct net_if *runtime_wifi_iface(void);

static void runtime_wifi_fill_operation(struct sq_vm_runtime *runtime, SqvmWifiOperation *out)
{
	const char *kind = runtime_wifi_op_kind_text(runtime->wifi_op_kind);
	const char *state = "idle";

	memset(out, 0, sizeof(*out));
	if (runtime->wifi_op_active) {
		state = runtime->wifi_op_done ?
				(runtime->wifi_op_cancelled ? "cancelled" :
				 runtime->wifi_op_ok ? "done" : "error") :
				"running";
	}
	out->active = runtime->wifi_op_active;
	if (kind != NULL) {
		out->kind = (const uint8_t *)kind;
		out->kind_len = strlen(kind);
	}
	out->state = (const uint8_t *)state;
	out->state_len = strlen(state);
	out->done = runtime->wifi_op_done;
	out->cancelled = runtime->wifi_op_cancelled;
	out->ok = runtime->wifi_op_ok;
	if (runtime->wifi_op_error != NULL) {
		out->error = (const uint8_t *)runtime->wifi_op_error;
		out->error_len = strlen(runtime->wifi_op_error);
	}
}

static void runtime_wifi_complete_if_ready(struct sq_vm_runtime *runtime)
{
	struct net_if *iface;
	struct wifi_iface_status status = {0};

	if (runtime == NULL || !runtime->wifi_op_active || runtime->wifi_op_done) {
		return;
	}
	if (k_uptime_get() > runtime->wifi_op_deadline_ms) {
		sq_vm_runtime_wifi_service_finish(runtime, SQ_VM_RUNTIME_WIFI_SERVICE_ERROR,
						  false, "timeout");
		if (runtime->wifi_op_kind == SQ_VM_RUNTIME_WIFI_OP_SCAN) {
			runtime->wifi_scan_collecting = false;
			runtime_wifi_scan_active_runtime = NULL;
		}
		return;
	}
	switch (runtime->wifi_op_kind) {
	case SQ_VM_RUNTIME_WIFI_OP_SCAN:
		if (runtime->wifi_scan_done) {
			sq_vm_runtime_wifi_service_finish(
				runtime,
				runtime->wifi_scan_status == 0 ? SQ_VM_RUNTIME_WIFI_SERVICE_IDLE :
								 SQ_VM_RUNTIME_WIFI_SERVICE_ERROR,
				runtime->wifi_scan_status == 0, "scan failed");
		}
		break;
	case SQ_VM_RUNTIME_WIFI_OP_CONNECT:
		if (runtime->wifi_station_connect_done) {
			if (runtime->wifi_station_connect_status != 0) {
				sq_vm_runtime_wifi_service_finish(
					runtime, SQ_VM_RUNTIME_WIFI_SERVICE_ERROR, false,
					"connect failed");
				break;
			}
			iface = runtime_wifi_iface();
			if (iface != NULL &&
			    net_mgmt(NET_REQUEST_WIFI_IFACE_STATUS, iface, &status, sizeof(status)) ==
				    0 &&
			    status.state == WIFI_STATE_COMPLETED) {
				net_dhcpv4_start(iface);
				sq_vm_runtime_wifi_service_finish(
					runtime, SQ_VM_RUNTIME_WIFI_SERVICE_CONNECTED, true,
					NULL);
			}
		}
		break;
	case SQ_VM_RUNTIME_WIFI_OP_DISCONNECT:
		if (runtime->wifi_station_disconnect_done) {
			if (runtime->wifi_station_disconnect_status == 0) {
				iface = runtime_wifi_iface();
				if (iface != NULL) {
					net_dhcpv4_stop(iface);
				}
				sq_vm_runtime_wifi_service_finish(
					runtime, SQ_VM_RUNTIME_WIFI_SERVICE_IDLE, true, NULL);
			} else {
				sq_vm_runtime_wifi_service_finish(
					runtime, SQ_VM_RUNTIME_WIFI_SERVICE_ERROR, false,
					"disconnect failed");
			}
		}
		break;
	default:
		break;
	}
}

static struct net_if *runtime_wifi_iface(void)
{
	return net_if_get_wifi_sta();
}

static struct net_if *runtime_wifi_ap_iface(void)
{
	return net_if_get_wifi_sap();
}

static const struct wifi_mgmt_ops *runtime_wifi_driver_ops(struct net_if *iface)
{
	const struct device *dev;
	const struct net_wifi_mgmt_offload *off_api;

	if (iface == NULL || !net_if_is_wifi(iface)) {
		return NULL;
	}
	dev = net_if_get_device(iface);
	if (dev == NULL) {
		return NULL;
	}
#if IS_ENABLED(CONFIG_WIFI_NM)
	struct wifi_nm_instance *nm = wifi_nm_get_instance_iface(iface);
	if (nm != NULL && nm->ops != NULL) {
		return nm->ops;
	}
#endif
	off_api = (const struct net_wifi_mgmt_offload *)dev->api;
	return off_api == NULL ? NULL : off_api->wifi_mgmt_api;
}

static bool runtime_wifi_profile_matches(const struct sq_vm_runtime *runtime,
					 const uint8_t *profile, size_t profile_len)
{
	return runtime != NULL && profile != NULL && profile_len == runtime->wifi_profile_len &&
	       profile_len > 0 &&
	       memcmp(runtime->wifi_profile, profile, profile_len) == 0;
}

static int runtime_wifi_configure_ap_ipv4(struct net_if *iface)
{
	struct in_addr addr = {0};
	struct in_addr netmask = {0};

	if (iface == NULL) {
		return -ENODEV;
	}
	if (net_addr_pton(AF_INET, SQ_VM_RUNTIME_WIFI_AP_IP, &addr) != 0 ||
	    net_addr_pton(AF_INET, SQ_VM_RUNTIME_WIFI_AP_NETMASK, &netmask) != 0) {
		return -EINVAL;
	}
	net_if_ipv4_set_gw(iface, &addr);
	(void)net_if_ipv4_addr_add(iface, &addr, NET_ADDR_MANUAL, 0);
	if (!net_if_ipv4_set_netmask_by_addr(iface, &addr, &netmask)) {
		return -EIO;
	}
	return 0;
}

static int runtime_wifi_start_ap_dhcp(struct net_if *iface)
{
	struct net_in_addr pool = {0};

	if (iface == NULL) {
		return -ENODEV;
	}
	if (net_addr_pton(AF_INET, SQ_VM_RUNTIME_WIFI_AP_IP, &pool) != 0) {
		return -EINVAL;
	}
	pool.s4_addr[3] += SQ_VM_RUNTIME_WIFI_AP_DHCP_POOL_START_OFFSET;
	int result = net_dhcpv4_server_start(iface, &pool);
	return result == -EALREADY ? 0 : result;
}

static int runtime_wifi_stop_ap_dhcp(struct net_if *iface)
{
	int result = net_dhcpv4_server_stop(iface);

	return result == -ENOENT ? 0 : result;
}

static void runtime_wifi_count_ap_lease(struct net_if *iface, struct dhcpv4_addr_slot *lease,
					void *user_data)
{
	ARG_UNUSED(iface);
	int32_t *count = user_data;

	if (count == NULL || lease == NULL) {
		return;
	}
	if (lease->state == DHCPV4_SERVER_ADDR_ALLOCATED && *count < INT32_MAX) {
		(*count)++;
	}
}

static int32_t runtime_wifi_ap_lease_count(struct net_if *iface)
{
	int32_t count = 0;

	if (iface == NULL) {
		return 0;
	}
	if (net_dhcpv4_server_foreach_lease(iface, runtime_wifi_count_ap_lease, &count) != 0) {
		return 0;
	}
	return count;
}

static void runtime_wifi_record_station_ipv4(struct sq_vm_runtime *runtime, struct net_if *iface,
					     SqvmWifiStatus *out)
{
	struct net_in_addr *addr;

	if (runtime == NULL || iface == NULL || out == NULL) {
		return;
	}
	memset(runtime->wifi_station_ip, 0, sizeof(runtime->wifi_station_ip));
	addr = net_if_ipv4_get_global_addr(iface, NET_ADDR_PREFERRED);
	if (addr == NULL) {
		return;
	}
	if (net_addr_ntop(NET_AF_INET, addr, runtime->wifi_station_ip,
			  sizeof(runtime->wifi_station_ip)) == NULL) {
		return;
	}
	out->ip_address = (const uint8_t *)runtime->wifi_station_ip;
	out->ip_address_len = strlen(runtime->wifi_station_ip);
}

static void runtime_wifi_record_scan_result(struct sq_vm_runtime *runtime,
					    const struct wifi_scan_result *entry)
{
	if (runtime == NULL || entry == NULL ||
	    runtime->wifi_scan_count >= SQVM_WIFI_SCAN_MAX_NETWORKS) {
		return;
	}

	size_t index = runtime->wifi_scan_count;
	struct sq_vm_runtime_wifi_scan_scratch *scan = &runtime->wifi_scan;
	SqvmWifiAccessPoint *network = &scan->networks[index];
	size_t ssid_len = entry->ssid_length;
	if (ssid_len >= SQ_VM_RUNTIME_WIFI_SSID_LEN) {
		ssid_len = SQ_VM_RUNTIME_WIFI_SSID_LEN - 1;
	}
	memcpy(scan->ssids[index], entry->ssid, ssid_len);
	scan->ssids[index][ssid_len] = '\0';
	copy_text(scan->auth[index], sizeof(scan->auth[index]), wifi_security_txt(entry->security));

	network->ssid = (const uint8_t *)scan->ssids[index];
	network->ssid_len = ssid_len;
	network->bssid = NULL;
	network->bssid_len = 0;
	network->ssid_length = entry->ssid_length;
	network->channel = entry->channel;
	network->rssi = entry->rssi;
	network->auth = (const uint8_t *)scan->auth[index];
	network->auth_len = strlen(scan->auth[index]);
	network->hidden = entry->ssid_length == 0;
	runtime->wifi_scan_count++;
}

static void runtime_wifi_scan_driver_callback(struct net_if *iface, int status,
					      struct wifi_scan_result *entry)
{
	ARG_UNUSED(iface);
	struct sq_vm_runtime *runtime = runtime_wifi_scan_active_runtime;

	if (runtime == NULL) {
		return;
	}
	if (entry != NULL) {
		if (runtime->wifi_scan_collecting) {
			runtime_wifi_record_scan_result(runtime, entry);
		}
		return;
	}
	runtime->wifi_scan_status = status;
	runtime->wifi_scan_collecting = false;
	runtime->wifi_scan_done = true;
	runtime_wifi_scan_active_runtime = NULL;
}

static void runtime_wifi_event_handler(struct net_mgmt_event_callback *cb, uint64_t mgmt_event,
				       struct net_if *iface)
{
	ARG_UNUSED(iface);
	struct sq_vm_runtime *runtime = CONTAINER_OF(cb, struct sq_vm_runtime, wifi_mgmt_cb);

	switch (mgmt_event) {
	case NET_EVENT_WIFI_SCAN_RESULT:
		runtime_wifi_record_scan_result(runtime, cb->info);
		break;
	case NET_EVENT_WIFI_SCAN_DONE:
		if (cb->info != NULL && cb->info_length >= sizeof(struct wifi_status)) {
			const struct wifi_status *status = cb->info;
			runtime->wifi_scan_status = status->status;
		}
		runtime->wifi_scan_collecting = false;
		runtime->wifi_scan_done = true;
		runtime_wifi_scan_active_runtime = NULL;
		break;
	case NET_EVENT_WIFI_CONNECT_RESULT:
		if (cb->info != NULL && cb->info_length >= sizeof(struct wifi_status)) {
			const struct wifi_status *status = cb->info;
			runtime->wifi_station_connect_status = status->status;
		}
		runtime->wifi_station_connect_done = true;
		break;
	case NET_EVENT_WIFI_DISCONNECT_RESULT:
		if (cb->info != NULL && cb->info_length >= sizeof(struct wifi_status)) {
			const struct wifi_status *status = cb->info;
			runtime->wifi_station_disconnect_status = status->status;
		}
		runtime->wifi_station_disconnect_done = true;
		break;
	case NET_EVENT_WIFI_AP_ENABLE_RESULT:
		runtime->wifi_ap_active = true;
		runtime->wifi_service_state = SQ_VM_RUNTIME_WIFI_SERVICE_AP_STARTED;
		runtime->wifi_ap_start_events++;
		break;
	case NET_EVENT_WIFI_AP_DISABLE_RESULT:
		runtime->wifi_ap_active = false;
		runtime->wifi_ap_clients = 0;
		runtime->wifi_service_state = SQ_VM_RUNTIME_WIFI_SERVICE_IDLE;
		runtime->wifi_ap_stop_events++;
		break;
	case NET_EVENT_WIFI_AP_STA_CONNECTED:
		sq_vm_runtime_wifi_note_ap_sta_connected(runtime);
		break;
	case NET_EVENT_WIFI_AP_STA_DISCONNECTED:
		sq_vm_runtime_wifi_note_ap_sta_disconnected(runtime);
		break;
	default:
		break;
	}
}

static void runtime_wifi_init_events(struct sq_vm_runtime *runtime)
{
	if (!runtime->wifi_mgmt_cb_registered) {
		net_mgmt_init_event_callback(&runtime->wifi_mgmt_cb, runtime_wifi_event_handler,
					     NET_EVENT_WIFI_SCAN_RESULT |
						     NET_EVENT_WIFI_SCAN_DONE |
						     NET_EVENT_WIFI_CONNECT_RESULT |
						     NET_EVENT_WIFI_DISCONNECT_RESULT |
						     NET_EVENT_WIFI_AP_ENABLE_RESULT |
						     NET_EVENT_WIFI_AP_DISABLE_RESULT |
						     NET_EVENT_WIFI_AP_STA_CONNECTED |
						     NET_EVENT_WIFI_AP_STA_DISCONNECTED);
		net_mgmt_add_event_callback(&runtime->wifi_mgmt_cb);
		runtime->wifi_mgmt_cb_registered = true;
	}
}

static bool runtime_wifi_state_blocks_scan(int state)
{
	return state == WIFI_STATE_SCANNING || state == WIFI_STATE_AUTHENTICATING ||
	       state == WIFI_STATE_ASSOCIATING || state == WIFI_STATE_ASSOCIATED ||
	       state == WIFI_STATE_4WAY_HANDSHAKE || state == WIFI_STATE_GROUP_HANDSHAKE ||
	       state == WIFI_STATE_COMPLETED;
}
#endif

static bool runtime_wifi_reset_needs_target_cleanup(const struct sq_vm_runtime *runtime)
{
	if (runtime == NULL) {
		return false;
	}
	if (runtime->wifi_op_active || runtime->wifi_service_state != SQ_VM_RUNTIME_WIFI_SERVICE_IDLE ||
	    runtime->wifi_ap_clients > 0 || runtime->wifi_ap_sta_connected_events > 0 ||
	    runtime->wifi_ap_sta_disconnected_events > 0) {
		return true;
	}
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	if (runtime_wifi_scan_active_runtime == runtime || runtime->wifi_ap_active ||
	    runtime->wifi_ap_start_events > 0 || runtime->wifi_ap_stop_events > 0) {
		return true;
	}
#endif
	return false;
}

#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
static bool runtime_wifi_station_may_be_active(const struct sq_vm_runtime *runtime)
{
	if (runtime == NULL) {
		return false;
	}
	switch (runtime->wifi_service_state) {
	case SQ_VM_RUNTIME_WIFI_SERVICE_CONNECTING:
	case SQ_VM_RUNTIME_WIFI_SERVICE_CONNECTED:
	case SQ_VM_RUNTIME_WIFI_SERVICE_DISCONNECTING:
		return true;
	default:
		break;
	}
	switch (runtime->wifi_op_kind) {
	case SQ_VM_RUNTIME_WIFI_OP_CONNECT:
	case SQ_VM_RUNTIME_WIFI_OP_DISCONNECT:
		return true;
	default:
		return false;
	}
}
#endif

void __weak sq_vm_runtime_wifi_reset_platform(struct sq_vm_runtime *runtime)
{
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	if (runtime == NULL) {
		return;
	}
	if (runtime_wifi_scan_active_runtime == runtime) {
		runtime->wifi_scan_collecting = false;
		runtime_wifi_scan_active_runtime = NULL;
	}
	struct net_if *ap_iface = runtime_wifi_ap_iface();
	if (ap_iface != NULL &&
	    (runtime->wifi_ap_active ||
	     runtime->wifi_service_state == SQ_VM_RUNTIME_WIFI_SERVICE_AP_STARTING ||
	     runtime->wifi_service_state == SQ_VM_RUNTIME_WIFI_SERVICE_AP_STARTED ||
	     runtime->wifi_service_state == SQ_VM_RUNTIME_WIFI_SERVICE_AP_STOPPING)) {
		(void)runtime_wifi_stop_ap_dhcp(ap_iface);
		(void)net_mgmt(NET_REQUEST_WIFI_AP_DISABLE, ap_iface, NULL, 0);
	}
	struct net_if *sta_iface = runtime_wifi_iface();
	if (sta_iface != NULL && runtime_wifi_station_may_be_active(runtime)) {
		net_dhcpv4_stop(sta_iface);
		(void)net_mgmt(NET_REQUEST_WIFI_DISCONNECT, sta_iface, NULL, 0);
	}
#else
	ARG_UNUSED(runtime);
#endif
}

void sq_vm_runtime_wifi_reset_target(struct sq_vm_runtime *runtime)
{
	if (!runtime_wifi_reset_needs_target_cleanup(runtime)) {
		return;
	}
	sq_vm_runtime_wifi_reset_platform(runtime);
}

#if !SQ_VM_RUNTIME_HAS_WIFI_MGMT
static int32_t runtime_wifi_unsupported_action(SqvmWifiOperation *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
	sqvm_wifi_operation_unsupported(out);
	return 0;
}
#endif

int32_t runtime_wifi_start_ap(void *user_data, const uint8_t *ssid, size_t ssid_len,
				     SqvmWifiOperation *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	struct sq_vm_runtime *runtime = user_data;
	struct net_if *iface = runtime_wifi_ap_iface();
	struct wifi_connect_req_params params = {0};

	if (runtime == NULL || iface == NULL) {
		runtime_wifi_error_operation(out, "unsupported");
		return 0;
	}
	if (ssid == NULL || ssid_len == 0 || ssid_len > SQ_VM_RUNTIME_WIFI_SSID_LEN - 1) {
		runtime_wifi_error_operation(out, "invalid ssid");
		return 0;
	}
	runtime_wifi_init_events(runtime);
	int ip_result = runtime_wifi_configure_ap_ipv4(iface);
	if (ip_result != 0) {
		runtime_wifi_error_operation(out, "ap ip failed");
		return 0;
	}
	sq_vm_runtime_wifi_service_begin(runtime, SQ_VM_RUNTIME_WIFI_OP_START_AP,
					 SQ_VM_RUNTIME_WIFI_SERVICE_AP_STARTING, 0);
	runtime->wifi_ap_clients = 0;

	params.ssid = ssid;
	params.ssid_length = (uint8_t)ssid_len;
	params.security = WIFI_SECURITY_TYPE_NONE;
	params.channel = WIFI_CHANNEL_ANY;
	params.band = WIFI_FREQ_BAND_2_4_GHZ;

	int result = net_mgmt(NET_REQUEST_WIFI_AP_ENABLE, iface, &params, sizeof(params));
	if (result != 0) {
		sq_vm_runtime_wifi_service_finish(runtime, SQ_VM_RUNTIME_WIFI_SERVICE_ERROR,
						  false, "ap start failed");
		runtime_wifi_fill_operation(runtime, out);
		return 0;
	}
	result = runtime_wifi_start_ap_dhcp(iface);
	if (result != 0) {
		(void)net_mgmt(NET_REQUEST_WIFI_AP_DISABLE, iface, NULL, 0);
		runtime->wifi_ap_active = false;
		sq_vm_runtime_wifi_service_finish(runtime, SQ_VM_RUNTIME_WIFI_SERVICE_ERROR,
						  false, "ap dhcp failed");
		runtime_wifi_fill_operation(runtime, out);
		return 0;
	}
	runtime->wifi_ap_active = true;
	sq_vm_runtime_wifi_service_finish(runtime, SQ_VM_RUNTIME_WIFI_SERVICE_AP_STARTED, true,
					  NULL);
	runtime_wifi_fill_operation(runtime, out);
	return 0;
#else
	ARG_UNUSED(user_data);
	ARG_UNUSED(ssid);
	ARG_UNUSED(ssid_len);

	return runtime_wifi_unsupported_action(out);
#endif
}

int32_t runtime_wifi_stop_ap(void *user_data, SqvmWifiOperation *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	struct sq_vm_runtime *runtime = user_data;
	struct net_if *iface = runtime_wifi_ap_iface();

	if (runtime == NULL || iface == NULL) {
		runtime_wifi_error_operation(out, "unsupported");
		return 0;
	}
	runtime_wifi_init_events(runtime);
	sq_vm_runtime_wifi_service_begin(runtime, SQ_VM_RUNTIME_WIFI_OP_STOP_AP,
					 SQ_VM_RUNTIME_WIFI_SERVICE_AP_STOPPING, 0);
	int result = runtime_wifi_stop_ap_dhcp(iface);
	if (result != 0) {
		sq_vm_runtime_wifi_service_finish(runtime, SQ_VM_RUNTIME_WIFI_SERVICE_ERROR,
						  false, "ap dhcp stop failed");
		runtime_wifi_fill_operation(runtime, out);
		return 0;
	}
	result = net_mgmt(NET_REQUEST_WIFI_AP_DISABLE, iface, NULL, 0);
	if (result != 0) {
		sq_vm_runtime_wifi_service_finish(runtime, SQ_VM_RUNTIME_WIFI_SERVICE_ERROR,
						  false, "ap stop failed");
		runtime_wifi_fill_operation(runtime, out);
		return 0;
	}
	runtime->wifi_ap_active = false;
	runtime->wifi_ap_clients = 0;
	sq_vm_runtime_wifi_service_finish(runtime, SQ_VM_RUNTIME_WIFI_SERVICE_IDLE, true,
					  NULL);
	runtime_wifi_fill_operation(runtime, out);
	return 0;
#else
	ARG_UNUSED(user_data);

	return runtime_wifi_unsupported_action(out);
#endif
}

int32_t runtime_wifi_connect(void *user_data, const uint8_t *profile, size_t profile_len,
				    SqvmWifiOperation *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	struct sq_vm_runtime *runtime = user_data;
	struct net_if *iface = runtime_wifi_iface();
	struct wifi_connect_req_params params = {0};

	if (runtime == NULL || iface == NULL) {
		runtime_wifi_error_operation(out, "unsupported");
		return 0;
	}
	if (!runtime_wifi_profile_matches(runtime, profile, profile_len)) {
		runtime_wifi_error_operation(out, "profile missing");
		return 0;
	}
	if (runtime->wifi_profile_password_len > 0 && runtime->wifi_profile_password_len < 8) {
		runtime_wifi_error_operation(out, "invalid password");
		return 0;
	}
	if (sq_vm_runtime_wifi_service_busy(runtime)) {
		runtime_wifi_error_operation(out, "wifi busy");
		return 0;
	}

	runtime_wifi_init_events(runtime);
	sq_vm_runtime_wifi_service_begin(runtime, SQ_VM_RUNTIME_WIFI_OP_CONNECT,
					 SQ_VM_RUNTIME_WIFI_SERVICE_CONNECTING,
					 SQ_VM_RUNTIME_WIFI_CONNECT_TIMEOUT_MS);
	runtime->wifi_station_connect_status = 0;
	runtime->wifi_station_connect_done = false;

	params.ssid = runtime->wifi_profile_ssid;
	params.ssid_length = (uint8_t)runtime->wifi_profile_ssid_len;
	params.channel = WIFI_CHANNEL_ANY;
	params.band = WIFI_FREQ_BAND_2_4_GHZ;
	params.timeout = SQ_VM_RUNTIME_WIFI_CONNECT_TIMEOUT_MS / 1000;
	params.mfp = WIFI_MFP_OPTIONAL;
	if (runtime->wifi_profile_password_len == 0) {
		params.security = WIFI_SECURITY_TYPE_NONE;
	} else {
		params.security = WIFI_SECURITY_TYPE_PSK;
		params.psk = runtime->wifi_profile_password;
		params.psk_length = (uint8_t)runtime->wifi_profile_password_len;
	}

	int result = net_mgmt(NET_REQUEST_WIFI_CONNECT, iface, &params, sizeof(params));
	if (result != 0) {
		sq_vm_runtime_wifi_service_finish(runtime, SQ_VM_RUNTIME_WIFI_SERVICE_ERROR,
						  false, "connect request failed");
		runtime_wifi_fill_operation(runtime, out);
		return 0;
	}
	runtime_wifi_fill_operation(runtime, out);
	return 0;
#else
	ARG_UNUSED(user_data);
	ARG_UNUSED(profile);
	ARG_UNUSED(profile_len);

	return runtime_wifi_unsupported_action(out);
#endif
}

int32_t runtime_wifi_disconnect(void *user_data, SqvmWifiOperation *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	struct sq_vm_runtime *runtime = user_data;
	struct net_if *iface = runtime_wifi_iface();

	if (runtime == NULL || iface == NULL) {
		runtime_wifi_error_operation(out, "unsupported");
		return 0;
	}
	if (sq_vm_runtime_wifi_service_busy(runtime)) {
		runtime_wifi_error_operation(out, "wifi busy");
		return 0;
	}

	runtime_wifi_init_events(runtime);
	sq_vm_runtime_wifi_service_begin(runtime, SQ_VM_RUNTIME_WIFI_OP_DISCONNECT,
					 SQ_VM_RUNTIME_WIFI_SERVICE_DISCONNECTING,
					 SQ_VM_RUNTIME_WIFI_DISCONNECT_TIMEOUT_MS);
	runtime->wifi_station_disconnect_status = 0;
	runtime->wifi_station_disconnect_done = false;

	int result = net_mgmt(NET_REQUEST_WIFI_DISCONNECT, iface, NULL, 0);
	if (result != 0) {
		sq_vm_runtime_wifi_service_finish(runtime, SQ_VM_RUNTIME_WIFI_SERVICE_ERROR,
						  false, "disconnect request failed");
		runtime_wifi_fill_operation(runtime, out);
		return 0;
	}
	runtime_wifi_fill_operation(runtime, out);
	return 0;
#else
	ARG_UNUSED(user_data);

	return runtime_wifi_unsupported_action(out);
#endif
}

int32_t runtime_wifi_get_ap_ip(void *user_data, SqvmWifiApIp *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	struct sq_vm_runtime *runtime = user_data;
	if (runtime == NULL || !runtime->wifi_ap_active) {
		SQ_SET_LITERAL_FIELD(out, error, "stopped");
		return 0;
	}
	SQ_SET_LITERAL_FIELD(out, ip, SQ_VM_RUNTIME_WIFI_AP_IP);
	SQ_SET_LITERAL_FIELD(out, gw, SQ_VM_RUNTIME_WIFI_AP_IP);
	SQ_SET_LITERAL_FIELD(out, netmask, SQ_VM_RUNTIME_WIFI_AP_NETMASK);
	return 0;
#else
	ARG_UNUSED(user_data);
	sqvm_wifi_ap_ip_unsupported(out);
	return 0;
#endif
}

int32_t runtime_wifi_status(void *user_data, SqvmWifiStatus *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	struct sq_vm_runtime *runtime = user_data;
	struct net_if *iface = runtime_wifi_iface();
	struct net_if *ap_iface = runtime_wifi_ap_iface();
	struct wifi_iface_status status = {0};
	if (runtime != NULL) {
		runtime_wifi_init_events(runtime);
	}
	SQ_SET_LITERAL_FIELD(out, backend, "zephyr");
	if (runtime != NULL && runtime->wifi_ap_active) {
		out->active = true;
		out->configured = true;
		out->driver_started = true;
		out->mode = (const uint8_t *)"ap";
		out->mode_len = 2;
		const char *service_state =
			runtime_wifi_public_service_state_text(runtime->wifi_service_state);
		out->state = (const uint8_t *)service_state;
		out->state_len = strlen(service_state);
		SQ_SET_LITERAL_FIELD(out, driver_mode, "ap");
		SQ_SET_LITERAL_FIELD(out, ip_address, SQ_VM_RUNTIME_WIFI_AP_IP);
		int32_t lease_count = runtime_wifi_ap_lease_count(ap_iface);
		out->clients = runtime->wifi_ap_clients > lease_count ? runtime->wifi_ap_clients :
								       lease_count;
		out->ap_start_events = runtime->wifi_ap_start_events;
		out->ap_stop_events = runtime->wifi_ap_stop_events;
		out->sta_connected_events = runtime->wifi_ap_sta_connected_events;
		out->sta_disconnected_events = runtime->wifi_ap_sta_disconnected_events;
		return 0;
	}
	if (iface == NULL) {
		out->active = false;
		out->driver_started = false;
		out->configured = false;
		SQ_SET_LITERAL_FIELD(out, state, "unavailable");
		SQ_SET_LITERAL_FIELD(out, error, "unsupported");
		return 0;
	}
	int result = net_mgmt(NET_REQUEST_WIFI_IFACE_STATUS, iface, &status, sizeof(status));
	if (result != 0) {
		out->active = false;
		out->driver_started = true;
		out->configured = false;
		SQ_SET_LITERAL_FIELD(out, state, "unavailable");
		SQ_SET_LITERAL_FIELD(out, error, "status unavailable");
		return 0;
	}
	out->active = status.state >= WIFI_STATE_ASSOCIATED;
	out->connected = status.state == WIFI_STATE_COMPLETED;
	out->driver_started = true;
	out->configured = status.state != WIFI_STATE_UNKNOWN &&
			  status.state != WIFI_STATE_INTERFACE_DISABLED;
	const char *service_state = runtime != NULL ?
					    runtime_wifi_public_service_state_text(
						    runtime->wifi_service_state) :
					    "idle";
	if (out->connected && runtime != NULL &&
	    runtime->wifi_service_state == SQ_VM_RUNTIME_WIFI_SERVICE_IDLE) {
		service_state = "started";
	}
	out->state = (const uint8_t *)service_state;
	out->state_len = strlen(service_state);
	SQ_SET_LITERAL_FIELD(out, backend, "zephyr");
	SQ_SET_LITERAL_FIELD(out, driver_mode, "station");
	if (runtime != NULL && runtime->wifi_profile_len > 0) {
		out->profile = (const uint8_t *)runtime->wifi_profile;
		out->profile_len = runtime->wifi_profile_len;
	}
	out->channel = status.channel;
	out->rssi = status.rssi;
	out->auth = (const uint8_t *)wifi_security_txt(status.security);
	out->auth_len = strlen(wifi_security_txt(status.security));
	if (out->connected) {
		runtime_wifi_record_station_ipv4(runtime, iface, out);
	}
	return 0;
#else
	ARG_UNUSED(user_data);
	out->active = false;
	SQ_SET_LITERAL_FIELD(out, state, "stopped");
	SQ_SET_LITERAL_FIELD(out, backend, "zephyr");
	out->driver_started = false;
	out->configured = false;
	SQ_SET_LITERAL_FIELD(out, error, "unsupported");
	return 0;
#endif
}

int32_t runtime_wifi_scan(void *user_data, SqvmWifiOperation *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	struct sq_vm_runtime *runtime = user_data;
	struct net_if *iface = runtime_wifi_iface();
	const struct wifi_mgmt_ops *wifi_mgmt_api;
	const struct device *dev;
	struct wifi_iface_status status = {0};
	struct wifi_scan_params params = {0};

	if (runtime == NULL || iface == NULL) {
		runtime_wifi_error_operation(out, "unsupported");
		return 0;
	}
	runtime_wifi_init_events(runtime);
	wifi_mgmt_api = runtime_wifi_driver_ops(iface);
	dev = net_if_get_device(iface);
	if (wifi_mgmt_api == NULL || wifi_mgmt_api->scan == NULL || dev == NULL ||
	    !net_if_is_admin_up(iface)) {
		runtime_wifi_error_operation(out, "unsupported");
		return 0;
	}
	int status_result = net_mgmt(NET_REQUEST_WIFI_IFACE_STATUS, iface, &status, sizeof(status));
	if (sq_vm_runtime_wifi_service_busy(runtime) ||
	    runtime_wifi_scan_active_runtime != NULL || runtime->wifi_ap_active ||
	    (status_result == 0 && runtime_wifi_state_blocks_scan(status.state))) {
		runtime_wifi_error_operation(out, "wifi busy");
		return 0;
	}
	runtime_wifi_reset_scan(runtime);
	sq_vm_runtime_wifi_service_begin(runtime, SQ_VM_RUNTIME_WIFI_OP_SCAN,
					 SQ_VM_RUNTIME_WIFI_SERVICE_SCANNING,
					 SQ_VM_RUNTIME_WIFI_SCAN_TIMEOUT_MS);
	runtime->wifi_scan_collecting = true;
	runtime_wifi_scan_active_runtime = runtime;
	int result = wifi_mgmt_api->scan(dev, iface, &params, runtime_wifi_scan_driver_callback);
	if (result != 0) {
		runtime->wifi_scan_collecting = false;
		runtime_wifi_scan_active_runtime = NULL;
		if (result == -EINPROGRESS || result == -EBUSY || result == -EALREADY) {
			sq_vm_runtime_wifi_service_finish(runtime,
							  SQ_VM_RUNTIME_WIFI_SERVICE_ERROR,
							  false, "wifi busy");
		} else {
			sq_vm_runtime_wifi_service_finish(runtime,
							  SQ_VM_RUNTIME_WIFI_SERVICE_ERROR,
							  false, "driver error");
		}
		runtime_wifi_fill_operation(runtime, out);
		return 0;
	}
	runtime_wifi_fill_operation(runtime, out);
	return 0;
#else
	ARG_UNUSED(user_data);
	runtime_wifi_error_operation(out, "unsupported");
	return 0;
#endif
}

int32_t runtime_wifi_operation(void *user_data, SqvmWifiOperation *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	struct sq_vm_runtime *runtime = user_data;
	if (runtime == NULL) {
		runtime_wifi_error_operation(out, "unsupported");
		return 0;
	}
	runtime_wifi_complete_if_ready(runtime);
	runtime_wifi_fill_operation(runtime, out);
	return 0;
#else
	ARG_UNUSED(user_data);
	sqvm_wifi_operation_idle(out);
	return 0;
#endif
}

int32_t runtime_wifi_result(void *user_data, SqvmWifiOperationResult *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	struct sq_vm_runtime *runtime = user_data;
	if (runtime == NULL) {
		sqvm_wifi_operation_result_unsupported(out);
		return 0;
	}
	runtime_wifi_complete_if_ready(runtime);
	const char *kind = runtime_wifi_op_kind_text(runtime->wifi_op_kind);
	out->ready = runtime->wifi_op_done;
	out->ok = runtime->wifi_op_ok;
	out->cancelled = runtime->wifi_op_cancelled;
	out->count = runtime->wifi_op_kind == SQ_VM_RUNTIME_WIFI_OP_SCAN ?
			     (int32_t)runtime->wifi_scan_count :
			     0;
	SQ_SET_LITERAL_FIELD(out, state, "idle");
	if (runtime->wifi_op_active) {
		const char *state = runtime->wifi_op_done ?
					    (runtime->wifi_op_cancelled ? "cancelled" :
					     runtime->wifi_op_ok ? "done" : "error") :
					    "running";
		out->state = (const uint8_t *)state;
		out->state_len = strlen(state);
	}
	if (kind != NULL) {
		out->kind = (const uint8_t *)kind;
		out->kind_len = strlen(kind);
	}
	if (runtime->wifi_op_error != NULL) {
		out->error = (const uint8_t *)runtime->wifi_op_error;
		out->error_len = strlen(runtime->wifi_op_error);
	}
	return 0;
#else
	ARG_UNUSED(user_data);
	sqvm_wifi_operation_result_unsupported(out);
	return 0;
#endif
}

int32_t runtime_wifi_cancel(void *user_data, SqvmWifiOperation *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	struct sq_vm_runtime *runtime = user_data;
	if (runtime == NULL) {
		runtime_wifi_error_operation(out, "unsupported");
		return 0;
	}
	if (!runtime->wifi_op_active || runtime->wifi_op_done) {
		sqvm_wifi_operation_idle(out);
		return 0;
	}
	if (runtime->wifi_op_kind == SQ_VM_RUNTIME_WIFI_OP_SCAN) {
		runtime->wifi_scan_collecting = false;
		runtime_wifi_scan_active_runtime = NULL;
		sq_vm_runtime_wifi_service_cancel(runtime, SQ_VM_RUNTIME_WIFI_SERVICE_IDLE);
	} else {
		sq_vm_runtime_wifi_service_cancel(runtime, runtime->wifi_service_state);
	}
	runtime_wifi_fill_operation(runtime, out);
	return 0;
#else
	ARG_UNUSED(user_data);
	sqvm_wifi_operation_idle(out);
	return 0;
#endif
}

int32_t runtime_wifi_scan_network(void *user_data, int32_t index,
					 SqvmWifiScanNetworkResult *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	struct sq_vm_runtime *runtime = user_data;
	if (runtime == NULL || index < 0 || (size_t)index >= runtime->wifi_scan_count) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "not found");
		return 0;
	}
	out->ok = true;
	out->network = runtime->wifi_scan.networks[index];
	return 0;
#else
	ARG_UNUSED(user_data);
	ARG_UNUSED(index);
	sqvm_wifi_scan_network_unsupported(out);
	return 0;
#endif
}
