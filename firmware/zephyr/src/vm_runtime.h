#ifndef SQUIDSCRIPT_VM_RUNTIME_H
#define SQUIDSCRIPT_VM_RUNTIME_H

#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>
#include <zephyr/kernel.h>
#include <zephyr/drivers/gpio.h>
#if IS_ENABLED(CONFIG_NET_L2_WIFI_MGMT) && IS_ENABLED(CONFIG_NET_MGMT_EVENT) && \
	IS_ENABLED(CONFIG_NET_MGMT_EVENT_INFO)
#include <zephyr/net/net_mgmt.h>
#endif

#include "app_store.h"
#include "vm_storage.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SQ_VM_RUNTIME_TRACE_MAX 4
#define SQ_VM_RUNTIME_TRACE_LEN 26
#define SQ_VM_RUNTIME_OUTPUT_MAX 12
#define SQ_VM_RUNTIME_OUTPUT_LEN 54
#define SQ_VM_RUNTIME_DRAWLOG_MAX 4
#define SQ_VM_RUNTIME_DRAWLOG_LEN 48
#define SQ_VM_RUNTIME_TIMER_MAX 2
#define SQ_VM_RUNTIME_ACTIVE_BINDING_MAX 3
#define SQ_VM_RUNTIME_INPUT_BUTTON_MAX 2
#define SQ_VM_RUNTIME_INPUT_POLL_MS 20
#define SQ_VM_RUNTIME_INPUT_DEBOUNCE_MS 30
#if defined(CONFIG_BOARD_NATIVE_SIM)
#define SQ_VM_RUNTIME_CONTEXT_BYTES 65536
#else
#define SQ_VM_RUNTIME_CONTEXT_BYTES 10880
#endif
#define SQ_VM_RUNTIME_SCRATCH_BYTES SQVM_STORAGE_TRANSFER_CAPACITY
#define SQ_VM_RUNTIME_WORK_STACK_SIZE 22016
#define SQ_VM_RUNTIME_EVENT_LEN 24
#define SQ_VM_RUNTIME_INDICATOR_BREATHE_STEPS 65
#define SQ_VM_RUNTIME_RETURN_STACK_MAX 2
#define SQ_VM_RUNTIME_ARMED_TIMER_MAX 2
#define SQ_VM_RUNTIME_WIFI_SSID_LEN 33
#define SQ_VM_RUNTIME_WIFI_BSSID_LEN 18
#define SQ_VM_RUNTIME_WIFI_AUTH_LEN 24
#define SQ_VM_RUNTIME_WIFI_IPV4_LEN 16
#define SQ_VM_RUNTIME_WIFI_PROFILE_NAME_BYTES 16
#define SQ_VM_RUNTIME_WIFI_PROFILE_SSID_BYTES 32
#define SQ_VM_RUNTIME_WIFI_PROFILE_PASSWORD_BYTES 64

enum sq_vm_runtime_status {
	SQ_VM_RUNTIME_IDLE = 0,
	SQ_VM_RUNTIME_RUNNING = 1,
	SQ_VM_RUNTIME_COMPLETE = 2,
	SQ_VM_RUNTIME_ERROR = 3,
};

enum sq_vm_runtime_transfer_owner {
	SQ_VM_RUNTIME_TRANSFER_FREE = 0,
	SQ_VM_RUNTIME_TRANSFER_SCRATCH = 1,
	SQ_VM_RUNTIME_TRANSFER_COMPLETION = 2,
	SQ_VM_RUNTIME_TRANSFER_WIFI_SCAN = 3,
};

enum sq_vm_runtime_lifecycle_phase {
	SQ_VM_RUNTIME_LIFECYCLE_IDLE = 0,
	SQ_VM_RUNTIME_LIFECYCLE_LAUNCH_REQUESTED = 1,
	SQ_VM_RUNTIME_LIFECYCLE_EXIT_FOR_LAUNCH = 2,
	SQ_VM_RUNTIME_LIFECYCLE_RETURN_REQUESTED = 3,
	SQ_VM_RUNTIME_LIFECYCLE_SLEEP_REQUESTED = 4,
	SQ_VM_RUNTIME_LIFECYCLE_SLEEP_CHECKPOINT = 5,
};

enum sq_vm_runtime_arm_phase {
	SQ_VM_RUNTIME_ARM_IDLE = 0,
	SQ_VM_RUNTIME_ARM_REQUESTED = 1,
};

struct sq_vm_runtime_timer {
	bool active;
	bool repeating;
	int32_t interval_ms;
	int64_t due_ms;
	char event[SQ_VM_RUNTIME_EVENT_LEN];
};

struct sq_vm_runtime_armed_timer {
	bool active;
	bool repeating;
	int32_t interval_ms;
	int64_t due_ms;
	char app_id[SQ_APP_STORE_APP_ID_MAX];
	char event[SQ_VM_RUNTIME_EVENT_LEN];
};

struct sq_vm_runtime_active_binding {
	bool active;
	char alias[SQVM_DEVICE_BINDING_NAME_CAP];
};

struct sq_vm_runtime_input_button {
	bool active;
	uint8_t pin;
	bool active_low;
	bool pressed;
	int64_t next_poll_ms;
	int64_t debounce_until_ms;
	char event[SQ_VM_RUNTIME_EVENT_LEN];
};

struct sq_vm_runtime_wifi_scan_scratch {
	SqvmWifiAccessPoint networks[SQVM_WIFI_SCAN_MAX_NETWORKS];
	char ssids[SQVM_WIFI_SCAN_MAX_NETWORKS][SQ_VM_RUNTIME_WIFI_SSID_LEN];
	char bssids[SQVM_WIFI_SCAN_MAX_NETWORKS][SQ_VM_RUNTIME_WIFI_BSSID_LEN];
	char auth[SQVM_WIFI_SCAN_MAX_NETWORKS][SQ_VM_RUNTIME_WIFI_AUTH_LEN];
};

union sq_vm_runtime_transfer {
	uint8_t init_scratch[SQ_VM_RUNTIME_SCRATCH_BYTES];
	SqvmStorageCompletion completion;
	struct sq_vm_runtime_wifi_scan_scratch wifi_scan;
};

struct sq_vm_runtime {
	uint64_t context_words[SQ_VM_RUNTIME_CONTEXT_BYTES / sizeof(uint64_t)];
	bool work_initialized;
	bool work_submitted;
	bool context_ready;
	union sq_vm_runtime_transfer transfer;
#if IS_ENABLED(CONFIG_SQUIDSCRIPT_ZEPHYR_DIAGNOSTIC)
	enum sq_vm_runtime_transfer_owner transfer_owner;
#endif
	SqvmDispatchResult result;
	const struct sq_vm_storage_backend *backend;
	const char *store_mount_point;
	const struct sq_app_registry *registry;
	struct sq_vm_storage_backend job_backend;
	bool start_apply_bindings;
	char event[SQ_VM_RUNTIME_EVENT_LEN];
	enum sq_vm_runtime_status status;
	int result_code;
	uint64_t dispatch_sequence;
	uint64_t last_dispatch_sequence;
	uint64_t last_dispatch_elapsed_us;
	uint32_t last_dispatch_sqbc_read_count;
	uint32_t last_dispatch_sqbc_read_bytes;
	uint32_t dispatch_sqbc_read_count;
	uint32_t dispatch_sqbc_read_bytes;
	bool dispatch_exited;
	char current_app[SQ_APP_STORE_APP_ID_MAX];
	enum sq_vm_runtime_lifecycle_phase lifecycle_phase;
	char lifecycle_target_app[SQ_APP_STORE_APP_ID_MAX];
	char lifecycle_previous_app[SQ_APP_STORE_APP_ID_MAX];
	enum sq_vm_runtime_arm_phase arm_phase;
	char arm_target_app[SQ_APP_STORE_APP_ID_MAX];
	bool planned_sleep_ready;
	int32_t planned_sleep_wake_after_ms;
	char start_reason[16];
	char return_stack[SQ_VM_RUNTIME_RETURN_STACK_MAX][SQ_APP_STORE_APP_ID_MAX];
	uint8_t return_stack_count;
	struct sq_vm_runtime_armed_timer armed_timers[SQ_VM_RUNTIME_ARMED_TIMER_MAX];
	uint8_t armed_timer_count;
	struct sq_vm_runtime_active_binding active_bindings[SQ_VM_RUNTIME_ACTIVE_BINDING_MAX];
	uint8_t active_binding_count;
	struct sq_vm_runtime_input_button input_buttons[SQ_VM_RUNTIME_INPUT_BUTTON_MAX];
	uint8_t input_button_count;
	char traces[SQ_VM_RUNTIME_TRACE_MAX][SQ_VM_RUNTIME_TRACE_LEN];
	uint8_t trace_count;
	char outputs[SQ_VM_RUNTIME_OUTPUT_MAX][SQ_VM_RUNTIME_OUTPUT_LEN];
	uint8_t output_count;
	char drawlog[SQ_VM_RUNTIME_DRAWLOG_MAX][SQ_VM_RUNTIME_DRAWLOG_LEN];
	uint8_t drawlog_count;
	bool indicator_state;
	bool indicator_gpio_configured;
	bool indicator_gpio_available;
	bool indicator_binding_active;
	uint8_t indicator_binding_pin;
	bool indicator_binding_active_low;
	bool indicator_breathe_active;
	uint8_t indicator_breathe_step;
	int64_t indicator_breathe_next_ms;
	bool indicator_blink_active;
	bool indicator_blink_on;
	int32_t indicator_blink_on_ms;
	int32_t indicator_blink_off_ms;
	int64_t indicator_blink_next_ms;
	SqdcConfig device_config_draft;
	bool device_config_draft_loaded;
	uint32_t gpio_configured_mask;
	uint32_t gpio_state_mask;
	struct sq_vm_runtime_timer timers[SQ_VM_RUNTIME_TIMER_MAX];
	char wifi_profile[SQ_VM_RUNTIME_WIFI_PROFILE_NAME_BYTES];
	size_t wifi_profile_len;
	uint8_t wifi_profile_ssid[SQ_VM_RUNTIME_WIFI_PROFILE_SSID_BYTES];
	size_t wifi_profile_ssid_len;
	uint8_t wifi_profile_password[SQ_VM_RUNTIME_WIFI_PROFILE_PASSWORD_BYTES];
	size_t wifi_profile_password_len;
#if IS_ENABLED(CONFIG_NET_L2_WIFI_MGMT) && IS_ENABLED(CONFIG_NET_MGMT_EVENT) && \
	IS_ENABLED(CONFIG_NET_MGMT_EVENT_INFO)
	char wifi_station_ip[SQ_VM_RUNTIME_WIFI_IPV4_LEN];
	size_t wifi_scan_count;
	int wifi_scan_status;
	int wifi_station_connect_status;
	int wifi_station_disconnect_status;
	bool wifi_ap_active;
	int32_t wifi_ap_start_events;
	int32_t wifi_ap_stop_events;
	struct k_sem wifi_scan_done;
	struct k_sem wifi_station_connect_done;
	struct k_sem wifi_station_disconnect_done;
	bool wifi_scan_sem_initialized;
	bool wifi_station_sem_initialized;
	struct net_mgmt_event_callback wifi_mgmt_cb;
	bool wifi_mgmt_cb_registered;
#endif
};

static inline int sq_vm_runtime_transfer_acquire(struct sq_vm_runtime *runtime,
						 enum sq_vm_runtime_transfer_owner owner)
{
	if (runtime == NULL || owner == SQ_VM_RUNTIME_TRANSFER_FREE) {
		return -EINVAL;
	}
#if IS_ENABLED(CONFIG_SQUIDSCRIPT_ZEPHYR_DIAGNOSTIC)
	if (runtime->transfer_owner != SQ_VM_RUNTIME_TRANSFER_FREE) {
		return -EBUSY;
	}
	runtime->transfer_owner = owner;
#else
	ARG_UNUSED(owner);
#endif
	return 0;
}

static inline int sq_vm_runtime_transfer_release(struct sq_vm_runtime *runtime,
						 enum sq_vm_runtime_transfer_owner owner)
{
	if (runtime == NULL || owner == SQ_VM_RUNTIME_TRANSFER_FREE) {
		return -EINVAL;
	}
#if IS_ENABLED(CONFIG_SQUIDSCRIPT_ZEPHYR_DIAGNOSTIC)
	if (runtime->transfer_owner != owner) {
		return -EBUSY;
	}
	runtime->transfer_owner = SQ_VM_RUNTIME_TRANSFER_FREE;
#else
	ARG_UNUSED(owner);
#endif
	return 0;
}

static inline bool sq_vm_runtime_lifecycle_busy(const struct sq_vm_runtime *runtime)
{
	return runtime != NULL &&
	       runtime->lifecycle_phase != SQ_VM_RUNTIME_LIFECYCLE_IDLE;
}

static inline bool sq_vm_runtime_arm_busy(const struct sq_vm_runtime *runtime)
{
	return runtime != NULL && runtime->arm_phase != SQ_VM_RUNTIME_ARM_IDLE;
}

void sq_vm_runtime_init(struct sq_vm_runtime *runtime);

void sq_vm_runtime_reset(struct sq_vm_runtime *runtime);
void sq_vm_runtime_reset_vm_context(struct sq_vm_runtime *runtime);
int sq_vm_runtime_wait_idle(struct sq_vm_runtime *runtime, int32_t timeout_ms);
void sq_vm_runtime_set_store_mount_point(struct sq_vm_runtime *runtime, const char *mount_point);
void sq_vm_runtime_set_registry(struct sq_vm_runtime *runtime,
				const struct sq_app_registry *registry);
const char *sq_vm_runtime_status_name(SqvmStatus status);
int sq_vm_runtime_status_to_errno(SqvmStatus status);

int sq_vm_runtime_dispatch(struct sq_vm_runtime *runtime,
			   const struct sq_vm_storage_backend *backend, const char *event);

int sq_vm_runtime_start(struct sq_vm_runtime *runtime,
			const struct sq_vm_storage_backend *backend, const char *event);
int sq_vm_runtime_start_event(struct sq_vm_runtime *runtime,
			      const struct sq_vm_storage_backend *backend,
			      const uint8_t *event, size_t event_len);

int sq_vm_runtime_record_output(struct sq_vm_runtime *runtime, const uint8_t *message,
				size_t message_len);
int sq_vm_runtime_record_trace(struct sq_vm_runtime *runtime, const uint8_t *message,
			       size_t message_len);
int sq_vm_runtime_record_drawlog(struct sq_vm_runtime *runtime, const char *line);
int sq_vm_runtime_indicator_write(struct sq_vm_runtime *runtime, bool value);
int sq_vm_runtime_indicator_toggle(struct sq_vm_runtime *runtime);
int sq_vm_runtime_indicator_read(struct sq_vm_runtime *runtime, bool *out);
int sq_vm_runtime_indicator_breathe(struct sq_vm_runtime *runtime);
int sq_vm_runtime_indicator_blink(struct sq_vm_runtime *runtime, int32_t on_ms, int32_t off_ms);
int sq_vm_runtime_device_config_load(struct sq_vm_runtime *runtime, const uint8_t *source,
				     size_t source_len, SqvmDeviceConfigResult *out);
int sq_vm_runtime_device_config_set(struct sq_vm_runtime *runtime, const uint8_t *key,
				    size_t key_len, SqvmDeviceConfigValue value,
				    SqvmDeviceConfigResult *out);
int sq_vm_runtime_device_config_rebind(struct sq_vm_runtime *runtime, const uint8_t *alias,
				       size_t alias_len, SqvmDeviceConfigResult *out);
int sq_vm_runtime_device_config_save(struct sq_vm_runtime *runtime, const uint8_t *destination,
				     size_t destination_len, SqvmDeviceConfigResult *out);
int sq_vm_runtime_hardware_gpio_write(struct sq_vm_runtime *runtime, const uint8_t *name,
				      size_t name_len, bool value);
int sq_vm_runtime_hardware_gpio_toggle(struct sq_vm_runtime *runtime, const uint8_t *name,
				       size_t name_len);
int sq_vm_runtime_hardware_gpio_read(struct sq_vm_runtime *runtime, const uint8_t *name,
				     size_t name_len, bool *out);
int sq_vm_runtime_register_timer(struct sq_vm_runtime *runtime, const uint8_t *event,
				 size_t event_len, int32_t interval_ms, bool repeating);
int sq_vm_runtime_clear_armed_app(struct sq_vm_runtime *runtime, const uint8_t *app,
				  size_t app_len);
int sq_vm_runtime_register_armed_timer(struct sq_vm_runtime *runtime, const char *app,
				       const uint8_t *event, size_t event_len,
				       int32_t interval_ms, bool repeating);
int sq_vm_runtime_next_due_armed_timer(struct sq_vm_runtime *runtime, char *app, size_t app_cap,
				       char *event, size_t event_cap);
int sq_vm_runtime_next_due_timer(struct sq_vm_runtime *runtime, char *event, size_t event_cap);
int sq_vm_runtime_poll(struct sq_vm_runtime *runtime);
size_t sq_vm_runtime_work_stack_size(void);
int sq_vm_runtime_work_stack_unused(size_t *unused);
int sq_vm_runtime_wifi_format_bssid(const uint8_t *mac, size_t mac_len, char *out, size_t out_len);
int sq_vm_runtime_set_wifi_profile(struct sq_vm_runtime *runtime, const uint8_t *profile,
				   size_t profile_len, const uint8_t *ssid, size_t ssid_len,
				   const uint8_t *password, size_t password_len);

#ifdef __cplusplus
}
#endif

#endif
