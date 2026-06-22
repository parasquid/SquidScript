#ifndef SQUIDSCRIPT_VM_RUNTIME_INTERNAL_H
#define SQUIDSCRIPT_VM_RUNTIME_INTERNAL_H

#include "vm_runtime.h"

#include "app_lifecycle.h"

#include <errno.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>

#include <zephyr/fs/fs.h>
#include <zephyr/kernel.h>
#include <zephyr/sys/sys_heap.h>

#if IS_ENABLED(CONFIG_NET_L2_WIFI_MGMT) && IS_ENABLED(CONFIG_NET_MGMT_EVENT) && \
	IS_ENABLED(CONFIG_NET_MGMT_EVENT_INFO)
#define SQ_VM_RUNTIME_HAS_WIFI_MGMT 1
#else
#define SQ_VM_RUNTIME_HAS_WIFI_MGMT 0
#endif

#define SQ_SET_LITERAL_FIELD(target, field, value) \
	do { \
		(target)->field = (const uint8_t *)(value); \
		(target)->field##_len = sizeof(value) - 1; \
	} while (false)

static inline size_t bounded_strlen(const char *value, size_t cap)
{
	size_t len = 0;

	while (len < cap && value[len] != '\0') {
		len++;
	}
	return len;
}

int32_t runtime_read_exact_at(void *user_data, size_t offset, uint8_t *out, size_t out_len);

void runtime_display_clear(void *user_data, uint8_t color);
void runtime_display_text(void *user_data, const uint8_t *text, size_t text_len,
			  const SqvmDisplayTextOptions *options);
void runtime_display_rect(void *user_data, const SqvmDisplayRectOptions *options);
void runtime_display_line(void *user_data, const SqvmDisplayLineOptions *options);
int32_t runtime_display_select(void *user_data, const uint8_t *name, size_t name_len);
void runtime_display_image(void *user_data, const uint8_t *path, size_t path_len,
			   const SqvmDisplayResourceOptions *options);
void runtime_display_draw(void *user_data, SqvmHandle drawable,
			  const SqvmDisplayResourceOptions *options);
void runtime_display_refresh_mode(void *user_data, const uint8_t *mode, size_t mode_len);
int32_t runtime_display_info(void *user_data, SqvmDisplayInfo *out);

int32_t runtime_indicator_write(void *user_data, bool value);
int32_t runtime_indicator_toggle(void *user_data);
int32_t runtime_indicator_read(void *user_data, bool *out);
int32_t runtime_indicator_breathe(void *user_data);
int32_t runtime_indicator_blink(void *user_data, int32_t on_ms, int32_t off_ms);
int32_t runtime_hardware_gpio_write(void *user_data, const uint8_t *name, size_t name_len,
				    bool value);
int32_t runtime_hardware_gpio_toggle(void *user_data, const uint8_t *name, size_t name_len);
int32_t runtime_hardware_gpio_read(void *user_data, const uint8_t *name, size_t name_len,
				   bool *out);
int parse_gpio_name(const uint8_t *name, size_t name_len, uint8_t *pin);
bool target_gpio_pin_supported(uint8_t pin);
int configure_input_button_gpio(uint8_t pin, bool active_low, bool *pressed);
int sq_vm_runtime_poll_indicator(struct sq_vm_runtime *runtime);
int sq_vm_runtime_poll_input_buttons(struct sq_vm_runtime *runtime);

int32_t runtime_app_launch(void *user_data, const uint8_t *app, size_t app_len);
int32_t runtime_app_arm(void *user_data, const uint8_t *app, size_t app_len);
int32_t runtime_app_disarm(void *user_data, const uint8_t *app, size_t app_len);
int32_t runtime_app_install_file(void *user_data, const uint8_t *file_ref, size_t file_ref_len,
				 const uint8_t *app_id, size_t app_id_len, uint8_t *out_app_id,
				 size_t out_app_id_cap, size_t *out_app_id_len);
int32_t runtime_app_registry_list(void *user_data, SqvmAppRegistryEntry *out, size_t out_cap,
				  size_t *out_count);
int32_t runtime_app_registry_get(void *user_data, const uint8_t *app, size_t app_len,
				 SqvmAppRegistryEntry *out);
int32_t runtime_app_process_stack(void *user_data, SqvmAppStackEntry *out, size_t out_cap,
				  size_t *out_count);
int32_t runtime_app_armed_stack(void *user_data, SqvmAppStackEntry *out, size_t out_cap,
				size_t *out_count);

int32_t runtime_timer_every(void *user_data, const uint8_t *event, size_t event_len,
			    int32_t interval_ms);
int32_t runtime_timer_after(void *user_data, const uint8_t *event, size_t event_len,
			    int32_t delay_ms);
int32_t runtime_ble_start(void *user_data, const uint8_t *id, size_t id_len);
int32_t runtime_ble_stop(void *user_data);
int32_t runtime_http_start(void *user_data, const uint8_t *id, size_t id_len);
int32_t runtime_http_stop(void *user_data);
int32_t runtime_power_sleep(void *user_data, int32_t wake_after_ms);

int32_t runtime_system_memory_text(void *user_data, uint8_t *out, size_t out_cap,
				   size_t *out_len);
int32_t runtime_system_storage_text(void *user_data, const uint8_t *name, size_t name_len,
				    uint8_t *out, size_t out_cap, size_t *out_len);
int32_t runtime_system_start_reason_text(void *user_data, uint8_t *out, size_t out_cap,
					 size_t *out_len);

int32_t runtime_wifi_start_ap(void *user_data, const uint8_t *ssid, size_t ssid_len,
			      SqvmWifiOperation *out);
int32_t runtime_wifi_stop_ap(void *user_data, SqvmWifiOperation *out);
int32_t runtime_wifi_connect(void *user_data, const uint8_t *profile, size_t profile_len,
			     SqvmWifiOperation *out);
int32_t runtime_wifi_disconnect(void *user_data, SqvmWifiOperation *out);
int32_t runtime_wifi_get_ap_ip(void *user_data, SqvmWifiApIp *out);
int32_t runtime_wifi_status(void *user_data, SqvmWifiStatus *out);
int32_t runtime_wifi_scan(void *user_data, SqvmWifiOperation *out);
int32_t runtime_wifi_operation(void *user_data, SqvmWifiOperation *out);
int32_t runtime_wifi_result(void *user_data, SqvmWifiOperationResult *out);
int32_t runtime_wifi_cancel(void *user_data, SqvmWifiOperation *out);
int32_t runtime_wifi_scan_network(void *user_data, int32_t index,
				  SqvmWifiScanNetworkResult *out);
void sq_vm_runtime_wifi_reset_target(struct sq_vm_runtime *runtime);
void sq_vm_runtime_wifi_reset_platform(struct sq_vm_runtime *runtime);

int32_t runtime_device_config_load(void *user_data, const uint8_t *source, size_t source_len,
				   SqvmDeviceConfigResult *out);
int32_t runtime_device_config_set(void *user_data, const uint8_t *key, size_t key_len,
				  SqvmDeviceConfigValue value, SqvmDeviceConfigResult *out);
int32_t runtime_device_config_rebind(void *user_data, const uint8_t *alias, size_t alias_len,
				     SqvmDeviceConfigResult *out);
int32_t runtime_device_config_save(void *user_data, const uint8_t *destination,
				   size_t destination_len, SqvmDeviceConfigResult *out);
void runtime_clear_active_bindings(struct sq_vm_runtime *runtime);
int sq_vm_runtime_apply_target_default_indicator_binding(struct sq_vm_runtime *runtime);
int __noinline sq_vm_runtime_prepare_app_start(struct sq_vm_runtime *runtime);

int32_t runtime_file_pick_file(void *user_data, const uint8_t *extension, size_t extension_len,
			       SqvmFilePickFileResult *out);
int32_t runtime_file_read_text(void *user_data, const uint8_t *path, size_t path_len,
			       SqvmFileReadTextResult *out);
int32_t runtime_file_read_lines(void *user_data, const uint8_t *path, size_t path_len,
				int32_t max_lines, SqvmFileReadLinesResult *out);
int32_t runtime_file_copy(void *user_data, const uint8_t *source, size_t source_len,
			  const uint8_t *library, size_t library_len, const uint8_t *name,
			  size_t name_len, SqvmFileCopyResult *out);
int32_t runtime_binbook_open(void *user_data, const uint8_t *path, size_t path_len,
			     SqvmBinBookOpenResult *out);
int32_t runtime_binbook_info(void *user_data, SqvmHandle book, SqvmBinBookInfoResult *out);
int32_t runtime_binbook_read_page(void *user_data, SqvmHandle book, int32_t page_index,
				  SqvmBinBookReadPageResult *out);
int32_t runtime_binbook_chapters(void *user_data, SqvmHandle book, int32_t offset,
				 int32_t limit, SqvmBinBookChapterEntry *out, size_t out_cap,
				 size_t *out_count, SqvmBinBookChapterListResult *out_result);
int32_t runtime_binbook_chapter(void *user_data, SqvmHandle book, int32_t index,
				SqvmBinBookChapterResult *out);
int runtime_binbook_validate_path(const char *path);

struct rust_binbook_page_meta {
	bool ok;
	uint16_t pixel_format;
	uint16_t compression_method;
	uint64_t blob_offset;
	uint32_t compressed_size;
	uint32_t uncompressed_size;
	uint16_t stored_width;
	uint16_t stored_height;
};

int32_t rust_binbook_page_meta(const uint8_t *data, size_t data_len,
			       uint64_t page_index_offset, uint64_t page_data_offset,
			       uint32_t page_index, struct rust_binbook_page_meta *out);
int32_t rust_binbook_decompress_page(const uint8_t *compressed, size_t compressed_len,
				     uint16_t compression_method, uint8_t *out,
				     size_t out_len, size_t uncompressed_size);

int32_t runtime_content_binbook_list(void *user_data, const uint8_t *library,
				     size_t library_len, int32_t offset, int32_t limit,
				     SqvmContentBinBookEntry *out, size_t out_cap,
				     size_t *out_count,
				     SqvmContentBinBookListResult *out_result);
int runtime_content_resolve_binbook_ref(struct sq_vm_runtime *runtime, const uint8_t *ref,
					size_t ref_len, char *out, size_t out_len);

#endif
