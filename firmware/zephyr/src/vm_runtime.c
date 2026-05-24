#include "vm_runtime.h"

#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <stddef.h>

#include <zephyr/devicetree.h>
#include <zephyr/drivers/gpio.h>
#include <zephyr/drivers/pwm.h>
#include <zephyr/fs/fs.h>
#include <zephyr/sys/sys_heap.h>
#if IS_ENABLED(CONFIG_NET_L2_WIFI_MGMT) && IS_ENABLED(CONFIG_NET_MGMT_EVENT) && \
	IS_ENABLED(CONFIG_NET_MGMT_EVENT_INFO)
#include <zephyr/net/dhcpv4.h>
#include <zephyr/net/dhcpv4_server.h>
#include <zephyr/net/net_if.h>
#include <zephyr/net/net_mgmt.h>
#include <zephyr/net/net_ip.h>
#include <zephyr/net/wifi_mgmt.h>
#endif

#define SQ_VM_RUNTIME_BREATHE_LEVEL_MS 31
#define SQ_SET_LITERAL_FIELD(target, field, value) \
	do { \
		(target)->field = (const uint8_t *)(value); \
		(target)->field##_len = sizeof(value) - 1; \
	} while (false)

static size_t bounded_strlen(const char *value, size_t cap)
{
	size_t len = 0;

	while (len < cap && value[len] != '\0') {
		len++;
	}
	return len;
}

static int parse_gpio_name(const uint8_t *name, size_t name_len, uint8_t *pin);
static int configure_raw_gpio(struct sq_vm_runtime *runtime, uint8_t pin);

static const uint8_t indicator_breathe_duties[SQ_VM_RUNTIME_INDICATOR_BREATHE_STEPS] = {
	0,  0,  1,  2,	4,  6,  8,  11, 15, 18, 22, 26, 31, 35, 40, 45, 50,
	55, 60, 65, 69, 74, 78, 82, 85, 89, 92, 94, 96, 98, 99, 100, 100, 100,
	99, 98, 96, 94, 92, 89, 85, 82, 78, 74, 69, 65, 60, 55, 50, 45, 40,
	35, 31, 26, 22, 18, 15, 11, 8,  6,  4,  2,	1,  0,  0,
};

#if IS_ENABLED(CONFIG_PWM) && DT_NODE_HAS_PROP(DT_ALIAS(indicator0), pwms)
static const struct pwm_dt_spec indicator_pwm = PWM_DT_SPEC_GET(DT_ALIAS(indicator0));
#define SQ_VM_RUNTIME_HAS_INDICATOR_PWM 1
#else
#define SQ_VM_RUNTIME_HAS_INDICATOR_PWM 0
#endif

#if IS_ENABLED(CONFIG_GPIO) && DT_NODE_HAS_PROP(DT_ALIAS(indicator0), gpios)
#define SQ_VM_RUNTIME_INDICATOR_GPIO_NODE DT_ALIAS(indicator0)
#elif IS_ENABLED(CONFIG_GPIO) && DT_NODE_HAS_PROP(DT_ALIAS(led0), gpios)
#define SQ_VM_RUNTIME_INDICATOR_GPIO_NODE DT_ALIAS(led0)
#endif

#ifdef SQ_VM_RUNTIME_INDICATOR_GPIO_NODE
static const struct gpio_dt_spec indicator_gpio =
	GPIO_DT_SPEC_GET(SQ_VM_RUNTIME_INDICATOR_GPIO_NODE, gpios);
#define SQ_VM_RUNTIME_HAS_INDICATOR_GPIO 1
#else
#define SQ_VM_RUNTIME_HAS_INDICATOR_GPIO 0
#endif

#if IS_ENABLED(CONFIG_GPIO) && DT_NODE_HAS_STATUS(DT_NODELABEL(gpio0), okay)
static const struct device *const gpio0_dev = DEVICE_DT_GET(DT_NODELABEL(gpio0));
#define SQ_VM_RUNTIME_HAS_GPIO0 1
#else
#define SQ_VM_RUNTIME_HAS_GPIO0 0
#endif

#if IS_ENABLED(CONFIG_NET_L2_WIFI_MGMT) && IS_ENABLED(CONFIG_NET_MGMT_EVENT) && \
	IS_ENABLED(CONFIG_NET_MGMT_EVENT_INFO)
#define SQ_VM_RUNTIME_HAS_WIFI_MGMT 1
#define SQ_VM_RUNTIME_WIFI_SCAN_TIMEOUT_MS 8000
#define SQ_VM_RUNTIME_WIFI_CONNECT_TIMEOUT_MS 15000
#define SQ_VM_RUNTIME_WIFI_DISCONNECT_TIMEOUT_MS 5000
#define SQ_VM_RUNTIME_WIFI_AP_IP "192.168.4.1"
#define SQ_VM_RUNTIME_WIFI_AP_NETMASK "255.255.255.0"
#define SQ_VM_RUNTIME_WIFI_AP_DHCP_POOL_START_OFFSET 10
#else
#define SQ_VM_RUNTIME_HAS_WIFI_MGMT 0
#endif

K_THREAD_STACK_DEFINE(sq_vm_runtime_work_stack, SQ_VM_RUNTIME_WORK_STACK_SIZE);
static struct k_work_q sq_vm_runtime_work_q;
static bool sq_vm_runtime_work_q_started;

static void runtime_trace(void *user_data, const uint8_t *message, size_t message_len)
{
	struct sq_vm_runtime *runtime = user_data;

	if (runtime->trace_count >= SQ_VM_RUNTIME_TRACE_MAX) {
		memmove(runtime->traces[0], runtime->traces[1],
			(SQ_VM_RUNTIME_TRACE_MAX - 1) * SQ_VM_RUNTIME_TRACE_LEN);
		runtime->trace_count = SQ_VM_RUNTIME_TRACE_MAX - 1;
	}

	size_t len = message_len;
	if (len >= SQ_VM_RUNTIME_TRACE_LEN) {
		len = SQ_VM_RUNTIME_TRACE_LEN - 1;
	}
	memcpy(runtime->traces[runtime->trace_count], message, len);
	runtime->traces[runtime->trace_count][len] = '\0';
	runtime->trace_count++;
}

static int32_t runtime_read_exact_at(void *user_data, size_t offset, uint8_t *out, size_t out_len)
{
	struct sq_vm_runtime *runtime = user_data;

	if (runtime->backend == NULL || runtime->backend->read_sqbc == NULL) {
		return -EINVAL;
	}
	return runtime->backend->read_sqbc(runtime->backend->user_data, offset, out, out_len);
}

static void runtime_debug_output(void *user_data, const uint8_t *message, size_t message_len)
{
	(void)sq_vm_runtime_record_output(user_data, message, message_len);
}

static void runtime_display_clear(void *user_data, const uint8_t *color, size_t color_len)
{
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];
	int written = snprintf(line, sizeof(line), "draw=clear color=%.*s", (int)color_len,
			       color == NULL ? (const uint8_t *)"" : color);

	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(user_data, line);
	}
}

static void runtime_display_text(void *user_data, const uint8_t *text, size_t text_len,
				 const SqvmDisplayTextOptions *options)
{
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];

	if (options == NULL) {
		return;
	}
	int written = snprintf(line, sizeof(line), "draw=text text=\"%.*s\" x=%d y=%d",
			       (int)text_len, text == NULL ? (const uint8_t *)"" : text,
			       options->x, options->y);
	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(user_data, line);
	}
}

static void runtime_display_rect(void *user_data, const SqvmDisplayRectOptions *options)
{
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];

	if (options == NULL) {
		return;
	}
	int written = snprintf(line, sizeof(line), "draw=rect x=%d y=%d w=%d h=%d", options->x,
			       options->y, options->w, options->h);
	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(user_data, line);
	}
}

static void runtime_display_line(void *user_data, const SqvmDisplayLineOptions *options)
{
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];

	if (options == NULL) {
		return;
	}
	int written = snprintf(line, sizeof(line), "draw=line x1=%d y1=%d x2=%d y2=%d",
			       options->x1, options->y1, options->x2, options->y2);
	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(user_data, line);
	}
}

static int32_t runtime_display_select(void *user_data, const uint8_t *name, size_t name_len)
{
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];
	int written = snprintf(line, sizeof(line), "draw=select name=%.*s", (int)name_len,
			       name == NULL ? (const uint8_t *)"" : name);

	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(user_data, line);
	}
	return 0;
}

static void runtime_display_image(void *user_data, const uint8_t *path, size_t path_len,
				  const SqvmDisplayResourceOptions *options)
{
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];

	if (options == NULL) {
		return;
	}
	int written = snprintf(line, sizeof(line), "draw=image path=\"%.*s\" x=%d y=%d",
			       (int)path_len, path == NULL ? (const uint8_t *)"" : path,
			       options->x, options->y);
	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(user_data, line);
	}
}

static void runtime_display_draw(void *user_data, const uint8_t *drawable, size_t drawable_len,
				 const SqvmDisplayResourceOptions *options)
{
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];

	if (options == NULL) {
		return;
	}
	int written = snprintf(line, sizeof(line), "draw=resource drawable=\"%.*s\" x=%d y=%d",
			       (int)drawable_len,
			       drawable == NULL ? (const uint8_t *)"" : drawable, options->x,
			       options->y);
	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(user_data, line);
	}
}

static int32_t runtime_indicator_write(void *user_data, bool value)
{
	return sq_vm_runtime_indicator_write(user_data, value);
}

static int32_t runtime_indicator_toggle(void *user_data)
{
	return sq_vm_runtime_indicator_toggle(user_data);
}

static int32_t runtime_indicator_read(void *user_data, bool *out)
{
	return sq_vm_runtime_indicator_read(user_data, out);
}

static int32_t runtime_indicator_breathe(void *user_data)
{
	return sq_vm_runtime_indicator_breathe(user_data);
}

static int32_t runtime_indicator_blink(void *user_data, int32_t on_ms, int32_t off_ms)
{
	return sq_vm_runtime_indicator_blink(user_data, on_ms, off_ms);
}

static int32_t runtime_hardware_gpio_write(void *user_data, const uint8_t *name, size_t name_len,
					   bool value)
{
	return sq_vm_runtime_hardware_gpio_write(user_data, name, name_len, value);
}

static int32_t runtime_hardware_gpio_toggle(void *user_data, const uint8_t *name, size_t name_len)
{
	return sq_vm_runtime_hardware_gpio_toggle(user_data, name, name_len);
}

static int32_t runtime_hardware_gpio_read(void *user_data, const uint8_t *name, size_t name_len,
					  bool *out)
{
	return sq_vm_runtime_hardware_gpio_read(user_data, name, name_len, out);
}

static int32_t runtime_app_lifecycle(void *user_data, const char *action, const uint8_t *app,
				     size_t app_len)
{
	struct sq_vm_runtime *runtime = user_data;
	char line[SQ_VM_RUNTIME_TRACE_LEN];

	if (runtime == NULL || action == NULL || (app == NULL && app_len > 0)) {
		return -EINVAL;
	}
	int written = snprintf(line, sizeof(line), "app.%s %.*s", action, (int)app_len,
			       app == NULL ? (const uint8_t *)"" : app);
	if (written > 0) {
		runtime_trace(runtime, (const uint8_t *)line, strlen(line));
	}
	return 0;
}

static int32_t runtime_app_launch(void *user_data, const uint8_t *app, size_t app_len)
{
	struct sq_vm_runtime *runtime = user_data;
	int result = runtime_app_lifecycle(user_data, "launch", app, app_len);

	if (result != 0) {
		return result;
	}
	if (runtime == NULL || app == NULL || app_len == 0 ||
	    app_len >= sizeof(runtime->pending_launch_app)) {
		return -EINVAL;
	}
	memcpy(runtime->pending_launch_app, app, app_len);
	runtime->pending_launch_app[app_len] = '\0';
	runtime->pending_launch_active = true;
	return 0;
}

static int32_t runtime_app_arm(void *user_data, const uint8_t *app, size_t app_len)
{
	struct sq_vm_runtime *runtime = user_data;
	int result = runtime_app_lifecycle(user_data, "arm", app, app_len);

	if (result != 0) {
		return result;
	}
	if (runtime == NULL || app == NULL || app_len == 0 ||
	    app_len >= sizeof(runtime->pending_arm_app)) {
		return -EINVAL;
	}
	memcpy(runtime->pending_arm_app, app, app_len);
	runtime->pending_arm_app[app_len] = '\0';
	runtime->pending_arm_active = true;
	return 0;
}

static int32_t runtime_app_disarm(void *user_data, const uint8_t *app, size_t app_len)
{
	int result = runtime_app_lifecycle(user_data, "disarm", app, app_len);

	if (result != 0) {
		return result;
	}
	struct sq_vm_runtime *runtime = user_data;
	if (runtime != NULL && app != NULL && strlen(runtime->pending_arm_app) == app_len &&
	    memcmp(runtime->pending_arm_app, app, app_len) == 0) {
		memset(runtime->pending_arm_app, 0, sizeof(runtime->pending_arm_app));
		runtime->pending_arm_active = false;
	}
	return sq_vm_runtime_clear_armed_app(user_data, app, app_len);
}

static void runtime_app_registry_entry_from_store(const struct sq_app_registry_entry *source,
						  SqvmAppRegistryEntry *out)
{
	size_t len;

	if (out == NULL) {
		return;
	}
	memset(out, 0, sizeof(*out));
	if (source == NULL) {
		return;
	}
	len = bounded_strlen(source->app_id, sizeof(source->app_id));
	out->id = (const uint8_t *)source->app_id;
	out->id_len = len;
	out->name = (const uint8_t *)source->app_id;
	out->name_len = len;
}

static int32_t runtime_app_registry_list(void *user_data, SqvmAppRegistryEntry *out,
					 size_t out_cap, size_t *out_count)
{
	struct sq_vm_runtime *runtime = user_data;
	size_t count;

	if (runtime == NULL || runtime->registry == NULL || out_count == NULL ||
	    (out == NULL && out_cap > 0)) {
		return -EINVAL;
	}
	count = runtime->registry->count;
	if (count > out_cap) {
		count = out_cap;
	}
	for (size_t i = 0; i < count; i++) {
		runtime_app_registry_entry_from_store(&runtime->registry->apps[i], &out[i]);
	}
	*out_count = count;
	return 0;
}

static int32_t runtime_app_registry_get(void *user_data, const uint8_t *app, size_t app_len,
					SqvmAppRegistryEntry *out)
{
	struct sq_vm_runtime *runtime = user_data;
	char app_id[SQ_APP_STORE_APP_ID_MAX];
	const struct sq_app_registry_entry *entry;

	if (runtime == NULL || runtime->registry == NULL || out == NULL || app == NULL ||
	    app_len == 0 || app_len >= sizeof(app_id)) {
		return -EINVAL;
	}
	memcpy(app_id, app, app_len);
	app_id[app_len] = '\0';
	entry = sq_app_registry_find(runtime->registry, app_id);
	if (entry == NULL) {
		return -ENOENT;
	}
	runtime_app_registry_entry_from_store(entry, out);
	return 0;
}

static int32_t runtime_app_process_stack(void *user_data, SqvmAppStackEntry *out, size_t out_cap,
					 size_t *out_count)
{
	struct sq_vm_runtime *runtime = user_data;
	size_t count;

	if (runtime == NULL || out_count == NULL || (out == NULL && out_cap > 0)) {
		return -EINVAL;
	}
	count = runtime->return_stack_count;
	if (count > out_cap) {
		count = out_cap;
	}
	for (size_t i = 0; i < count; i++) {
		size_t len = bounded_strlen(runtime->return_stack[i], SQ_APP_STORE_APP_ID_MAX);
		out[i].app_id = (const uint8_t *)runtime->return_stack[i];
		out[i].app_id_len = len;
		out[i].event = NULL;
		out[i].event_len = 0;
	}
	*out_count = count;
	return 0;
}

static int32_t runtime_app_armed_stack(void *user_data, SqvmAppStackEntry *out, size_t out_cap,
				       size_t *out_count)
{
	struct sq_vm_runtime *runtime = user_data;
	size_t count = 0;

	if (runtime == NULL || out_count == NULL || (out == NULL && out_cap > 0)) {
		return -EINVAL;
	}
	for (size_t i = 0; i < SQ_VM_RUNTIME_ARMED_TIMER_MAX && count < out_cap; i++) {
		const struct sq_vm_runtime_armed_timer *timer = &runtime->armed_timers[i];
		if (!timer->active) {
			continue;
		}
		out[count].app_id = (const uint8_t *)timer->app_id;
		out[count].app_id_len = bounded_strlen(timer->app_id, sizeof(timer->app_id));
		out[count].event = (const uint8_t *)timer->event;
		out[count].event_len = bounded_strlen(timer->event, sizeof(timer->event));
		count++;
	}
	*out_count = count;
	return 0;
}

static int32_t runtime_timer_every(void *user_data, const uint8_t *event, size_t event_len,
				   int32_t interval_ms)
{
	return sq_vm_runtime_register_timer(user_data, event, event_len, interval_ms, true);
}

static int32_t runtime_timer_after(void *user_data, const uint8_t *event, size_t event_len,
				  int32_t delay_ms)
{
	return sq_vm_runtime_register_timer(user_data, event, event_len, delay_ms, false);
}

static int32_t runtime_system_memory_text(void *user_data, uint8_t *out, size_t out_cap,
					  size_t *out_len)
{
	ARG_UNUSED(user_data);
	size_t heap_free_bytes = 0;
	size_t heap_allocated_bytes = 0;

	if (out == NULL || out_len == NULL || out_cap == 0) {
		return -EINVAL;
	}

#ifdef CONFIG_SYS_HEAP_RUNTIME_STATS
	struct k_heap *heaps = NULL;
	int heap_array_count = k_heap_array_get(&heaps);
	if (heap_array_count > 0 && heaps != NULL) {
		for (int i = 0; i < heap_array_count; i++) {
			struct sys_memory_stats stats;

			if (sys_heap_runtime_stats_get(&heaps[i].heap, &stats) == 0) {
				heap_free_bytes += stats.free_bytes;
				heap_allocated_bytes += stats.allocated_bytes;
			}
		}
	}
#endif

	int written = snprintf((char *)out, out_cap, "RAM %u KiB heap %zu B used %zu B free",
			       (unsigned int)CONFIG_SRAM_SIZE, heap_allocated_bytes,
			       heap_free_bytes);
	if (written <= 0 || (size_t)written >= out_cap) {
		return -ENOSPC;
	}
	*out_len = (size_t)written;
	return 0;
}

static int write_human_bytes(uint8_t *out, size_t out_cap, size_t *out_len, const char *label,
			     uint64_t bytes)
{
	int written;

	if (bytes >= 1024u * 1024u) {
		written = snprintf((char *)out, out_cap, "%s %llu MiB", label,
				   (unsigned long long)(bytes / (1024u * 1024u)));
	} else if (bytes >= 1024u) {
		written = snprintf((char *)out, out_cap, "%s %llu KiB", label,
				   (unsigned long long)(bytes / 1024u));
	} else {
		written = snprintf((char *)out, out_cap, "%s %llu B", label,
				   (unsigned long long)bytes);
	}
	if (written <= 0 || (size_t)written >= out_cap) {
		return -ENOSPC;
	}
	*out_len = (size_t)written;
	return 0;
}

static int32_t runtime_system_storage_text(void *user_data, const uint8_t *name, size_t name_len,
					   uint8_t *out, size_t out_cap, size_t *out_len)
{
	struct sq_vm_runtime *runtime = user_data;
	struct fs_statvfs stat;
	uint64_t free_bytes;

	if (runtime == NULL || name == NULL || out == NULL || out_len == NULL || out_cap == 0) {
		return -EINVAL;
	}
	if (name_len != 4 || memcmp(name, "apps", 4) != 0) {
		return -EINVAL;
	}
	if (runtime->store_mount_point == NULL) {
		return -ENODEV;
	}
	if (fs_statvfs(runtime->store_mount_point, &stat) != 0) {
		return -EIO;
	}
	free_bytes = (uint64_t)stat.f_bfree * (uint64_t)stat.f_frsize;
	return write_human_bytes(out, out_cap, out_len, "Apps", free_bytes);
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
	memset(runtime->wifi_scan_networks, 0, sizeof(runtime->wifi_scan_networks));
	memset(runtime->wifi_scan_ssids, 0, sizeof(runtime->wifi_scan_ssids));
	memset(runtime->wifi_scan_bssids, 0, sizeof(runtime->wifi_scan_bssids));
	memset(runtime->wifi_scan_auth, 0, sizeof(runtime->wifi_scan_auth));
	runtime->wifi_scan_count = 0;
	runtime->wifi_scan_status = 0;
}

static struct net_if *runtime_wifi_iface(void)
{
	return net_if_get_wifi_sta();
}

static struct net_if *runtime_wifi_ap_iface(void)
{
	return net_if_get_wifi_sap();
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
	SqvmWifiAccessPoint *network = &runtime->wifi_scan_networks[index];
	size_t ssid_len = entry->ssid_length;
	if (ssid_len >= SQ_VM_RUNTIME_WIFI_SSID_LEN) {
		ssid_len = SQ_VM_RUNTIME_WIFI_SSID_LEN - 1;
	}
	memcpy(runtime->wifi_scan_ssids[index], entry->ssid, ssid_len);
	runtime->wifi_scan_ssids[index][ssid_len] = '\0';
	if (entry->mac_length >= 6) {
		(void)sq_vm_runtime_wifi_format_bssid(entry->mac, entry->mac_length,
						      runtime->wifi_scan_bssids[index],
						      sizeof(runtime->wifi_scan_bssids[index]));
	}
	copy_text(runtime->wifi_scan_auth[index], sizeof(runtime->wifi_scan_auth[index]),
		  wifi_security_txt(entry->security));

	network->ssid = (const uint8_t *)runtime->wifi_scan_ssids[index];
	network->ssid_len = strlen(runtime->wifi_scan_ssids[index]);
	network->bssid = (const uint8_t *)runtime->wifi_scan_bssids[index];
	network->bssid_len = strlen(runtime->wifi_scan_bssids[index]);
	network->ssid_length = entry->ssid_length;
	network->channel = entry->channel;
	network->rssi = entry->rssi;
	network->auth = (const uint8_t *)runtime->wifi_scan_auth[index];
	network->auth_len = strlen(runtime->wifi_scan_auth[index]);
	network->hidden = entry->ssid_length == 0;
	runtime->wifi_scan_count++;
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
		k_sem_give(&runtime->wifi_scan_done);
		break;
	case NET_EVENT_WIFI_CONNECT_RESULT:
		if (cb->info != NULL && cb->info_length >= sizeof(struct wifi_status)) {
			const struct wifi_status *status = cb->info;
			runtime->wifi_station_connect_status = status->status;
		}
		k_sem_give(&runtime->wifi_station_connect_done);
		break;
	case NET_EVENT_WIFI_DISCONNECT_RESULT:
		if (cb->info != NULL && cb->info_length >= sizeof(struct wifi_status)) {
			const struct wifi_status *status = cb->info;
			runtime->wifi_station_disconnect_status = status->status;
		}
		k_sem_give(&runtime->wifi_station_disconnect_done);
		break;
	case NET_EVENT_WIFI_AP_ENABLE_RESULT:
		runtime->wifi_ap_active = true;
		runtime->wifi_ap_start_events++;
		break;
	case NET_EVENT_WIFI_AP_DISABLE_RESULT:
		runtime->wifi_ap_active = false;
		runtime->wifi_ap_stop_events++;
		break;
	default:
		break;
	}
}

static void runtime_wifi_init_events(struct sq_vm_runtime *runtime)
{
	if (!runtime->wifi_scan_sem_initialized) {
		k_sem_init(&runtime->wifi_scan_done, 0, 1);
		runtime->wifi_scan_sem_initialized = true;
	}
	if (!runtime->wifi_station_sem_initialized) {
		k_sem_init(&runtime->wifi_station_connect_done, 0, 1);
		k_sem_init(&runtime->wifi_station_disconnect_done, 0, 1);
		runtime->wifi_station_sem_initialized = true;
	}
	if (!runtime->wifi_mgmt_cb_registered) {
		net_mgmt_init_event_callback(&runtime->wifi_mgmt_cb, runtime_wifi_event_handler,
					     NET_EVENT_WIFI_SCAN_RESULT |
						     NET_EVENT_WIFI_SCAN_DONE |
						     NET_EVENT_WIFI_CONNECT_RESULT |
						     NET_EVENT_WIFI_DISCONNECT_RESULT |
						     NET_EVENT_WIFI_AP_ENABLE_RESULT |
						     NET_EVENT_WIFI_AP_DISABLE_RESULT);
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

#if !SQ_VM_RUNTIME_HAS_WIFI_MGMT
static int32_t runtime_wifi_unsupported_action(SqvmWifiActionResult *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
	out->ok = false;
	SQ_SET_LITERAL_FIELD(out, error, "unsupported");
	return 0;
}
#endif

static int32_t runtime_wifi_start_ap(void *user_data, const uint8_t *ssid, size_t ssid_len,
				     SqvmWifiActionResult *out)
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
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "unsupported");
		return 0;
	}
	if (ssid == NULL || ssid_len == 0 || ssid_len > SQ_VM_RUNTIME_WIFI_SSID_LEN - 1) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "invalid ssid");
		return 0;
	}
	runtime_wifi_init_events(runtime);
	int ip_result = runtime_wifi_configure_ap_ipv4(iface);
	if (ip_result != 0) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "ap ip failed");
		return 0;
	}

	params.ssid = ssid;
	params.ssid_length = (uint8_t)ssid_len;
	params.security = WIFI_SECURITY_TYPE_NONE;
	params.channel = WIFI_CHANNEL_ANY;
	params.band = WIFI_FREQ_BAND_2_4_GHZ;

	int result = net_mgmt(NET_REQUEST_WIFI_AP_ENABLE, iface, &params, sizeof(params));
	if (result != 0) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "ap start failed");
		return 0;
	}
	result = runtime_wifi_start_ap_dhcp(iface);
	if (result != 0) {
		(void)net_mgmt(NET_REQUEST_WIFI_AP_DISABLE, iface, NULL, 0);
		runtime->wifi_ap_active = false;
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "ap dhcp failed");
		return 0;
	}
	runtime->wifi_ap_active = true;
	out->ok = true;
	return 0;
#else
	ARG_UNUSED(user_data);
	ARG_UNUSED(ssid);
	ARG_UNUSED(ssid_len);

	return runtime_wifi_unsupported_action(out);
#endif
}

static int32_t runtime_wifi_stop_ap(void *user_data, SqvmWifiActionResult *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	struct sq_vm_runtime *runtime = user_data;
	struct net_if *iface = runtime_wifi_ap_iface();

	if (runtime == NULL || iface == NULL) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "unsupported");
		return 0;
	}
	runtime_wifi_init_events(runtime);
	int result = runtime_wifi_stop_ap_dhcp(iface);
	if (result != 0) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "ap dhcp stop failed");
		return 0;
	}
	result = net_mgmt(NET_REQUEST_WIFI_AP_DISABLE, iface, NULL, 0);
	if (result != 0) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "ap stop failed");
		return 0;
	}
	runtime->wifi_ap_active = false;
	out->ok = true;
	return 0;
#else
	ARG_UNUSED(user_data);

	return runtime_wifi_unsupported_action(out);
#endif
}

static int32_t runtime_wifi_connect(void *user_data, const uint8_t *profile, size_t profile_len,
				    SqvmWifiActionResult *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	struct sq_vm_runtime *runtime = user_data;
	struct net_if *iface = runtime_wifi_iface();
	struct wifi_connect_req_params params = {0};
	struct wifi_iface_status status = {0};

	if (runtime == NULL || iface == NULL) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "unsupported");
		return 0;
	}
	if (!runtime_wifi_profile_matches(runtime, profile, profile_len)) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "profile missing");
		return 0;
	}
	if (runtime->wifi_profile_password_len > 0 && runtime->wifi_profile_password_len < 8) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "invalid password");
		return 0;
	}

	runtime_wifi_init_events(runtime);
	k_sem_reset(&runtime->wifi_station_connect_done);
	runtime->wifi_station_connect_status = 0;

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
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "connect request failed");
		return 0;
	}
	if (k_sem_take(&runtime->wifi_station_connect_done,
		       K_MSEC(SQ_VM_RUNTIME_WIFI_CONNECT_TIMEOUT_MS)) != 0) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "connect timeout");
		return 0;
	}
	if (runtime->wifi_station_connect_status != 0) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "connect failed");
		return 0;
	}
	result = net_mgmt(NET_REQUEST_WIFI_IFACE_STATUS, iface, &status, sizeof(status));
	if (result != 0 || status.state != WIFI_STATE_COMPLETED) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "connect pending");
		return 0;
	}
	net_dhcpv4_start(iface);
	out->ok = true;
	return 0;
#else
	ARG_UNUSED(user_data);
	ARG_UNUSED(profile);
	ARG_UNUSED(profile_len);

	return runtime_wifi_unsupported_action(out);
#endif
}

static int32_t runtime_wifi_disconnect(void *user_data, SqvmWifiActionResult *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	struct sq_vm_runtime *runtime = user_data;
	struct net_if *iface = runtime_wifi_iface();

	if (runtime == NULL || iface == NULL) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "unsupported");
		return 0;
	}

	runtime_wifi_init_events(runtime);
	k_sem_reset(&runtime->wifi_station_disconnect_done);
	runtime->wifi_station_disconnect_status = 0;

	int result = net_mgmt(NET_REQUEST_WIFI_DISCONNECT, iface, NULL, 0);
	if (result != 0) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "disconnect request failed");
		return 0;
	}
	if (k_sem_take(&runtime->wifi_station_disconnect_done,
		       K_MSEC(SQ_VM_RUNTIME_WIFI_DISCONNECT_TIMEOUT_MS)) != 0) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "disconnect timeout");
		return 0;
	}
	if (runtime->wifi_station_disconnect_status != 0) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "disconnect failed");
		return 0;
	}
	net_dhcpv4_stop(iface);
	out->ok = true;
	return 0;
#else
	ARG_UNUSED(user_data);

	return runtime_wifi_unsupported_action(out);
#endif
}

static int32_t runtime_wifi_get_ap_ip(void *user_data, SqvmWifiApIp *out)
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
	SQ_SET_LITERAL_FIELD(out, error, "unsupported");
	return 0;
#endif
}

static int32_t runtime_wifi_status(void *user_data, SqvmWifiStatus *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	struct sq_vm_runtime *runtime = user_data;
	struct net_if *iface = runtime_wifi_iface();
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
		SQ_SET_LITERAL_FIELD(out, state, "started");
		SQ_SET_LITERAL_FIELD(out, driver_mode, "ap");
		SQ_SET_LITERAL_FIELD(out, ip_address, SQ_VM_RUNTIME_WIFI_AP_IP);
		out->ap_start_events = runtime->wifi_ap_start_events;
		out->ap_stop_events = runtime->wifi_ap_stop_events;
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
	out->state = (const uint8_t *)wifi_state_txt(status.state);
	out->state_len = strlen(wifi_state_txt(status.state));
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

static int32_t runtime_wifi_scan(void *user_data, SqvmWifiScanResult *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	struct sq_vm_runtime *runtime = user_data;
	struct net_if *iface = runtime_wifi_iface();
	struct wifi_iface_status status = {0};
	struct wifi_scan_params params = {0};

	if (runtime == NULL || iface == NULL) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "unsupported");
		return 0;
	}
	runtime_wifi_init_events(runtime);
	int status_result = net_mgmt(NET_REQUEST_WIFI_IFACE_STATUS, iface, &status, sizeof(status));
	if (status_result == 0 && runtime_wifi_state_blocks_scan(status.state)) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "wifi busy");
		return 0;
	}

	runtime_wifi_reset_scan(runtime);
	k_sem_reset(&runtime->wifi_scan_done);
	params.max_bss_cnt = SQVM_WIFI_SCAN_MAX_NETWORKS;
	int result = net_mgmt(NET_REQUEST_WIFI_SCAN, iface, &params, sizeof(params));
	if (result != 0) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "scan request failed");
		return 0;
	}
	if (k_sem_take(&runtime->wifi_scan_done, K_MSEC(SQ_VM_RUNTIME_WIFI_SCAN_TIMEOUT_MS)) != 0) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "scan timeout");
		return 0;
	}
	if (runtime->wifi_scan_status != 0) {
		out->ok = false;
		SQ_SET_LITERAL_FIELD(out, error, "scan failed");
		return 0;
	}
	out->ok = true;
	out->networks = runtime->wifi_scan_networks;
	out->network_count = runtime->wifi_scan_count;
	return 0;
#else
	ARG_UNUSED(user_data);
	out->ok = false;
	SQ_SET_LITERAL_FIELD(out, error, "unsupported");
	out->networks = NULL;
	out->network_count = 0;
	return 0;
#endif
}

static int32_t runtime_device_config_unsupported(SqvmDeviceConfigResult *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
	out->ok = false;
	SQ_SET_LITERAL_FIELD(out, error, "unsupported");
	return 0;
}

static int32_t runtime_device_config_error(SqvmDeviceConfigResult *out, const char *error)
{
	if (out == NULL || error == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
	out->ok = false;
	out->error = (const uint8_t *)error;
	out->error_len = strlen(error);
	return 0;
}

static int32_t runtime_device_config_ok(SqvmDeviceConfigResult *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
	out->ok = true;
	return 0;
}

static const char *runtime_device_config_status_error(SqdcStatus status)
{
	switch (status) {
	case SQDC_STATUS_OK:
		return NULL;
	case SQDC_STATUS_BUFFER_TOO_SMALL:
		return "buffer too small";
	case SQDC_STATUS_PARSE_ERROR:
		return "parse error";
	case SQDC_STATUS_TOO_MANY_RECORDS:
		return "too many records";
	case SQDC_STATUS_INVALID_ARGUMENT:
	default:
		return "invalid argument";
	}
}

static int runtime_device_config_read_file(const char *path, uint8_t *buffer, size_t buffer_len,
					   size_t *out_len)
{
	struct fs_dirent entry;
	struct fs_file_t file;
	int result;

	if (path == NULL || buffer == NULL || out_len == NULL) {
		return -EINVAL;
	}
	*out_len = 0;
	result = fs_stat(path, &entry);
	if (result != 0) {
		return result;
	}
	if (entry.type != FS_DIR_ENTRY_FILE) {
		return -ENOENT;
	}
	if (entry.size > buffer_len) {
		return -ENOSPC;
	}

	fs_file_t_init(&file);
	result = fs_open(&file, path, FS_O_READ);
	if (result != 0) {
		return result;
	}
	ssize_t read = fs_read(&file, buffer, entry.size);
	result = fs_close(&file);
	if (read < 0) {
		return (int)read;
	}
	if ((size_t)read != entry.size) {
		return -EIO;
	}
	*out_len = (size_t)read;
	return result;
}

static int runtime_device_config_write_file(const char *path, const uint8_t *bytes, size_t len)
{
	struct fs_file_t file;
	int result;
	ssize_t written;

	if (path == NULL || bytes == NULL || len == 0) {
		return -EINVAL;
	}

	fs_file_t_init(&file);
	result = fs_open(&file, path, FS_O_CREATE | FS_O_WRITE | FS_O_TRUNC);
	if (result != 0) {
		return result;
	}
	written = fs_write(&file, bytes, len);
	result = fs_close(&file);
	if (written < 0) {
		return (int)written;
	}
	if ((size_t)written != len) {
		return -EIO;
	}
	return result;
}

static int sq_vm_runtime_device_config_load_resource(struct sq_vm_runtime *runtime,
						     const uint8_t *resource_bytes,
						     size_t resource_len,
						     SqvmDeviceConfigResult *out)
{
	char resource[SQ_APP_STORE_PATH_MAX];
	char path[SQ_APP_STORE_PATH_MAX];
	size_t bytes_len;
	SqdcStatus status;
	int result;

	if (runtime == NULL || resource_bytes == NULL || out == NULL) {
		return -EINVAL;
	}
	if (runtime->store_mount_point == NULL || runtime->current_app[0] == '\0') {
		return runtime_device_config_error(out, "no current app");
	}

	status = sqdc_is_safe_sqdevice_path(resource_bytes, resource_len);
	if (status != SQDC_STATUS_OK) {
		return runtime_device_config_error(out, "invalid resource path");
	}
	if (resource_len >= sizeof(resource)) {
		return runtime_device_config_error(out, "resource path too long");
	}
	memcpy(resource, resource_bytes, resource_len);
	resource[resource_len] = '\0';

	result = sq_app_store_resource_path(runtime->store_mount_point, runtime->current_app,
					    resource, path, sizeof(path));
	if (result != 0) {
		return runtime_device_config_error(out, "resource path failed");
	}
	result = runtime_device_config_read_file(path, runtime->transfer.completion.bytes,
						 sizeof(runtime->transfer.completion.bytes),
						 &bytes_len);
	if (result == -ENOSPC) {
		return runtime_device_config_error(out, "resource too large");
	}
	if (result != 0) {
		return runtime_device_config_error(out, "resource read failed");
	}

	status = sqdc_parse_sqdevice(runtime->transfer.completion.bytes, bytes_len,
				     &runtime->device_config_draft);
	if (status != SQDC_STATUS_OK) {
		const char *error = runtime_device_config_status_error(status);
		return runtime_device_config_error(out, error != NULL ? error : "parse error");
	}
	runtime->device_config_draft_loaded = true;
	return runtime_device_config_ok(out);
}

int sq_vm_runtime_device_config_load(struct sq_vm_runtime *runtime, const uint8_t *source,
				     size_t source_len, SqvmDeviceConfigResult *out)
{
	static const char package_prefix[] = "package:";
	const uint8_t *resource_bytes;
	size_t resource_len;

	if (runtime == NULL || source == NULL || out == NULL) {
		return -EINVAL;
	}
	if (source_len <= sizeof(package_prefix) - 1 ||
	    memcmp(source, package_prefix, sizeof(package_prefix) - 1) != 0) {
		return runtime_device_config_error(out, "unsupported source");
	}
	if (runtime->store_mount_point == NULL || runtime->current_app[0] == '\0') {
		return runtime_device_config_error(out, "no current app");
	}

	resource_bytes = source + sizeof(package_prefix) - 1;
	resource_len = source_len - (sizeof(package_prefix) - 1);
	return sq_vm_runtime_device_config_load_resource(runtime, resource_bytes, resource_len, out);
}

int sq_vm_runtime_device_config_set(struct sq_vm_runtime *runtime, const uint8_t *key,
				    size_t key_len, SqvmDeviceConfigValue value,
				    SqvmDeviceConfigResult *out)
{
	SqdcStatus status;

	if (runtime == NULL || key == NULL || out == NULL) {
		return -EINVAL;
	}
	if (!runtime->device_config_draft_loaded) {
		return runtime_device_config_error(out, "no draft");
	}

	switch (value.kind) {
	case SQVM_DEVICE_CONFIG_VALUE_NULL:
		status = sqdc_config_set_null(&runtime->device_config_draft, key, key_len);
		break;
	case SQVM_DEVICE_CONFIG_VALUE_BOOL:
		status = sqdc_config_set_bool(&runtime->device_config_draft, key, key_len,
					      value.bool_value);
		break;
	case SQVM_DEVICE_CONFIG_VALUE_I32:
		status = sqdc_config_set_i32(&runtime->device_config_draft, key, key_len,
					     value.i32_value);
		break;
	case SQVM_DEVICE_CONFIG_VALUE_STRING:
		status = sqdc_config_set_string(&runtime->device_config_draft, key, key_len,
						value.string, value.string_len);
		break;
	default:
		status = SQDC_STATUS_INVALID_ARGUMENT;
		break;
	}
	if (status != SQDC_STATUS_OK) {
		const char *error = runtime_device_config_status_error(status);
		return runtime_device_config_error(out, error != NULL ? error : "invalid argument");
	}
	return runtime_device_config_ok(out);
}

static const SqdcRecord *runtime_device_config_find(const SqdcConfig *config, const char *key)
{
	size_t key_len;

	if (config == NULL || key == NULL) {
		return NULL;
	}
	key_len = strlen(key);
	for (size_t i = 0; i < config->count; i++) {
		const SqdcRecord *record = &config->records[i];
		if (record->present && record->key_len == key_len &&
		    memcmp(record->key, key, key_len) == 0) {
			return record;
		}
	}
	return NULL;
}

static bool runtime_device_config_string_equals(const SqdcConfig *config, const char *key,
						const char *expected)
{
	const SqdcRecord *record = runtime_device_config_find(config, key);
	size_t expected_len = strlen(expected);

	return record != NULL && record->value.kind == SQDC_VALUE_STRING &&
	       record->value.string_len == expected_len &&
	       memcmp(record->value.string, expected, expected_len) == 0;
}

static int runtime_device_config_read_string(const SqdcConfig *config, const char *key,
					     const uint8_t **out, size_t *out_len)
{
	const SqdcRecord *record = runtime_device_config_find(config, key);

	if (record == NULL || out == NULL || out_len == NULL ||
	    record->value.kind != SQDC_VALUE_STRING) {
		return -EINVAL;
	}
	*out = record->value.string;
	*out_len = record->value.string_len;
	return 0;
}

static int runtime_device_config_read_bool(const SqdcConfig *config, const char *key, bool *out)
{
	const SqdcRecord *record = runtime_device_config_find(config, key);

	if (record == NULL || out == NULL || record->value.kind != SQDC_VALUE_BOOL) {
		return -EINVAL;
	}
	*out = record->value.bool_value;
	return 0;
}

int sq_vm_runtime_device_config_rebind(struct sq_vm_runtime *runtime, const uint8_t *alias,
				       size_t alias_len, SqvmDeviceConfigResult *out)
{
	const uint8_t *pin_name;
	size_t pin_name_len;
	uint8_t pin;
	bool active_low;

	if (runtime == NULL || alias == NULL || out == NULL) {
		return -EINVAL;
	}
	if (alias_len != strlen("indicator.default") ||
	    memcmp(alias, "indicator.default", alias_len) != 0) {
		return runtime_device_config_error(out, "unsupported binding");
	}
	if (!runtime->device_config_draft_loaded) {
		return runtime_device_config_error(out, "no draft");
	}
	if (!runtime_device_config_string_equals(&runtime->device_config_draft, "service",
						 "indicator.default") ||
	    !runtime_device_config_string_equals(&runtime->device_config_draft, "mode", "gpio")) {
		return runtime_device_config_error(out, "invalid binding");
	}
	if (runtime_device_config_read_string(&runtime->device_config_draft, "pinName", &pin_name,
					      &pin_name_len) != 0 ||
	    parse_gpio_name(pin_name, pin_name_len, &pin) != 0 ||
	    runtime_device_config_read_bool(&runtime->device_config_draft, "activeLow",
					    &active_low) != 0) {
		return runtime_device_config_error(out, "invalid binding");
	}

	runtime->indicator_breathe_active = false;
	runtime->indicator_blink_active = false;
	runtime->indicator_binding_active = true;
	runtime->indicator_binding_pin = pin;
	runtime->indicator_binding_active_low = active_low;
	return runtime_device_config_ok(out);
}

static int sq_vm_runtime_apply_target_default_indicator_binding(struct sq_vm_runtime *runtime)
{
#if SQ_VM_RUNTIME_HAS_INDICATOR_GPIO
	static const uint8_t service_key[] = "service";
	static const uint8_t mode_key[] = "mode";
	static const uint8_t pin_name_key[] = "pinName";
	static const uint8_t active_low_key[] = "activeLow";
	static const uint8_t service_value[] = "indicator.default";
	static const uint8_t mode_value[] = "gpio";
	char pin_name[sizeof("GPIO255")];
	SqvmDeviceConfigResult result = {0};
	SqdcStatus status;
	int written;

	if (runtime == NULL) {
		return -EINVAL;
	}

	written = snprintf(pin_name, sizeof(pin_name), "GPIO%u", indicator_gpio.pin);
	if (written <= 0 || (size_t)written >= sizeof(pin_name)) {
		return -EINVAL;
	}

	status = sqdc_config_clear(&runtime->device_config_draft);
	if (status == SQDC_STATUS_OK) {
		status = sqdc_config_set_string(&runtime->device_config_draft, service_key,
						strlen((const char *)service_key), service_value,
						strlen((const char *)service_value));
	}
	if (status == SQDC_STATUS_OK) {
		status = sqdc_config_set_string(&runtime->device_config_draft, mode_key,
						strlen((const char *)mode_key), mode_value,
						strlen((const char *)mode_value));
	}
	if (status == SQDC_STATUS_OK) {
		status = sqdc_config_set_string(&runtime->device_config_draft, pin_name_key,
						strlen((const char *)pin_name_key),
						(const uint8_t *)pin_name, strlen(pin_name));
	}
	if (status == SQDC_STATUS_OK) {
		status = sqdc_config_set_bool(
			&runtime->device_config_draft, active_low_key,
			strlen((const char *)active_low_key),
			(indicator_gpio.dt_flags & GPIO_ACTIVE_LOW) != 0);
	}
	if (status != SQDC_STATUS_OK) {
		return -EINVAL;
	}

	runtime->device_config_draft_loaded = true;
	if (sq_vm_runtime_device_config_rebind(runtime, (const uint8_t *)"indicator.default",
					       strlen("indicator.default"), &result) != 0 ||
	    !result.ok) {
		return -EINVAL;
	}
	return 0;
#else
	if (runtime == NULL) {
		return -EINVAL;
	}
	runtime->indicator_binding_active = false;
	runtime->indicator_binding_pin = 0;
	runtime->indicator_binding_active_low = false;
	runtime->device_config_draft_loaded = false;
	return 0;
#endif
}

static size_t runtime_fixed_text_len(const uint8_t *bytes, size_t cap)
{
	size_t len = 0;

	while (len < cap && bytes[len] != 0) {
		len++;
	}
	return len;
}

static bool runtime_fixed_text_equals(const uint8_t *bytes, size_t cap, const char *expected)
{
	size_t len;

	if (bytes == NULL || expected == NULL) {
		return false;
	}
	len = runtime_fixed_text_len(bytes, cap);
	return len == strlen(expected) && memcmp(bytes, expected, len) == 0;
}

static bool runtime_device_binding_resource_has_prefix(const uint8_t *resource,
						       size_t resource_len,
						       const char *prefix)
{
	size_t prefix_len;

	if (resource == NULL || prefix == NULL) {
		return false;
	}
	prefix_len = strlen(prefix);
	return resource_len > prefix_len && memcmp(resource, prefix, prefix_len) == 0;
}

static int sq_vm_runtime_apply_inline_gpio_indicator_binding(struct sq_vm_runtime *runtime,
							     const uint8_t *resource,
							     size_t resource_len,
							     SqvmDeviceConfigResult *out)
{
	static const char gpio_prefix[] = "gpio:";
	static const uint8_t service_key[] = "service";
	static const uint8_t mode_key[] = "mode";
	static const uint8_t pin_name_key[] = "pinName";
	static const uint8_t active_low_key[] = "activeLow";
	static const uint8_t service_value[] = "indicator.default";
	static const uint8_t mode_value[] = "gpio";
	const uint8_t *pin_name;
	size_t pin_name_len;
	uint8_t pin;
	SqdcStatus status;

	if (runtime == NULL || resource == NULL || out == NULL ||
	    !runtime_device_binding_resource_has_prefix(resource, resource_len, gpio_prefix)) {
		return -EINVAL;
	}

	pin_name = resource + strlen(gpio_prefix);
	pin_name_len = resource_len - strlen(gpio_prefix);
	if (parse_gpio_name(pin_name, pin_name_len, &pin) != 0) {
		return runtime_device_config_error(out, "invalid binding");
	}

	status = sqdc_config_clear(&runtime->device_config_draft);
	if (status == SQDC_STATUS_OK) {
		status = sqdc_config_set_string(&runtime->device_config_draft, service_key,
						strlen((const char *)service_key), service_value,
						strlen((const char *)service_value));
	}
	if (status == SQDC_STATUS_OK) {
		status = sqdc_config_set_string(&runtime->device_config_draft, mode_key,
						strlen((const char *)mode_key), mode_value,
						strlen((const char *)mode_value));
	}
	if (status == SQDC_STATUS_OK) {
		status = sqdc_config_set_string(&runtime->device_config_draft, pin_name_key,
						strlen((const char *)pin_name_key), pin_name,
						pin_name_len);
	}
	if (status == SQDC_STATUS_OK) {
		status = sqdc_config_set_bool(&runtime->device_config_draft, active_low_key,
					      strlen((const char *)active_low_key), false);
	}
	if (status != SQDC_STATUS_OK) {
		const char *error = runtime_device_config_status_error(status);
		return runtime_device_config_error(out, error != NULL ? error : "invalid binding");
	}

	runtime->device_config_draft_loaded = true;
	return sq_vm_runtime_device_config_rebind(runtime, (const uint8_t *)"indicator.default",
						  strlen("indicator.default"), out);
}

static int sq_vm_runtime_apply_saved_device_config(struct sq_vm_runtime *runtime)
{
	char path[SQ_APP_STORE_PATH_MAX];
	size_t bytes_len = 0;
	SqvmDeviceConfigResult result = {0};
	SqdcStatus status;
	int fs_result;

	if (runtime == NULL || runtime->store_mount_point == NULL) {
		return 0;
	}

	fs_result = sq_app_store_device_config_path(runtime->store_mount_point, path, sizeof(path));
	if (fs_result != 0) {
		return fs_result;
	}
	fs_result = runtime_device_config_read_file(path, runtime->transfer.completion.bytes,
						    sizeof(runtime->transfer.completion.bytes),
						    &bytes_len);
	if (fs_result == -ENOENT) {
		return 0;
	}
	if (fs_result != 0) {
		return fs_result;
	}

	status = sqdc_decode_sqdc(runtime->transfer.completion.bytes, bytes_len,
				  &runtime->device_config_draft);
	if (status != SQDC_STATUS_OK) {
		return -EINVAL;
	}
	runtime->device_config_draft_loaded = true;
	if (sq_vm_runtime_device_config_rebind(runtime, (const uint8_t *)"indicator.default",
					       strlen("indicator.default"), &result) != 0 ||
	    !result.ok) {
		return -EINVAL;
	}
	return 0;
}

static int sq_vm_runtime_apply_device_bindings(struct sq_vm_runtime *runtime)
{
	size_t count = 0;
	SqvmStatus status;

	if (runtime == NULL || runtime->backend == NULL || runtime->backend->read_sqbc == NULL ||
	    runtime->store_mount_point == NULL || runtime->current_app[0] == '\0') {
		return 0;
	}

	status = sqvm_device_binding_count_from_reader(runtime, runtime_read_exact_at,
						       runtime->transfer.init_scratch,
						       sizeof(runtime->transfer.init_scratch),
						       &count);
	if (status != SQVM_STATUS_OK) {
		return sq_vm_runtime_status_to_errno(status);
	}

	for (size_t index = 0; index < count; index++) {
		SqvmDeviceBinding binding = {0};
		SqvmDeviceConfigResult result = {0};
		size_t resource_len;

		status = sqvm_device_binding_read_from_reader(runtime, runtime_read_exact_at,
							      runtime->transfer.init_scratch,
							      sizeof(runtime->transfer.init_scratch),
							      index, &binding);
		if (status != SQVM_STATUS_OK) {
			return sq_vm_runtime_status_to_errno(status);
		}
		if (!runtime_fixed_text_equals(binding.service, sizeof(binding.service), "indicator") ||
		    !runtime_fixed_text_equals(binding.binding, sizeof(binding.binding), "default")) {
			return -ENOTSUP;
		}

		resource_len = runtime_fixed_text_len(binding.resource, sizeof(binding.resource));
		if (resource_len == 0 || resource_len >= sizeof(binding.resource)) {
			return -EINVAL;
		}
		if (runtime_device_binding_resource_has_prefix(binding.resource, resource_len,
							       "gpio:")) {
			if (sq_vm_runtime_apply_inline_gpio_indicator_binding(
				    runtime, binding.resource, resource_len, &result) != 0 ||
			    !result.ok) {
				return -EINVAL;
			}
		} else {
			if (sq_vm_runtime_device_config_load_resource(runtime, binding.resource,
								      resource_len, &result) != 0 ||
			    !result.ok) {
				return -EINVAL;
			}
			memset(&result, 0, sizeof(result));
			if (sq_vm_runtime_device_config_rebind(
				    runtime, (const uint8_t *)"indicator.default",
				    strlen("indicator.default"), &result) != 0 ||
			    !result.ok) {
				return -EINVAL;
			}
		}
	}
	return 0;
}

static int32_t runtime_device_config_load(void *user_data, const uint8_t *source,
					  size_t source_len, SqvmDeviceConfigResult *out)
{
	return sq_vm_runtime_device_config_load(user_data, source, source_len, out);
}

static int32_t runtime_device_config_set(void *user_data, const uint8_t *key,
					 size_t key_len, SqvmDeviceConfigValue value,
					 SqvmDeviceConfigResult *out)
{
	return sq_vm_runtime_device_config_set(user_data, key, key_len, value, out);
}

static int32_t runtime_device_config_rebind(void *user_data, const uint8_t *alias,
					    size_t alias_len, SqvmDeviceConfigResult *out)
{
	return sq_vm_runtime_device_config_rebind(user_data, alias, alias_len, out);
}

int sq_vm_runtime_device_config_save(struct sq_vm_runtime *runtime, const uint8_t *destination,
				     size_t destination_len, SqvmDeviceConfigResult *out)
{
	char path[SQ_APP_STORE_PATH_MAX];
	size_t encoded_len = 0;
	SqdcStatus status;
	int result;

	if (runtime == NULL || destination == NULL || out == NULL) {
		return -EINVAL;
	}
	if (destination_len != strlen("flash") || memcmp(destination, "flash", destination_len) != 0) {
		return runtime_device_config_unsupported(out);
	}
	if (runtime->store_mount_point == NULL) {
		return runtime_device_config_error(out, "no store");
	}
	if (!runtime->device_config_draft_loaded) {
		return runtime_device_config_error(out, "no draft");
	}

	result = sq_app_store_prepare_filesystem(runtime->store_mount_point);
	if (result != 0) {
		return runtime_device_config_error(out, "storage prepare failed");
	}
	result = sq_app_store_device_config_path(runtime->store_mount_point, path, sizeof(path));
	if (result != 0) {
		return runtime_device_config_error(out, "config path failed");
	}

	status = sqdc_encode_sqdc(&runtime->device_config_draft,
				  runtime->transfer.completion.bytes,
				  sizeof(runtime->transfer.completion.bytes), &encoded_len);
	if (status != SQDC_STATUS_OK) {
		const char *error = runtime_device_config_status_error(status);
		return runtime_device_config_error(out, error != NULL ? error : "encode error");
	}
	result = runtime_device_config_write_file(path, runtime->transfer.completion.bytes,
						  encoded_len);
	if (result != 0) {
		return runtime_device_config_error(out, "config write failed");
	}
	return runtime_device_config_ok(out);
}

static int32_t runtime_device_config_save(void *user_data, const uint8_t *destination,
					  size_t destination_len, SqvmDeviceConfigResult *out)
{
	return sq_vm_runtime_device_config_save(user_data, destination, destination_len, out);
}

static int32_t runtime_content_pick_file(void *user_data, const uint8_t *extension,
					 size_t extension_len, SqvmContentPickFileResult *out)
{
	ARG_UNUSED(user_data);
	ARG_UNUSED(extension);
	ARG_UNUSED(extension_len);

	if (out == NULL) {
		return -EINVAL;
	}
	out->ok = false;
	out->error = (const uint8_t *)"unsupported";
	out->error_len = strlen("unsupported");
	out->path = NULL;
	out->path_len = 0;
	return 0;
}

static int32_t runtime_content_read_text(void *user_data, const uint8_t *path, size_t path_len,
					 SqvmContentReadTextResult *out)
{
	ARG_UNUSED(user_data);
	ARG_UNUSED(path);
	ARG_UNUSED(path_len);

	if (out == NULL) {
		return -EINVAL;
	}
	out->ok = false;
	out->error = (const uint8_t *)"unsupported";
	out->error_len = strlen("unsupported");
	out->text = NULL;
	out->text_len = 0;
	return 0;
}

static int32_t runtime_content_read_lines(void *user_data, const uint8_t *path, size_t path_len,
					  int32_t max_lines, SqvmContentReadLinesResult *out)
{
	ARG_UNUSED(user_data);
	ARG_UNUSED(path);
	ARG_UNUSED(path_len);
	ARG_UNUSED(max_lines);

	if (out == NULL) {
		return -EINVAL;
	}
	out->ok = false;
	out->error = (const uint8_t *)"unsupported";
	out->error_len = strlen("unsupported");
	return 0;
}

static void clear_dispatch_transfer(struct sq_vm_runtime *runtime)
{
	memset(&runtime->transfer, 0, sizeof(runtime->transfer));
	memset(&runtime->result, 0, sizeof(runtime->result));
	runtime->backend = NULL;
}

static void runtime_work_handler(struct k_work *work)
{
	struct sq_vm_runtime *runtime = CONTAINER_OF(work, struct sq_vm_runtime, work);
	int result = sq_vm_runtime_dispatch(runtime, &runtime->job_backend, runtime->event);

	runtime->result_code = result;
	runtime->dispatch_exited = result == 0 && runtime->result.exited;
	runtime->status = result == 0 ? SQ_VM_RUNTIME_COMPLETE : SQ_VM_RUNTIME_ERROR;
}

void sq_vm_runtime_init(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL || runtime->work_initialized) {
		return;
	}
	if (!sq_vm_runtime_work_q_started) {
		k_work_queue_start(&sq_vm_runtime_work_q, sq_vm_runtime_work_stack,
				   K_THREAD_STACK_SIZEOF(sq_vm_runtime_work_stack), 5, NULL);
		sq_vm_runtime_work_q_started = true;
	}
	k_work_init(&runtime->work, runtime_work_handler);
	runtime->work_initialized = true;
	runtime->status = SQ_VM_RUNTIME_IDLE;
	(void)sq_vm_runtime_apply_target_default_indicator_binding(runtime);
}

size_t sq_vm_runtime_work_stack_size(void)
{
	return K_THREAD_STACK_SIZEOF(sq_vm_runtime_work_stack);
}

int sq_vm_runtime_work_stack_unused(size_t *unused)
{
	if (unused == NULL) {
		return -EINVAL;
	}

#if defined(CONFIG_INIT_STACKS) && defined(CONFIG_THREAD_STACK_INFO)
	if (!sq_vm_runtime_work_q_started) {
		*unused = sq_vm_runtime_work_stack_size();
		return 0;
	}

	return k_thread_stack_space_get(k_work_queue_thread_get(&sq_vm_runtime_work_q), unused);
#else
	*unused = 0;
	return -ENOTSUP;
#endif
}

void sq_vm_runtime_reset(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL) {
		return;
	}
	sq_vm_runtime_reset_vm_context(runtime);
	memset(&runtime->job_backend, 0, sizeof(runtime->job_backend));
	memset(runtime->event, 0, sizeof(runtime->event));
	memset(runtime->traces, 0, sizeof(runtime->traces));
	runtime->trace_count = 0;
	memset(runtime->current_app, 0, sizeof(runtime->current_app));
	memset(runtime->pending_launch_app, 0, sizeof(runtime->pending_launch_app));
	runtime->pending_launch_active = false;
	memset(runtime->pending_arm_app, 0, sizeof(runtime->pending_arm_app));
	runtime->pending_arm_active = false;
	runtime->arm_registration_active = false;
	memset(runtime->arm_registration_app, 0, sizeof(runtime->arm_registration_app));
	memset(runtime->lifecycle_target_app, 0, sizeof(runtime->lifecycle_target_app));
	runtime->lifecycle_launch_after_exit = false;
	memset(runtime->return_stack, 0, sizeof(runtime->return_stack));
	runtime->return_stack_count = 0;
	memset(runtime->armed_timers, 0, sizeof(runtime->armed_timers));
	runtime->armed_timer_count = 0;
	memset(runtime->outputs, 0, sizeof(runtime->outputs));
	runtime->output_count = 0;
	memset(runtime->drawlog, 0, sizeof(runtime->drawlog));
	runtime->drawlog_count = 0;
	memset(runtime->timers, 0, sizeof(runtime->timers));
	runtime->indicator_state = false;
	runtime->indicator_breathe_active = false;
	runtime->indicator_breathe_step = 0;
	runtime->indicator_breathe_next_ms = 0;
	runtime->indicator_blink_active = false;
	runtime->indicator_blink_on = false;
	runtime->indicator_blink_on_ms = 0;
	runtime->indicator_blink_off_ms = 0;
	runtime->indicator_blink_next_ms = 0;
	runtime->indicator_binding_active = false;
	runtime->indicator_binding_pin = 0;
	runtime->indicator_binding_active_low = false;
	memset(&runtime->device_config_draft, 0, sizeof(runtime->device_config_draft));
	runtime->device_config_draft_loaded = false;
	(void)sq_vm_runtime_apply_target_default_indicator_binding(runtime);
	runtime->gpio_configured_mask = 0;
	runtime->gpio_state_mask = 0;
	memset(runtime->wifi_profile, 0, sizeof(runtime->wifi_profile));
	runtime->wifi_profile_len = 0;
	memset(runtime->wifi_profile_ssid, 0, sizeof(runtime->wifi_profile_ssid));
	runtime->wifi_profile_ssid_len = 0;
	memset(runtime->wifi_profile_password, 0, sizeof(runtime->wifi_profile_password));
	runtime->wifi_profile_password_len = 0;
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	memset(runtime->wifi_station_ip, 0, sizeof(runtime->wifi_station_ip));
	runtime->wifi_station_connect_status = 0;
	runtime->wifi_station_disconnect_status = 0;
	runtime->wifi_ap_active = false;
	runtime->wifi_ap_start_events = 0;
	runtime->wifi_ap_stop_events = 0;
#endif
	runtime->dispatch_exited = false;
	runtime->result_code = 0;
	runtime->status = SQ_VM_RUNTIME_IDLE;
}

void sq_vm_runtime_reset_vm_context(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL) {
		return;
	}
	clear_dispatch_transfer(runtime);
	memset(runtime->context_words, 0, sizeof(runtime->context_words));
	runtime->context_ready = false;
}

void sq_vm_runtime_set_store_mount_point(struct sq_vm_runtime *runtime, const char *mount_point)
{
	if (runtime != NULL) {
		runtime->store_mount_point = mount_point;
	}
}

void sq_vm_runtime_set_registry(struct sq_vm_runtime *runtime, const struct sq_app_registry *registry)
{
	if (runtime != NULL) {
		runtime->registry = registry;
	}
}

const char *sq_vm_runtime_status_name(SqvmStatus status)
{
	switch (status) {
	case SQVM_STATUS_OK:
		return "ok";
	case SQVM_STATUS_INVALID_ARGUMENT:
		return "invalid_argument";
	case SQVM_STATUS_VM_ERROR:
		return "vm_error";
	default:
		return "unknown";
	}
}

int sq_vm_runtime_status_to_errno(SqvmStatus status)
{
	switch (status) {
	case SQVM_STATUS_OK:
		return 0;
	case SQVM_STATUS_INVALID_ARGUMENT:
		return -EINVAL;
	case SQVM_STATUS_VM_ERROR:
		return -EIO;
	default:
		return -EIO;
	}
}

int sq_vm_runtime_dispatch(struct sq_vm_runtime *runtime,
			   const struct sq_vm_storage_backend *backend, const char *event)
{
	SqvmCallbacks callbacks;
	SqvmStatus status;

	if (runtime == NULL || backend == NULL || event == NULL) {
		return -EINVAL;
	}
	if (sqvm_context_size() > sizeof(runtime->context_words)) {
		return -ENOMEM;
	}

	clear_dispatch_transfer(runtime);
	runtime->backend = backend;
	callbacks = (SqvmCallbacks){
		.user_data = runtime,
		.trace = runtime_trace,
		.read_exact_at = runtime_read_exact_at,
		.debug_output = runtime_debug_output,
		.display_clear = runtime_display_clear,
		.display_text = runtime_display_text,
		.display_rect = runtime_display_rect,
		.display_line = runtime_display_line,
		.display_select = runtime_display_select,
		.display_image = runtime_display_image,
		.display_draw = runtime_display_draw,
		.indicator_write = runtime_indicator_write,
		.indicator_toggle = runtime_indicator_toggle,
		.indicator_read = runtime_indicator_read,
		.indicator_breathe = runtime_indicator_breathe,
		.indicator_blink = runtime_indicator_blink,
		.hardware_gpio_write = runtime_hardware_gpio_write,
		.hardware_gpio_toggle = runtime_hardware_gpio_toggle,
		.hardware_gpio_read = runtime_hardware_gpio_read,
		.app_launch = runtime_app_launch,
		.app_arm = runtime_app_arm,
		.app_disarm = runtime_app_disarm,
		.app_registry_list = runtime_app_registry_list,
		.app_registry_get = runtime_app_registry_get,
		.app_process_stack = runtime_app_process_stack,
		.app_armed_stack = runtime_app_armed_stack,
		.timer_every = runtime_timer_every,
		.timer_after = runtime_timer_after,
		.wifi_start_ap = runtime_wifi_start_ap,
		.wifi_stop_ap = runtime_wifi_stop_ap,
		.wifi_connect = runtime_wifi_connect,
		.wifi_disconnect = runtime_wifi_disconnect,
		.wifi_get_ap_ip = runtime_wifi_get_ap_ip,
		.wifi_status = runtime_wifi_status,
		.wifi_scan = runtime_wifi_scan,
		.device_config_load = runtime_device_config_load,
		.device_config_set = runtime_device_config_set,
		.device_config_rebind = runtime_device_config_rebind,
		.device_config_save = runtime_device_config_save,
		.content_pick_file = runtime_content_pick_file,
		.content_read_text = runtime_content_read_text,
		.content_read_lines = runtime_content_read_lines,
		.system_memory_text = runtime_system_memory_text,
		.system_storage_text = runtime_system_storage_text,
	};

	if (!runtime->context_ready) {
		status = sqvm_context_prepare(runtime->context_words, sizeof(runtime->context_words));
		if (status != SQVM_STATUS_OK) {
			return sq_vm_runtime_status_to_errno(status);
		}
		status = sqvm_context_init_in_place(runtime->context_words, callbacks,
						    runtime->transfer.init_scratch,
						    sizeof(runtime->transfer.init_scratch));
		if (status != SQVM_STATUS_OK) {
			return sq_vm_runtime_status_to_errno(status);
		}
		runtime->context_ready = true;
	}
	status = sqvm_dispatch_start_resumable(runtime->context_words, callbacks,
					       (const uint8_t *)event, strlen(event),
					       &runtime->result);
	if (status != SQVM_STATUS_OK) {
		return sq_vm_runtime_status_to_errno(status);
	}

	while (runtime->result.outcome == SQVM_DISPATCH_PENDING_STORAGE) {
		int storage_result = sq_vm_storage_complete_request(backend, &runtime->result.storage,
								   &runtime->transfer.completion);
		if (storage_result != 0) {
			return storage_result;
		}
		status = sqvm_dispatch_resume_storage(runtime->context_words, callbacks,
						      &runtime->transfer.completion,
						      &runtime->result);
		if (status != SQVM_STATUS_OK) {
			return sq_vm_runtime_status_to_errno(status);
		}
	}

	runtime->dispatch_exited = runtime->result.outcome == SQVM_DISPATCH_COMPLETE &&
				   runtime->result.exited;
	return runtime->result.outcome == SQVM_DISPATCH_COMPLETE ? 0 : -EIO;
}

int sq_vm_runtime_start(struct sq_vm_runtime *runtime,
			const struct sq_vm_storage_backend *backend, const char *event)
{
	size_t event_len;
	int result;

	if (runtime == NULL || backend == NULL || event == NULL) {
		return -EINVAL;
	}
	sq_vm_runtime_init(runtime);
	if (runtime->status == SQ_VM_RUNTIME_RUNNING) {
		return -EBUSY;
	}
	event_len = strlen(event);
	if (event_len == 0 || event_len >= sizeof(runtime->event)) {
		return -EINVAL;
	}

	runtime->job_backend = *backend;
	runtime->backend = &runtime->job_backend;
	if (strcmp(event, "app.start") == 0) {
		result = sq_vm_runtime_apply_saved_device_config(runtime);
		if (result != 0) {
			return result;
		}
		result = sq_vm_runtime_apply_device_bindings(runtime);
		if (result != 0) {
			return result;
		}
	}
	memcpy(runtime->event, event, event_len + 1);
	runtime->result_code = 0;
	runtime->dispatch_exited = false;
	runtime->status = SQ_VM_RUNTIME_RUNNING;
	k_work_submit_to_queue(&sq_vm_runtime_work_q, &runtime->work);
	return 0;
}

int sq_vm_runtime_record_output(struct sq_vm_runtime *runtime, const uint8_t *message,
				size_t message_len)
{
	if (runtime == NULL || (message == NULL && message_len > 0)) {
		return -EINVAL;
	}
	size_t slot = runtime->output_count;
	if (slot >= SQ_VM_RUNTIME_OUTPUT_MAX) {
		memmove(runtime->outputs[0], runtime->outputs[1],
			(SQ_VM_RUNTIME_OUTPUT_MAX - 1) * SQ_VM_RUNTIME_OUTPUT_LEN);
		slot = SQ_VM_RUNTIME_OUTPUT_MAX - 1;
		runtime->output_count = SQ_VM_RUNTIME_OUTPUT_MAX - 1;
	}
	size_t len = message_len;
	if (len >= SQ_VM_RUNTIME_OUTPUT_LEN) {
		len = SQ_VM_RUNTIME_OUTPUT_LEN - 1;
	}
	memcpy(runtime->outputs[slot], message, len);
	runtime->outputs[slot][len] = '\0';
	runtime->output_count++;
	return 0;
}

int sq_vm_runtime_record_drawlog(struct sq_vm_runtime *runtime, const char *line)
{
	if (runtime == NULL || line == NULL) {
		return -EINVAL;
	}
	size_t slot = runtime->drawlog_count;
	if (slot >= SQ_VM_RUNTIME_DRAWLOG_MAX) {
		memmove(runtime->drawlog[0], runtime->drawlog[1],
			(SQ_VM_RUNTIME_DRAWLOG_MAX - 1) * SQ_VM_RUNTIME_DRAWLOG_LEN);
		slot = SQ_VM_RUNTIME_DRAWLOG_MAX - 1;
		runtime->drawlog_count = SQ_VM_RUNTIME_DRAWLOG_MAX - 1;
	}
	size_t len = 0;
	while (len < SQ_VM_RUNTIME_DRAWLOG_LEN - 1 && line[len] != '\0') {
		len++;
	}
	memcpy(runtime->drawlog[slot], line, len);
	runtime->drawlog[slot][len] = '\0';
	runtime->drawlog_count++;
	return 0;
}

static int configure_indicator_gpio(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL || runtime->indicator_gpio_configured) {
		return 0;
	}
	runtime->indicator_gpio_configured = true;
#if SQ_VM_RUNTIME_HAS_INDICATOR_GPIO
	if (!gpio_is_ready_dt(&indicator_gpio)) {
		return 0;
	}
	if (gpio_pin_configure_dt(&indicator_gpio, GPIO_OUTPUT_INACTIVE) != 0) {
		return 0;
	}
	runtime->indicator_gpio_available = true;
#endif
	return 0;
}

static bool indicator_is_active_low(void)
{
#if SQ_VM_RUNTIME_HAS_INDICATOR_GPIO
	return (indicator_gpio.dt_flags & GPIO_ACTIVE_LOW) != 0;
#else
	return false;
#endif
}

static bool runtime_indicator_active_low(const struct sq_vm_runtime *runtime)
{
	if (runtime != NULL && runtime->indicator_binding_active) {
		return runtime->indicator_binding_active_low;
	}
	return indicator_is_active_low();
}

static bool indicator_uses_dt_gpio_pin(uint8_t pin)
{
#if SQ_VM_RUNTIME_HAS_INDICATOR_GPIO
	return indicator_gpio.pin == pin;
#else
	ARG_UNUSED(pin);
	return false;
#endif
}

static uint8_t runtime_indicator_pin(const struct sq_vm_runtime *runtime)
{
	if (runtime != NULL && runtime->indicator_binding_active) {
		return runtime->indicator_binding_pin;
	}
#if SQ_VM_RUNTIME_HAS_INDICATOR_GPIO
	return indicator_gpio.pin;
#else
	return 0;
#endif
}

static bool indicator_uses_raw_gpio(uint8_t pin)
{
	return indicator_uses_dt_gpio_pin(pin);
}

static int set_indicator_raw_output(struct sq_vm_runtime *runtime, bool raw_high)
{
#if SQ_VM_RUNTIME_HAS_INDICATOR_PWM
	if (pwm_is_ready_dt(&indicator_pwm)) {
		uint32_t pulse = raw_high ? indicator_pwm.period : 0U;
		return pwm_set_dt(&indicator_pwm, indicator_pwm.period, pulse);
	}
#endif
	(void)configure_indicator_gpio(runtime);
#if SQ_VM_RUNTIME_HAS_INDICATOR_GPIO
	if (runtime->indicator_gpio_available) {
		int result = gpio_pin_set_raw(indicator_gpio.port, indicator_gpio.pin, raw_high ? 1 : 0);
		if (result != 0) {
			return result;
		}
	}
#endif
	return 0;
}

static int set_indicator_brightness(struct sq_vm_runtime *runtime, uint8_t brightness)
{
	uint8_t clamped = brightness > 100U ? 100U : brightness;
	uint8_t pin = runtime_indicator_pin(runtime);
	bool active_low = runtime_indicator_active_low(runtime);
	ARG_UNUSED(active_low);
#if SQ_VM_RUNTIME_HAS_INDICATOR_PWM
	uint8_t raw_high_percent = active_low ? (uint8_t)(100U - clamped) : clamped;
#endif

	runtime->indicator_state = clamped > 0U;
#if SQ_VM_RUNTIME_HAS_INDICATOR_PWM
	if (indicator_uses_dt_gpio_pin(pin) && pwm_is_ready_dt(&indicator_pwm)) {
		uint32_t pulse = (indicator_pwm.period * (uint32_t)raw_high_percent) / 100U;
		return pwm_set_dt(&indicator_pwm, indicator_pwm.period, pulse);
	}
#endif
	if (!indicator_uses_dt_gpio_pin(pin)) {
		int result = configure_raw_gpio(runtime, pin);
		if (result != 0) {
			return result;
		}
#if SQ_VM_RUNTIME_HAS_GPIO0
		bool raw_high = active_low ? clamped == 0U : clamped > 0U;
		if (device_is_ready(gpio0_dev)) {
			return gpio_pin_set_raw(gpio0_dev, pin, raw_high ? 1 : 0);
		}
#endif
		return 0;
	}
	(void)configure_indicator_gpio(runtime);
#if SQ_VM_RUNTIME_HAS_INDICATOR_GPIO
	if (runtime->indicator_gpio_available) {
		int result = gpio_pin_set_dt(&indicator_gpio, clamped > 0U ? 1 : 0);
		if (result != 0) {
			return result;
		}
	}
#endif
	return 0;
}

int sq_vm_runtime_indicator_write(struct sq_vm_runtime *runtime, bool value)
{
	if (runtime == NULL) {
		return -EINVAL;
	}
	runtime->indicator_breathe_active = false;
	runtime->indicator_blink_active = false;
	return set_indicator_brightness(runtime, value ? 100U : 0U);
}

int sq_vm_runtime_indicator_toggle(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL) {
		return -EINVAL;
	}
	return sq_vm_runtime_indicator_write(runtime, !runtime->indicator_state);
}

int sq_vm_runtime_indicator_read(struct sq_vm_runtime *runtime, bool *out)
{
	if (runtime == NULL || out == NULL) {
		return -EINVAL;
	}
	*out = runtime->indicator_state;
	return 0;
}

int sq_vm_runtime_indicator_breathe(struct sq_vm_runtime *runtime)
{
	int64_t now;

	if (runtime == NULL) {
		return -EINVAL;
	}
	now = k_uptime_get();
	runtime->indicator_breathe_active = true;
	runtime->indicator_blink_active = false;
	runtime->indicator_breathe_step = 0;
	runtime->indicator_breathe_next_ms = now;
	return set_indicator_brightness(runtime, 0U);
}

int sq_vm_runtime_indicator_blink(struct sq_vm_runtime *runtime, int32_t on_ms, int32_t off_ms)
{
	int64_t now;

	if (runtime == NULL || on_ms <= 0 || off_ms <= 0) {
		return -EINVAL;
	}
	now = k_uptime_get();
	runtime->indicator_breathe_active = false;
	runtime->indicator_blink_active = true;
	runtime->indicator_blink_on = true;
	runtime->indicator_blink_on_ms = on_ms;
	runtime->indicator_blink_off_ms = off_ms;
	runtime->indicator_blink_next_ms = now + on_ms;
	return set_indicator_brightness(runtime, 100U);
}

static int parse_gpio_name(const uint8_t *name, size_t name_len, uint8_t *pin)
{
	uint32_t value = 0;

	if (name == NULL || pin == NULL || name_len < 5 || name_len > 6 ||
	    memcmp(name, "GPIO", 4) != 0) {
		return -EINVAL;
	}
	for (size_t i = 4; i < name_len; i++) {
		if (name[i] < '0' || name[i] > '9') {
			return -EINVAL;
		}
		value = (value * 10U) + (uint32_t)(name[i] - '0');
	}
	if (value > 25U) {
		return -EINVAL;
	}
	*pin = (uint8_t)value;
	return 0;
}

static int configure_raw_gpio(struct sq_vm_runtime *runtime, uint8_t pin)
{
	uint32_t bit = BIT(pin);

	if ((runtime->gpio_configured_mask & bit) != 0) {
		return 0;
	}
#if SQ_VM_RUNTIME_HAS_GPIO0
	if (device_is_ready(gpio0_dev)) {
		int result = gpio_pin_configure(gpio0_dev, pin, GPIO_OUTPUT);
		if (result != 0) {
			return result;
		}
	}
#endif
	runtime->gpio_configured_mask |= bit;
	return 0;
}

int sq_vm_runtime_hardware_gpio_write(struct sq_vm_runtime *runtime, const uint8_t *name,
				      size_t name_len, bool value)
{
	uint8_t pin;
	uint32_t bit;
	int result;

	if (runtime == NULL || parse_gpio_name(name, name_len, &pin) != 0) {
		return -EINVAL;
	}
	if (indicator_uses_raw_gpio(pin)) {
		runtime->indicator_breathe_active = false;
		runtime->indicator_blink_active = false;
		runtime->indicator_state = indicator_is_active_low() ? !value : value;
		bit = BIT(pin);
		runtime->gpio_configured_mask |= bit;
		if (value) {
			runtime->gpio_state_mask |= bit;
		} else {
			runtime->gpio_state_mask &= ~bit;
		}
		return set_indicator_raw_output(runtime, value);
	}
	result = configure_raw_gpio(runtime, pin);
	if (result != 0) {
		return result;
	}
	bit = BIT(pin);
	if (value) {
		runtime->gpio_state_mask |= bit;
	} else {
		runtime->gpio_state_mask &= ~bit;
	}
#if SQ_VM_RUNTIME_HAS_GPIO0
	if (device_is_ready(gpio0_dev)) {
		return gpio_pin_set_raw(gpio0_dev, pin, value ? 1 : 0);
	}
#endif
	return 0;
}

int sq_vm_runtime_hardware_gpio_toggle(struct sq_vm_runtime *runtime, const uint8_t *name,
				       size_t name_len)
{
	bool value;
	int result = sq_vm_runtime_hardware_gpio_read(runtime, name, name_len, &value);

	if (result != 0) {
		return result;
	}
	return sq_vm_runtime_hardware_gpio_write(runtime, name, name_len, !value);
}

int sq_vm_runtime_hardware_gpio_read(struct sq_vm_runtime *runtime, const uint8_t *name,
				     size_t name_len, bool *out)
{
	uint8_t pin;
	uint32_t bit;

	if (runtime == NULL || out == NULL || parse_gpio_name(name, name_len, &pin) != 0) {
		return -EINVAL;
	}
	bit = BIT(pin);
	if ((runtime->gpio_configured_mask & bit) != 0) {
		*out = (runtime->gpio_state_mask & bit) != 0;
		return 0;
	}
#if SQ_VM_RUNTIME_HAS_GPIO0
	if (device_is_ready(gpio0_dev)) {
		int value = gpio_pin_get_raw(gpio0_dev, pin);
		if (value < 0) {
			return value;
		}
		*out = value != 0;
		return 0;
	}
#endif
	*out = (runtime->gpio_state_mask & bit) != 0;
	return 0;
}

static int sq_vm_runtime_poll_indicator_breathe(struct sq_vm_runtime *runtime)
{
	int64_t now;
	uint8_t brightness;

	if (!runtime->indicator_breathe_active) {
		return 0;
	}
	now = k_uptime_get();
	if (now < runtime->indicator_breathe_next_ms) {
		return 0;
	}

	brightness = indicator_breathe_duties[runtime->indicator_breathe_step];
	runtime->indicator_breathe_step =
		(uint8_t)((runtime->indicator_breathe_step + 1U) %
			  SQ_VM_RUNTIME_INDICATOR_BREATHE_STEPS);
	runtime->indicator_breathe_next_ms = now + SQ_VM_RUNTIME_BREATHE_LEVEL_MS;
	return set_indicator_brightness(runtime, brightness);
}

static int sq_vm_runtime_poll_indicator_blink(struct sq_vm_runtime *runtime)
{
	int64_t now;

	if (!runtime->indicator_blink_active) {
		return 0;
	}
	now = k_uptime_get();
	if (now < runtime->indicator_blink_next_ms) {
		return 0;
	}

	runtime->indicator_blink_on = !runtime->indicator_blink_on;
	runtime->indicator_blink_next_ms =
		now + (runtime->indicator_blink_on ? runtime->indicator_blink_on_ms :
						   runtime->indicator_blink_off_ms);
	return set_indicator_brightness(runtime, runtime->indicator_blink_on ? 100U : 0U);
}

int sq_vm_runtime_register_timer(struct sq_vm_runtime *runtime, const uint8_t *event,
				 size_t event_len, int32_t interval_ms, bool repeating)
{
	if (runtime == NULL || event == NULL || event_len == 0 ||
	    event_len >= SQ_VM_RUNTIME_EVENT_LEN || interval_ms <= 0) {
		return -EINVAL;
	}
	if (runtime->arm_registration_active) {
		return sq_vm_runtime_register_armed_timer(runtime, runtime->arm_registration_app,
							  event, event_len, interval_ms,
							  repeating);
	}
	for (size_t i = 0; i < SQ_VM_RUNTIME_TIMER_MAX; i++) {
		if (runtime->timers[i].active &&
		    strncmp(runtime->timers[i].event, (const char *)event, event_len) == 0 &&
		    runtime->timers[i].event[event_len] == '\0') {
			runtime->timers[i].repeating = repeating;
			runtime->timers[i].interval_ms = interval_ms;
			runtime->timers[i].due_ms = k_uptime_get() + interval_ms;
			return 0;
		}
	}
	for (size_t i = 0; i < SQ_VM_RUNTIME_TIMER_MAX; i++) {
		if (!runtime->timers[i].active) {
			runtime->timers[i].active = true;
			runtime->timers[i].repeating = repeating;
			runtime->timers[i].interval_ms = interval_ms;
			runtime->timers[i].due_ms = k_uptime_get() + interval_ms;
			memcpy(runtime->timers[i].event, event, event_len);
			runtime->timers[i].event[event_len] = '\0';
			return 0;
		}
	}
	return -ENOSPC;
}

int sq_vm_runtime_register_armed_timer(struct sq_vm_runtime *runtime, const char *app,
				       const uint8_t *event, size_t event_len,
				       int32_t interval_ms, bool repeating)
{
	if (runtime == NULL || app == NULL || app[0] == '\0' ||
	    strlen(app) >= SQ_APP_STORE_APP_ID_MAX || event == NULL || event_len == 0 ||
	    event_len >= SQ_VM_RUNTIME_EVENT_LEN || interval_ms <= 0) {
		return -EINVAL;
	}
	for (size_t i = 0; i < SQ_VM_RUNTIME_ARMED_TIMER_MAX; i++) {
		struct sq_vm_runtime_armed_timer *timer = &runtime->armed_timers[i];
		if (timer->active && strcmp(timer->app_id, app) == 0 &&
		    strncmp(timer->event, (const char *)event, event_len) == 0 &&
		    timer->event[event_len] == '\0') {
			timer->repeating = repeating;
			timer->interval_ms = interval_ms;
			timer->due_ms = k_uptime_get() + interval_ms;
			return 0;
		}
	}
	for (size_t i = 0; i < SQ_VM_RUNTIME_ARMED_TIMER_MAX; i++) {
		struct sq_vm_runtime_armed_timer *timer = &runtime->armed_timers[i];
		if (!timer->active) {
			timer->active = true;
			timer->repeating = repeating;
			timer->interval_ms = interval_ms;
			timer->due_ms = k_uptime_get() + interval_ms;
			strncpy(timer->app_id, app, sizeof(timer->app_id) - 1);
			timer->app_id[sizeof(timer->app_id) - 1] = '\0';
			memcpy(timer->event, event, event_len);
			timer->event[event_len] = '\0';
			runtime->armed_timer_count++;
			return 0;
		}
	}
	return -ENOSPC;
}

int sq_vm_runtime_clear_armed_app(struct sq_vm_runtime *runtime, const uint8_t *app,
				  size_t app_len)
{
	if (runtime == NULL || app == NULL || app_len == 0 ||
	    app_len >= SQ_APP_STORE_APP_ID_MAX) {
		return -EINVAL;
	}
	for (size_t i = 0; i < SQ_VM_RUNTIME_ARMED_TIMER_MAX; i++) {
		struct sq_vm_runtime_armed_timer *timer = &runtime->armed_timers[i];
		if (timer->active && strlen(timer->app_id) == app_len &&
		    memcmp(timer->app_id, app, app_len) == 0) {
			memset(timer, 0, sizeof(*timer));
		}
	}
	runtime->armed_timer_count = 0;
	for (size_t i = 0; i < SQ_VM_RUNTIME_ARMED_TIMER_MAX; i++) {
		if (runtime->armed_timers[i].active) {
			runtime->armed_timer_count++;
		}
	}
	return 0;
}

int sq_vm_runtime_next_due_armed_timer(struct sq_vm_runtime *runtime, char *app, size_t app_cap,
				       char *event, size_t event_cap)
{
	if (runtime == NULL || app == NULL || app_cap == 0 || event == NULL || event_cap == 0) {
		return -EINVAL;
	}
	int64_t now = k_uptime_get();
	for (size_t i = 0; i < SQ_VM_RUNTIME_ARMED_TIMER_MAX; i++) {
		struct sq_vm_runtime_armed_timer *timer = &runtime->armed_timers[i];
		if (!timer->active || timer->due_ms > now) {
			continue;
		}
		size_t app_len = strlen(timer->app_id);
		size_t event_len = strlen(timer->event);
		if (app_len == 0 || app_len >= app_cap || event_len == 0 || event_len >= event_cap) {
			return -ENOSPC;
		}
		memcpy(app, timer->app_id, app_len + 1);
		memcpy(event, timer->event, event_len + 1);
		if (timer->repeating) {
			timer->due_ms = now + timer->interval_ms;
		} else {
			memset(timer, 0, sizeof(*timer));
			runtime->armed_timer_count--;
		}
		return 0;
	}
	return -ENOENT;
}

int sq_vm_runtime_next_due_timer(struct sq_vm_runtime *runtime, char *event, size_t event_cap)
{
	if (runtime == NULL || event == NULL || event_cap == 0) {
		return -EINVAL;
	}
	int64_t now = k_uptime_get();
	for (size_t i = 0; i < SQ_VM_RUNTIME_TIMER_MAX; i++) {
		struct sq_vm_runtime_timer *timer = &runtime->timers[i];
		if (!timer->active || timer->due_ms > now) {
			continue;
		}
		size_t event_len = strlen(timer->event);
		if (event_len == 0 || event_len >= event_cap) {
			return -ENOSPC;
		}
		memcpy(event, timer->event, event_len + 1);
		if (timer->repeating) {
			timer->due_ms = now + timer->interval_ms;
		} else {
			memset(timer, 0, sizeof(*timer));
		}
		return 0;
	}
	return -ENOENT;
}

int sq_vm_runtime_poll(struct sq_vm_runtime *runtime)
{
	char event[SQ_VM_RUNTIME_EVENT_LEN];

	if (runtime == NULL) {
		return 0;
	}
	(void)sq_vm_runtime_poll_indicator_blink(runtime);
	(void)sq_vm_runtime_poll_indicator_breathe(runtime);
	if (runtime->status == SQ_VM_RUNTIME_RUNNING || runtime->job_backend.read_sqbc == NULL) {
		return 0;
	}
	if (sq_vm_runtime_next_due_timer(runtime, event, sizeof(event)) != 0) {
		return 0;
	}
	return sq_vm_runtime_start(runtime, &runtime->job_backend, event);
}
