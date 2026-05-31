#ifndef SQUIDSCRIPT_DEVICE_PROTOCOL_H
#define SQUIDSCRIPT_DEVICE_PROTOCOL_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include <zephyr/fs/fs.h>

#include "app_store.h"
#include "fallback_app.h"
#include "vm_runtime.h"

#define SQ_DEVICE_FIELD_TARGET 1
#define SQ_DEVICE_FIELD_FIRMWARE 2
#define SQ_DEVICE_FIELD_DIAGNOSTIC 3

#define SQ_DEVICE_APP_LIST_FIELD_APP 1
#define SQ_DEVICE_APP_FIELD_ID 1
#define SQ_DEVICE_APP_FIELD_SQBC_LEN 2
#define SQ_DEVICE_INSTALL_FIELD_APP_ID 1
#define SQ_DEVICE_INSTALL_FIELD_TOTAL_LEN 2
#define SQ_DEVICE_INSTALL_FIELD_CRC32 3
#define SQ_DEVICE_CHUNK_FIELD_OFFSET 1
#define SQ_DEVICE_CHUNK_FIELD_BYTES 2
#define SQ_DEVICE_RESOURCE_FIELD_APP_ID 1
#define SQ_DEVICE_RESOURCE_FIELD_PATH 2
#define SQ_DEVICE_RESOURCE_FIELD_TOTAL_LEN 3
#define SQ_DEVICE_RESOURCE_FIELD_CRC32 4
#define SQ_DEVICE_LINE_FIELD_VALUE 1
#define SQ_DEVICE_RECORD_FIELD_ENTRY 1
#define SQ_DEVICE_RECORD_FIELD_KEY 1
#define SQ_DEVICE_RECORD_FIELD_VALUE 2
#define SQ_DEVICE_STATE_FIELD_BYTES 1
#define SQ_DEVICE_ERROR_FIELD_CODE 250
#define SQ_DEVICE_ERROR_FIELD_MESSAGE 251
#define SQ_DEVICE_INSTALL_MAX_BYTES 65536u
#define SQ_DEVICE_TEMP_RUN_MAX_BYTES SQ_DEVICE_INSTALL_MAX_BYTES
#define SQ_DEVICE_TEMP_STATE_BYTES SQVM_SAVED_STATE_CAPACITY
#define SQ_DEVICE_RESPONSE_BYTES 916u
#define SQ_DEVICE_STAGING_PATH_BYTES 80u
#define SQ_DEVICE_RESOURCE_PATH_BYTES 80u
#define SQ_DEVICE_WIFI_PROFILE_NAME_BYTES 16
#define SQ_DEVICE_WIFI_PROFILE_SSID_BYTES 32
#define SQ_DEVICE_WIFI_PROFILE_PASSWORD_BYTES 64
#define SQ_DEVICE_PLANNED_RESUME_MAGIC "SQPR"
#define SQ_DEVICE_PLANNED_RESUME_VERSION 1u
#define SQ_DEVICE_PLANNED_RESUME_LEN                                                   \
	(4u + 1u + SQ_APP_STORE_APP_ID_MAX + 1u +                                      \
	 (SQ_VM_RUNTIME_RETURN_STACK_MAX * SQ_APP_STORE_APP_ID_MAX) + 1u +             \
	 (SQ_VM_RUNTIME_ARMED_TIMER_MAX * SQ_APP_STORE_APP_ID_MAX))

struct sq_device_identity {
	const char *target;
	const char *firmware;
	bool diagnostic;
};

struct sq_device_install_session {
	bool active;
	char app_id[SQ_APP_STORE_APP_ID_MAX];
	size_t total_len;
	size_t received;
	uint32_t expected_crc;
	uint32_t running_crc;
	char staging_path[SQ_DEVICE_STAGING_PATH_BYTES];
};

struct sq_device_temp_session {
	bool active;
	char app_id[SQ_APP_STORE_APP_ID_MAX];
	size_t total_len;
	size_t received;
	uint32_t expected_crc;
	uint32_t running_crc;
	char staging_path[SQ_DEVICE_STAGING_PATH_BYTES];
};

struct sq_device_resource_session {
	bool active;
	char app_id[SQ_APP_STORE_APP_ID_MAX];
	char resource_path[SQ_DEVICE_RESOURCE_PATH_BYTES];
	size_t total_len;
	size_t received;
	uint32_t expected_crc;
	uint32_t running_crc;
	char staging_path[SQ_DEVICE_STAGING_PATH_BYTES];
};

enum sq_device_protocol_scratch_owner {
	SQ_DEVICE_PROTOCOL_SCRATCH_FREE = 0,
	SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME = 1,
};

struct sq_device_planned_resume_record {
	char current_app[SQ_APP_STORE_APP_ID_MAX];
	uint8_t return_stack_count;
	char return_stack[SQ_VM_RUNTIME_RETURN_STACK_MAX][SQ_APP_STORE_APP_ID_MAX];
	uint8_t armed_app_count;
	char armed_apps[SQ_VM_RUNTIME_ARMED_TIMER_MAX][SQ_APP_STORE_APP_ID_MAX];
};

struct sq_device_protocol_scratch {
	enum sq_device_protocol_scratch_owner owner;
	struct sq_device_planned_resume_record planned_resume_record;
	uint8_t planned_resume_bytes[SQ_DEVICE_PLANNED_RESUME_LEN];
	size_t planned_resume_len;
	char planned_resume_temp_path[SQ_APP_STORE_PLANNED_RESUME_PATH_MAX];
	char planned_resume_final_path[SQ_APP_STORE_PLANNED_RESUME_PATH_MAX];
	struct fs_file_t planned_resume_file;
};

struct sq_device_protocol_context {
	const struct sq_device_identity *identity;
	const struct sq_app_registry *registry;
	struct sq_app_registry *mutable_registry;
	struct sq_device_install_session *install_session;
	struct sq_device_temp_session *temp_session;
	struct sq_device_resource_session *resource_session;
	struct sq_device_protocol_scratch *scratch;
	struct sq_vm_runtime *runtime;
	struct sq_app_store_vm_storage *launch_storage;
	struct sq_app_store_vm_storage *trigger_storage;
	const struct sq_firmware_fallback_app *fallback_app;
	const char *store_mount_point;
};

int sq_device_protocol_encode_planned_resume(
	const struct sq_device_planned_resume_record *record, uint8_t *out, size_t out_cap,
	size_t *out_len);
int sq_device_protocol_decode_planned_resume(const uint8_t *bytes, size_t len,
					     struct sq_device_planned_resume_record *out);
int sq_device_protocol_planned_resume_from_runtime(
	const struct sq_vm_runtime *runtime, struct sq_device_planned_resume_record *out);

int sq_device_protocol_handle_frame(const uint8_t *request, size_t request_len,
				    const struct sq_device_protocol_context *context, uint8_t *response,
				    size_t response_cap, size_t *response_len);

int sq_device_protocol_poll(const struct sq_device_protocol_context *context);

int sq_device_protocol_start_root(const struct sq_device_protocol_context *context);

int sq_device_protocol_restore_planned_resume(const struct sq_device_protocol_context *context);

#endif
