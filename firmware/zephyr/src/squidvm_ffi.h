#ifndef SQUIDSCRIPT_SQUIDVM_FFI_H
#define SQUIDSCRIPT_SQUIDVM_FFI_H

#include <stddef.h>
#include <stdbool.h>
#include <stdint.h>

#define SQVM_STORAGE_TRANSFER_CAPACITY 1024
#define SQVM_SAVED_STATE_CAPACITY 512

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
	SQVM_STATUS_OK = 0,
	SQVM_STATUS_INVALID_ARGUMENT = 1,
	SQVM_STATUS_VM_ERROR = 2,
} SqvmStatus;

typedef enum {
	SQDP_STATUS_OK = 0,
	SQDP_STATUS_INVALID_ARGUMENT = 1,
	SQDP_STATUS_BUFFER_TOO_SMALL = 2,
	SQDP_STATUS_ENCODE_ERROR = 3,
} SqdpStatus;

typedef enum {
	SQDP_ACTION_NONE = 0,
	SQDP_ACTION_BEGIN_INSTALL = 1,
	SQDP_ACTION_WRITE_INSTALL_CHUNK = 2,
	SQDP_ACTION_COMMIT_INSTALL = 3,
	SQDP_ACTION_BEGIN_TEMP_RUN = 4,
	SQDP_ACTION_WRITE_TEMP_RUN_CHUNK = 5,
	SQDP_ACTION_COMMIT_TEMP_RUN = 6,
	SQDP_ACTION_BEGIN_RESOURCE_INSTALL = 7,
	SQDP_ACTION_WRITE_RESOURCE_CHUNK = 8,
	SQDP_ACTION_COMMIT_RESOURCE_INSTALL = 9,
} SqdpActionKind;

typedef struct {
	uint8_t app_id[48];
	size_t sqbc_len;
} SqdpAppListEntry;

typedef struct {
	const uint8_t *bytes;
	size_t len;
} SqdpLineSlice;

typedef struct {
	const uint8_t *key;
	size_t key_len;
	uint64_t value;
} SqdpResourceMetric;

typedef struct {
	uint8_t app_id[48];
	uint8_t event[32];
} SqdpLifecycleTimer;

typedef struct {
	SqdpActionKind kind;
	const uint8_t *app_id;
	size_t app_id_len;
	const uint8_t *resource_path;
	size_t resource_path_len;
	const uint8_t *staging_path;
	size_t staging_path_len;
	size_t offset;
	const uint8_t *bytes;
	size_t bytes_len;
	size_t total_len;
} SqdpAction;

typedef struct {
	const uint8_t *profile;
	size_t profile_len;
	const uint8_t *ssid;
	size_t ssid_len;
	const uint8_t *password;
	size_t password_len;
} SqdpWifiProfile;

typedef struct {
	const uint8_t *bytes;
	size_t bytes_len;
} SqdpStateImport;

typedef struct {
	const uint8_t *app_id;
	size_t app_id_len;
} SqdpAppLaunch;

typedef struct {
	uint8_t event[32];
	int32_t interval_ms;
	bool repeating;
} SqvmTriggerTimer;

typedef struct {
	const uint8_t *app_id;
	size_t app_id_len;
	const uint8_t *event;
	size_t event_len;
} SqdpEventDispatch;

typedef enum {
	SQVM_DISPATCH_COMPLETE = 0,
	SQVM_DISPATCH_PENDING_STORAGE = 1,
} SqvmDispatchOutcome;

typedef enum {
	SQVM_STORAGE_REQUEST_NONE = 0,
	SQVM_STORAGE_REQUEST_SQBC_READ = 1,
	SQVM_STORAGE_REQUEST_STATE_LOAD = 2,
	SQVM_STORAGE_REQUEST_STATE_SAVE = 3,
	SQVM_STORAGE_REQUEST_STATE_RESET = 4,
} SqvmStorageRequestKind;

typedef struct {
	SqvmStorageRequestKind kind;
	size_t offset;
	size_t len;
	uint8_t bytes[SQVM_STORAGE_TRANSFER_CAPACITY];
} SqvmStorageRequest;

typedef struct {
	bool has_len;
	size_t len;
	uint8_t bytes[SQVM_STORAGE_TRANSFER_CAPACITY];
} SqvmStorageCompletion;

typedef struct {
	SqvmStatus status;
	SqvmDispatchOutcome outcome;
	bool exited;
	SqvmStorageRequest storage;
} SqvmDispatchResult;

typedef void (*SqvmTraceCallback)(void *user_data, const uint8_t *message, size_t message_len);
typedef int32_t (*SqvmReadExactAtCallback)(
	void *user_data,
	size_t offset,
	uint8_t *out,
	size_t out_len);
typedef void (*SqvmDebugOutputCallback)(void *user_data, const uint8_t *message, size_t message_len);
typedef struct {
	int32_t x;
	int32_t y;
	int32_t w;
	int32_t h;
	int32_t font_height;
	const uint8_t *text_color;
	size_t text_color_len;
	const uint8_t *background_color;
	size_t background_color_len;
	const uint8_t *align;
	size_t align_len;
	const uint8_t *valign;
	size_t valign_len;
} SqvmDisplayTextOptions;
typedef struct {
	int32_t x;
	int32_t y;
	int32_t w;
	int32_t h;
	const uint8_t *fill_color;
	size_t fill_color_len;
	const uint8_t *stroke_color;
	size_t stroke_color_len;
} SqvmDisplayRectOptions;
typedef struct {
	int32_t x1;
	int32_t y1;
	int32_t x2;
	int32_t y2;
	const uint8_t *color;
	size_t color_len;
} SqvmDisplayLineOptions;
typedef struct {
	int32_t x;
	int32_t y;
	int32_t w;
	int32_t h;
} SqvmDisplayResourceOptions;
typedef struct {
	const uint8_t *id;
	size_t id_len;
	const uint8_t *name;
	size_t name_len;
	const uint8_t *build;
	size_t build_len;
	const uint8_t *description;
	size_t description_len;
} SqvmAppRegistryEntry;
typedef struct {
	const uint8_t *app_id;
	size_t app_id_len;
	const uint8_t *event;
	size_t event_len;
} SqvmAppStackEntry;
typedef void (*SqvmDisplayClearCallback)(void *user_data, const uint8_t *color, size_t color_len);
typedef void (*SqvmDisplayTextCallback)(
	void *user_data,
	const uint8_t *text,
	size_t text_len,
	const SqvmDisplayTextOptions *options);
typedef void (*SqvmDisplayRectCallback)(void *user_data, const SqvmDisplayRectOptions *options);
typedef void (*SqvmDisplayLineCallback)(void *user_data, const SqvmDisplayLineOptions *options);
typedef int32_t (*SqvmDisplaySelectCallback)(void *user_data, const uint8_t *name,
					     size_t name_len);
typedef void (*SqvmDisplayImageCallback)(void *user_data, const uint8_t *path,
					 size_t path_len,
					 const SqvmDisplayResourceOptions *options);
typedef void (*SqvmDisplayDrawCallback)(void *user_data, const uint8_t *drawable,
					size_t drawable_len,
					const SqvmDisplayResourceOptions *options);

#define SQVM_WIFI_SCAN_MAX_NETWORKS 4

typedef struct {
	bool active;
	const uint8_t *mode;
	size_t mode_len;
	const uint8_t *ip_address;
	size_t ip_address_len;
	const uint8_t *ssid;
	size_t ssid_len;
	int32_t clients;
	const uint8_t *error;
	size_t error_len;
	const uint8_t *state;
	size_t state_len;
	const uint8_t *backend;
	size_t backend_len;
	bool driver_started;
	bool configured;
	const uint8_t *driver_mode;
	size_t driver_mode_len;
	int32_t channel;
	int32_t ap_start_events;
	int32_t ap_stop_events;
	int32_t probe_events;
	int32_t sta_connected_events;
	int32_t sta_disconnected_events;
	const uint8_t *last_backend_code;
	size_t last_backend_code_len;
	const uint8_t *profile;
	size_t profile_len;
	bool connected;
	int32_t scan_matches;
	int32_t rssi;
	const uint8_t *auth;
	size_t auth_len;
	const uint8_t *bssid;
	size_t bssid_len;
	const uint8_t *disconnect_reason;
	size_t disconnect_reason_len;
	int32_t disconnect_reason_code;
} SqvmWifiStatus;

typedef struct {
	const uint8_t *ssid;
	size_t ssid_len;
	const uint8_t *bssid;
	size_t bssid_len;
	int32_t ssid_length;
	int32_t channel;
	int32_t rssi;
	const uint8_t *auth;
	size_t auth_len;
	bool hidden;
} SqvmWifiAccessPoint;

typedef struct {
	bool ok;
	const uint8_t *error;
	size_t error_len;
	const SqvmWifiAccessPoint *networks;
	size_t network_count;
} SqvmWifiScanResult;

typedef struct {
	bool ok;
	const uint8_t *error;
	size_t error_len;
} SqvmWifiActionResult;

typedef enum {
	SQVM_DEVICE_CONFIG_VALUE_NULL = 0,
	SQVM_DEVICE_CONFIG_VALUE_BOOL = 1,
	SQVM_DEVICE_CONFIG_VALUE_I32 = 2,
	SQVM_DEVICE_CONFIG_VALUE_STRING = 3,
} SqvmDeviceConfigValueKind;

typedef struct {
	SqvmDeviceConfigValueKind kind;
	bool bool_value;
	int32_t i32_value;
	const uint8_t *string;
	size_t string_len;
} SqvmDeviceConfigValue;

typedef struct {
	bool ok;
	const uint8_t *error;
	size_t error_len;
	const uint8_t *warning;
	size_t warning_len;
} SqvmDeviceConfigResult;

typedef struct {
	const uint8_t *ip;
	size_t ip_len;
	const uint8_t *gw;
	size_t gw_len;
	const uint8_t *netmask;
	size_t netmask_len;
	const uint8_t *error;
	size_t error_len;
} SqvmWifiApIp;

typedef int32_t (*SqvmWifiStartApCallback)(void *user_data, const uint8_t *ssid,
					   size_t ssid_len, SqvmWifiActionResult *out);
typedef int32_t (*SqvmWifiStopApCallback)(void *user_data, SqvmWifiActionResult *out);
typedef int32_t (*SqvmWifiConnectCallback)(void *user_data, const uint8_t *profile,
					   size_t profile_len, SqvmWifiActionResult *out);
typedef int32_t (*SqvmWifiDisconnectCallback)(void *user_data, SqvmWifiActionResult *out);
typedef int32_t (*SqvmWifiGetApIpCallback)(void *user_data, SqvmWifiApIp *out);
typedef int32_t (*SqvmWifiStatusCallback)(void *user_data, SqvmWifiStatus *out);
typedef int32_t (*SqvmWifiScanCallback)(void *user_data, SqvmWifiScanResult *out);
typedef int32_t (*SqvmDeviceConfigLoadCallback)(void *user_data, const uint8_t *source,
						size_t source_len, SqvmDeviceConfigResult *out);
typedef int32_t (*SqvmDeviceConfigSetCallback)(void *user_data, const uint8_t *key,
					       size_t key_len, SqvmDeviceConfigValue value,
					       SqvmDeviceConfigResult *out);
typedef int32_t (*SqvmDeviceConfigRebindCallback)(void *user_data, const uint8_t *alias,
						  size_t alias_len, SqvmDeviceConfigResult *out);
typedef int32_t (*SqvmDeviceConfigSaveCallback)(void *user_data, const uint8_t *destination,
						size_t destination_len,
						SqvmDeviceConfigResult *out);

typedef int32_t (*SqvmIndicatorWriteCallback)(void *user_data, bool value);
typedef int32_t (*SqvmIndicatorToggleCallback)(void *user_data);
typedef int32_t (*SqvmIndicatorReadCallback)(void *user_data, bool *out);
typedef int32_t (*SqvmIndicatorBreatheCallback)(void *user_data);
typedef int32_t (*SqvmHardwareGpioWriteCallback)(
	void *user_data,
	const uint8_t *name,
	size_t name_len,
	bool value);
typedef int32_t (*SqvmHardwareGpioToggleCallback)(
	void *user_data,
	const uint8_t *name,
	size_t name_len);
typedef int32_t (*SqvmHardwareGpioReadCallback)(
	void *user_data,
	const uint8_t *name,
	size_t name_len,
	bool *out);
typedef int32_t (*SqvmAppLifecycleCallback)(void *user_data, const uint8_t *app, size_t app_len);
typedef int32_t (*SqvmAppRegistryListCallback)(void *user_data, SqvmAppRegistryEntry *out,
					       size_t out_cap, size_t *out_count);
typedef int32_t (*SqvmAppRegistryGetCallback)(void *user_data, const uint8_t *app,
					      size_t app_len, SqvmAppRegistryEntry *out);
typedef int32_t (*SqvmAppStackCallback)(void *user_data, SqvmAppStackEntry *out,
					size_t out_cap, size_t *out_count);
typedef int32_t (*SqvmTimerEveryCallback)(
	void *user_data,
	const uint8_t *event,
	size_t event_len,
	int32_t interval_ms);
typedef int32_t (*SqvmTimerAfterCallback)(
	void *user_data,
	const uint8_t *event,
	size_t event_len,
	int32_t delay_ms);
typedef int32_t (*SqvmSystemMemoryTextCallback)(
	void *user_data,
	uint8_t *out,
	size_t out_cap,
	size_t *out_len);
typedef int32_t (*SqvmSystemStorageTextCallback)(
	void *user_data,
	const uint8_t *name,
	size_t name_len,
	uint8_t *out,
	size_t out_cap,
	size_t *out_len);

typedef struct {
	void *user_data;
	SqvmTraceCallback trace;
	SqvmReadExactAtCallback read_exact_at;
	SqvmDebugOutputCallback debug_output;
	SqvmDisplayClearCallback display_clear;
	SqvmDisplayTextCallback display_text;
	SqvmDisplayRectCallback display_rect;
	SqvmDisplayLineCallback display_line;
	SqvmDisplaySelectCallback display_select;
	SqvmDisplayImageCallback display_image;
	SqvmDisplayDrawCallback display_draw;
	SqvmIndicatorWriteCallback indicator_write;
	SqvmIndicatorToggleCallback indicator_toggle;
	SqvmIndicatorReadCallback indicator_read;
	SqvmIndicatorBreatheCallback indicator_breathe;
	SqvmHardwareGpioWriteCallback hardware_gpio_write;
	SqvmHardwareGpioToggleCallback hardware_gpio_toggle;
	SqvmHardwareGpioReadCallback hardware_gpio_read;
	SqvmAppLifecycleCallback app_launch;
	SqvmAppLifecycleCallback app_arm;
	SqvmAppLifecycleCallback app_disarm;
	SqvmAppRegistryListCallback app_registry_list;
	SqvmAppRegistryGetCallback app_registry_get;
	SqvmAppStackCallback app_process_stack;
	SqvmAppStackCallback app_armed_stack;
	SqvmTimerEveryCallback timer_every;
	SqvmTimerAfterCallback timer_after;
	SqvmWifiStartApCallback wifi_start_ap;
	SqvmWifiStopApCallback wifi_stop_ap;
	SqvmWifiConnectCallback wifi_connect;
	SqvmWifiDisconnectCallback wifi_disconnect;
	SqvmWifiGetApIpCallback wifi_get_ap_ip;
	SqvmWifiStatusCallback wifi_status;
	SqvmWifiScanCallback wifi_scan;
	SqvmDeviceConfigLoadCallback device_config_load;
	SqvmDeviceConfigSetCallback device_config_set;
	SqvmDeviceConfigRebindCallback device_config_rebind;
	SqvmDeviceConfigSaveCallback device_config_save;
	SqvmSystemMemoryTextCallback system_memory_text;
	SqvmSystemStorageTextCallback system_storage_text;
} SqvmCallbacks;

size_t sqvm_context_size(void);
size_t sqvm_context_align(void);
size_t sqvm_storage_transfer_capacity(void);
size_t sqvm_saved_state_capacity(void);
SqvmStatus sqvm_context_prepare(void *context, size_t context_len);
SqvmStatus sqvm_context_init_in_place(
	void *context,
	SqvmCallbacks callbacks,
	uint8_t *scratch,
	size_t scratch_len);
SqvmStatus sqvm_trigger_timer_count(
	const uint8_t *sqbc,
	size_t sqbc_len,
	size_t *out_count);
SqvmStatus sqvm_trigger_timer_read(
	const uint8_t *sqbc,
	size_t sqbc_len,
	size_t index,
	SqvmTriggerTimer *out_timer);
SqvmStatus sqvm_trigger_timer_count_from_reader(
	void *user_data,
	SqvmReadExactAtCallback read_exact_at,
	uint8_t *scratch,
	size_t scratch_len,
	size_t *out_count);
SqvmStatus sqvm_trigger_timer_read_from_reader(
	void *user_data,
	SqvmReadExactAtCallback read_exact_at,
	uint8_t *scratch,
	size_t scratch_len,
	size_t index,
	SqvmTriggerTimer *out_timer);
SqvmStatus sqvm_dispatch(
	void *context,
	SqvmCallbacks callbacks,
	const uint8_t *event,
	size_t event_len);
SqvmStatus sqvm_dispatch_start_resumable(
	void *context,
	SqvmCallbacks callbacks,
	const uint8_t *event,
	size_t event_len,
	SqvmDispatchResult *out_result);
SqvmStatus sqvm_dispatch_resume_storage(
	void *context,
	SqvmCallbacks callbacks,
	const SqvmStorageCompletion *completion,
	SqvmDispatchResult *out_result);
SqdpStatus sqdp_encode_empty_response(
	uint8_t opcode,
	uint8_t status,
	uint32_t sequence,
	uint8_t *out,
	size_t out_cap,
	size_t *out_len);
SqdpStatus sqdp_encode_hello_response(
	uint8_t opcode,
	uint32_t sequence,
	const uint8_t *target,
	size_t target_len,
	const uint8_t *firmware,
	size_t firmware_len,
	bool diagnostic,
	uint8_t *out,
	size_t out_cap,
	size_t *out_len);
SqdpStatus sqdp_encode_error_response(
	uint8_t opcode,
	uint32_t sequence,
	int64_t code,
	const uint8_t *message,
	size_t message_len,
	uint8_t *out,
	size_t out_cap,
	size_t *out_len);
SqdpStatus sqdp_encode_error_response_for_code(
	uint8_t opcode,
	uint32_t sequence,
	int64_t code,
	uint8_t *out,
	size_t out_cap,
	size_t *out_len);
SqdpStatus sqdp_encode_app_list_response(
	uint32_t sequence,
	const SqdpAppListEntry *entries,
	size_t entry_count,
	uint8_t *out,
	size_t out_cap,
	size_t *out_len);
SqdpStatus sqdp_encode_line_response(
	uint8_t opcode,
	uint32_t sequence,
	const uint8_t *fixed_lines,
	size_t fixed_count,
	size_t fixed_stride,
	const SqdpLineSlice *extra_lines,
	size_t extra_count,
	uint8_t *out,
	size_t out_cap,
	size_t *out_len);
SqdpStatus sqdp_encode_lifecycle_response(
	uint32_t sequence,
	const uint8_t *active_app,
	size_t active_app_len,
	const uint8_t *process_stack,
	size_t process_count,
	size_t process_stride,
	const SqdpLifecycleTimer *armed_timers,
	size_t armed_count,
	uint8_t *out,
	size_t out_cap,
	size_t *out_len);
SqdpStatus sqdp_encode_resources_response(
	uint32_t sequence,
	const SqdpResourceMetric *metrics,
	size_t metric_count,
	uint8_t *out,
	size_t out_cap,
	size_t *out_len);
SqdpStatus sqdp_encode_state_response(
	uint32_t sequence,
	const uint8_t *bytes,
	size_t bytes_len,
	uint8_t *out,
	size_t out_cap,
	size_t *out_len);
SqdpStatus sqdp_prepare_key_event(
	const uint8_t *request,
	size_t request_len,
	uint8_t *out,
	size_t out_cap,
	size_t *out_len);
SqdpStatus sqdp_parse_wifi_profile_set_request(
	const uint8_t *request,
	size_t request_len,
	SqdpWifiProfile *out_profile);
SqdpStatus sqdp_parse_state_import_request(
	const uint8_t *request,
	size_t request_len,
	SqdpStateImport *out_import);
SqdpStatus sqdp_parse_app_launch_request(
	const uint8_t *request,
	size_t request_len,
	SqdpAppLaunch *out_launch);
SqdpStatus sqdp_parse_event_dispatch_request(
	const uint8_t *request,
	size_t request_len,
	SqdpEventDispatch *out_event);
SqdpStatus sqdp_prepare_transfer_begin(
	const uint8_t *request,
	size_t request_len,
	void *session,
	SqdpAction *out_action);
SqdpStatus sqdp_prepare_transfer_chunk(
	const uint8_t *request,
	size_t request_len,
	const void *session,
	SqdpAction *out_action);
SqdpStatus sqdp_complete_transfer_chunk(
	void *session,
	const uint8_t *bytes,
	size_t bytes_len);
SqdpStatus sqdp_prepare_transfer_commit(
	const uint8_t *request,
	size_t request_len,
	const void *session,
	SqdpAction *out_action);
void sqdp_clear_transfer_session(void *session);
SqdpStatus sqdp_prepare_resource_begin(
	const uint8_t *request,
	size_t request_len,
	void *session,
	SqdpAction *out_action);
SqdpStatus sqdp_prepare_resource_chunk(
	const uint8_t *request,
	size_t request_len,
	const void *session,
	SqdpAction *out_action);
SqdpStatus sqdp_complete_resource_chunk(
	void *session,
	const uint8_t *bytes,
	size_t bytes_len);
SqdpStatus sqdp_prepare_resource_commit(
	const uint8_t *request,
	size_t request_len,
	const void *session,
	SqdpAction *out_action);
void sqdp_clear_resource_session(void *session);

#ifdef __cplusplus
}
#endif

#endif
