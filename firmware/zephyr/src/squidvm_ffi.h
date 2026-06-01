#ifndef SQUIDSCRIPT_SQUIDVM_FFI_H
#define SQUIDSCRIPT_SQUIDVM_FFI_H

#include <stddef.h>
#include <stdbool.h>
#include <stdint.h>

#define SQVM_STORAGE_TRANSFER_CAPACITY 640
#define SQVM_SAVED_STATE_CAPACITY 512
#define SQVM_DEVICE_BINDING_NAME_CAP 32
#define SQVM_DEVICE_BINDING_RESOURCE_CAP 128

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

#define SQDC_CONFIG_MAX_RECORDS 5
#define SQDC_CONFIG_KEY_CAP 32
#define SQDC_CONFIG_STRING_CAP 48

typedef enum {
	SQDC_STATUS_OK = 0,
	SQDC_STATUS_INVALID_ARGUMENT = 1,
	SQDC_STATUS_BUFFER_TOO_SMALL = 2,
	SQDC_STATUS_PARSE_ERROR = 3,
	SQDC_STATUS_TOO_MANY_RECORDS = 4,
} SqdcStatus;

typedef enum {
	SQDC_VALUE_NULL = 0,
	SQDC_VALUE_BOOL = 1,
	SQDC_VALUE_I32 = 2,
	SQDC_VALUE_STRING = 3,
} SqdcValueKind;

typedef struct {
	SqdcValueKind kind;
	bool bool_value;
	int32_t i32_value;
	uint8_t string[SQDC_CONFIG_STRING_CAP];
	size_t string_len;
} SqdcValue;

typedef struct {
	bool present;
	uint8_t key[SQDC_CONFIG_KEY_CAP];
	size_t key_len;
	SqdcValue value;
} SqdcRecord;

typedef struct {
	SqdcRecord records[SQDC_CONFIG_MAX_RECORDS];
	size_t count;
} SqdcConfig;

typedef enum {
	SQDC_DEVICE_BINDING_RESOURCE_UNSUPPORTED = 0,
	SQDC_DEVICE_BINDING_RESOURCE_PACKAGE_SQDEVICE = 1,
	SQDC_DEVICE_BINDING_RESOURCE_INLINE_GPIO = 2,
	SQDC_DEVICE_BINDING_RESOURCE_INLINE_GPIO_BUTTON = 3,
} SqdcDeviceBindingResourceKind;

typedef struct {
	SqdcDeviceBindingResourceKind kind;
	uint8_t alias[SQVM_DEVICE_BINDING_NAME_CAP];
	size_t alias_len;
	uint8_t resource[SQVM_DEVICE_BINDING_RESOURCE_CAP];
	size_t resource_len;
} SqdcDeviceBindingPlan;

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
	uint8_t app_id[40];
	uint32_t sqbc_len;
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
	uint8_t app_id[40];
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
	uint8_t service[SQVM_DEVICE_BINDING_NAME_CAP];
	uint8_t binding[SQVM_DEVICE_BINDING_NAME_CAP];
	uint8_t resource[SQVM_DEVICE_BINDING_RESOURCE_CAP];
} SqvmDeviceBinding;

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
	bool ok;
	const uint8_t *error;
	size_t error_len;
	const uint8_t *warning;
	size_t warning_len;
	bool available;
	const uint8_t *status;
	size_t status_len;
	const uint8_t *binding;
	size_t binding_len;
	const uint8_t *driver;
	size_t driver_len;
	const uint8_t *transport;
	size_t transport_len;
	int32_t width;
	int32_t height;
	int32_t physical_width;
	int32_t physical_height;
	int32_t rotation;
	const uint8_t *color_model;
	size_t color_model_len;
	int32_t logical_gray_levels;
	int32_t native_bpp;
	const uint8_t *native_pixel_format;
	size_t native_pixel_format_len;
	int32_t default_font_height;
	bool supports_partial_refresh;
	bool supports_fast_refresh;
} SqvmDisplayInfo;
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
typedef int32_t (*SqvmDisplayInfoCallback)(void *user_data, SqvmDisplayInfo *out);

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
	bool active;
	const uint8_t *kind;
	size_t kind_len;
	const uint8_t *state;
	size_t state_len;
	bool done;
	bool cancelled;
	bool ok;
	const uint8_t *error;
	size_t error_len;
} SqvmWifiOperation;

typedef struct {
	bool ready;
	const uint8_t *kind;
	size_t kind_len;
	const uint8_t *state;
	size_t state_len;
	bool ok;
	const uint8_t *error;
	size_t error_len;
	bool cancelled;
	int32_t count;
} SqvmWifiOperationResult;

typedef struct {
	bool ok;
	const uint8_t *error;
	size_t error_len;
	SqvmWifiAccessPoint network;
} SqvmWifiScanNetworkResult;

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
	bool ok;
	const uint8_t *error;
	size_t error_len;
	const uint8_t *path;
	size_t path_len;
} SqvmFilePickFileResult;

typedef struct {
	bool ok;
	const uint8_t *error;
	size_t error_len;
	const uint8_t *text;
	size_t text_len;
} SqvmFileReadTextResult;

typedef struct {
	bool ok;
	const uint8_t *error;
	size_t error_len;
} SqvmFileReadLinesResult;

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
					   size_t ssid_len, SqvmWifiOperation *out);
typedef int32_t (*SqvmWifiStopApCallback)(void *user_data, SqvmWifiOperation *out);
typedef int32_t (*SqvmWifiConnectCallback)(void *user_data, const uint8_t *profile,
					   size_t profile_len, SqvmWifiOperation *out);
typedef int32_t (*SqvmWifiDisconnectCallback)(void *user_data, SqvmWifiOperation *out);
typedef int32_t (*SqvmWifiGetApIpCallback)(void *user_data, SqvmWifiApIp *out);
typedef int32_t (*SqvmWifiStatusCallback)(void *user_data, SqvmWifiStatus *out);
typedef int32_t (*SqvmWifiScanCallback)(void *user_data, SqvmWifiOperation *out);
typedef int32_t (*SqvmWifiOperationCallback)(void *user_data, SqvmWifiOperation *out);
typedef int32_t (*SqvmWifiResultCallback)(void *user_data, SqvmWifiOperationResult *out);
typedef int32_t (*SqvmWifiCancelCallback)(void *user_data, SqvmWifiOperation *out);
typedef int32_t (*SqvmWifiScanNetworkCallback)(void *user_data, int32_t index,
					       SqvmWifiScanNetworkResult *out);
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
typedef int32_t (*SqvmFilePickFileCallback)(void *user_data, const uint8_t *extension,
					       size_t extension_len,
					       SqvmFilePickFileResult *out);
typedef int32_t (*SqvmFileReadTextCallback)(void *user_data, const uint8_t *path,
					       size_t path_len, SqvmFileReadTextResult *out);
typedef int32_t (*SqvmFileReadLinesCallback)(void *user_data, const uint8_t *path,
						size_t path_len, int32_t max_lines,
						SqvmFileReadLinesResult *out);

typedef int32_t (*SqvmIndicatorWriteCallback)(void *user_data, bool value);
typedef int32_t (*SqvmIndicatorToggleCallback)(void *user_data);
typedef int32_t (*SqvmIndicatorReadCallback)(void *user_data, bool *out);
typedef int32_t (*SqvmIndicatorBreatheCallback)(void *user_data);
typedef int32_t (*SqvmIndicatorBlinkCallback)(void *user_data, int32_t on_ms, int32_t off_ms);
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
typedef int32_t (*SqvmSystemStartReasonTextCallback)(
	void *user_data,
	uint8_t *out,
	size_t out_cap,
	size_t *out_len);
typedef int32_t (*SqvmPowerSleepCallback)(void *user_data, int32_t wake_after_ms);

typedef struct {
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
	SqvmDisplayInfoCallback display_info;
	SqvmIndicatorWriteCallback indicator_write;
	SqvmIndicatorToggleCallback indicator_toggle;
	SqvmIndicatorReadCallback indicator_read;
	SqvmIndicatorBreatheCallback indicator_breathe;
	SqvmIndicatorBlinkCallback indicator_blink;
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
	SqvmWifiOperationCallback wifi_operation;
	SqvmWifiResultCallback wifi_result;
	SqvmWifiCancelCallback wifi_cancel;
	SqvmWifiScanNetworkCallback wifi_scan_network;
	SqvmDeviceConfigLoadCallback device_config_load;
	SqvmDeviceConfigSetCallback device_config_set;
	SqvmDeviceConfigRebindCallback device_config_rebind;
	SqvmDeviceConfigSaveCallback device_config_save;
	SqvmFilePickFileCallback file_pick_file;
	SqvmFileReadTextCallback file_read_text;
	SqvmFileReadLinesCallback file_read_lines;
	SqvmSystemMemoryTextCallback system_memory_text;
	SqvmSystemStorageTextCallback system_storage_text;
	SqvmSystemStartReasonTextCallback system_start_reason_text;
	SqvmPowerSleepCallback power_sleep;
} SqvmCallbacks;

size_t sqvm_context_size(void);
size_t sqvm_context_align(void);
size_t sqvm_storage_transfer_capacity(void);
size_t sqvm_saved_state_capacity(void);
SqdcStatus sqdc_config_clear(SqdcConfig *config);
SqdcStatus sqdc_is_safe_sqdevice_path(const uint8_t *path, size_t path_len);
SqdcStatus sqdc_parse_sqdevice(const uint8_t *input, size_t input_len, SqdcConfig *out);
SqdcStatus sqdc_config_set_null(SqdcConfig *config, const uint8_t *key, size_t key_len);
SqdcStatus sqdc_config_set_bool(SqdcConfig *config, const uint8_t *key, size_t key_len,
				bool value);
SqdcStatus sqdc_config_set_i32(SqdcConfig *config, const uint8_t *key, size_t key_len,
			       int32_t value);
SqdcStatus sqdc_config_set_string(SqdcConfig *config, const uint8_t *key, size_t key_len,
				  const uint8_t *value, size_t value_len);
SqdcStatus sqdc_encode_sqdc(const SqdcConfig *config, uint8_t *out, size_t out_cap,
			    size_t *out_len);
SqdcStatus sqdc_decode_sqdc(const uint8_t *input, size_t input_len, SqdcConfig *out);
SqdcStatus sqdc_plan_device_binding(const uint8_t *service, size_t service_len,
				    const uint8_t *binding, size_t binding_len,
				    const uint8_t *resource, size_t resource_len,
				    SqdcDeviceBindingPlan *out,
				    SqdcConfig *out_inline_config);
SqvmStatus sqvm_context_prepare(void *context, size_t context_len);
SqvmStatus sqvm_context_reset_in_place(void *context, size_t context_len);
SqvmStatus sqvm_context_init_in_place(
	void *context,
	void *user_data,
	const SqvmCallbacks *callbacks,
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
SqvmStatus sqvm_device_binding_count_from_reader(
	void *user_data,
	SqvmReadExactAtCallback read_exact_at,
	uint8_t *scratch,
	size_t scratch_len,
	size_t *out_count);
SqvmStatus sqvm_event_handler_exists_from_reader(
	void *user_data,
	SqvmReadExactAtCallback read_exact_at,
	uint8_t *scratch,
	size_t scratch_len,
	const uint8_t *event,
	size_t event_len,
	bool *out_exists);
SqvmStatus sqvm_device_binding_read_from_reader(
	void *user_data,
	SqvmReadExactAtCallback read_exact_at,
	uint8_t *scratch,
	size_t scratch_len,
	size_t index,
	SqvmDeviceBinding *out_binding);
SqvmStatus sqvm_dispatch(
	void *context,
	void *user_data,
	const SqvmCallbacks *callbacks,
	const uint8_t *event,
	size_t event_len);
SqvmStatus sqvm_dispatch_start_resumable(
	void *context,
	void *user_data,
	const SqvmCallbacks *callbacks,
	const uint8_t *event,
	size_t event_len,
	SqvmDispatchResult *out_result);
SqvmStatus sqvm_dispatch_resume_storage(
	void *context,
	void *user_data,
	const SqvmCallbacks *callbacks,
	const SqvmStorageCompletion *completion,
	SqvmDispatchResult *out_result);
void sqvm_ffi_panic_abort(void);
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
SqdpStatus sqdp_encode_lifecycle_response_from_runtime_timers(
	uint32_t sequence,
	const uint8_t *active_app,
	size_t active_app_len,
	const uint8_t *process_stack,
	size_t process_count,
	size_t process_stride,
	const uint8_t *armed_timer_base,
	size_t armed_timer_count,
	size_t armed_timer_stride,
	size_t armed_timer_active_offset,
	size_t armed_timer_app_id_offset,
	size_t armed_timer_app_id_cap,
	size_t armed_timer_event_offset,
	size_t armed_timer_event_cap,
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
