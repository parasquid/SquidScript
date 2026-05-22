#ifndef SQUIDSCRIPT_DEVICE_PROTOCOL_H
#define SQUIDSCRIPT_DEVICE_PROTOCOL_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "app_store.h"
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
	char staging_path[SQ_APP_STORE_PATH_MAX];
};

struct sq_device_temp_session {
	bool active;
	char app_id[SQ_APP_STORE_APP_ID_MAX];
	size_t total_len;
	size_t received;
	uint32_t expected_crc;
	uint32_t running_crc;
	char staging_path[SQ_APP_STORE_PATH_MAX];
};

struct sq_device_resource_session {
	bool active;
	char app_id[SQ_APP_STORE_APP_ID_MAX];
	char resource_path[SQ_APP_STORE_PATH_MAX];
	size_t total_len;
	size_t received;
	uint32_t expected_crc;
	uint32_t running_crc;
	char staging_path[SQ_APP_STORE_PATH_MAX];
};

struct sq_device_protocol_context {
	const struct sq_device_identity *identity;
	const struct sq_app_registry *registry;
	struct sq_app_registry *mutable_registry;
	struct sq_device_install_session *install_session;
	struct sq_device_temp_session *temp_session;
	struct sq_device_resource_session *resource_session;
	struct sq_vm_runtime *runtime;
	struct sq_app_store_vm_storage *launch_storage;
	const char *store_mount_point;
};

int sq_device_protocol_handle_frame(const uint8_t *request, size_t request_len,
				    const struct sq_device_protocol_context *context, uint8_t *response,
				    size_t response_cap, size_t *response_len);

#endif
