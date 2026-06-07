#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <errno.h>

#include <zephyr/fs/fs.h>
#include <zephyr/ztest.h>

#include "app_store.h"
#include "app_lifecycle.h"
#include "device_protocol.h"
#include "protocol.h"
#include "serial_transport.h"
#include "squidscript_fallback_app.h"
#include "squidscript_protocol_fixtures.h"
#include "squidscript_target_defaults.h"
#include "vm_runtime.h"
#include "vm_runtime_internal.h"
#include "vm_fs_storage.h"
#include "squidvm_ffi.h"
#include "vm_storage.h"

#define SQ_PROTOCOL_DONE 1

enum sq_test_field_type {
	SQ_FIELD_BYTES = 0,
	SQ_FIELD_STRING = 1,
	SQ_FIELD_BOOL = 3,
	SQ_FIELD_I64 = 4,
	SQ_FIELD_U64 = 5,
	SQ_FIELD_U32 = 6,
	SQ_FIELD_RECORD = 32,
};

struct sq_protocol_field {
	uint8_t tag;
	uint8_t type;
	const uint8_t *value;
	uint16_t len;
};

static bool runtime_has_active_binding(const struct sq_vm_runtime *runtime, const char *alias)
{
	if (runtime == NULL || alias == NULL) {
		return false;
	}
	for (size_t i = 0; i < runtime->active_binding_count; i++) {
		if (runtime->active_bindings[i].active &&
		    strcmp(runtime->active_bindings[i].alias, alias) == 0) {
			return true;
		}
	}
	return false;
}

static int test_wifi_reset_platform_calls;
static enum sq_vm_runtime_wifi_op_kind test_wifi_reset_platform_kind;
static enum sq_vm_runtime_wifi_service_state test_wifi_reset_platform_state;
static bool test_wifi_reset_platform_ap_active;

void sq_vm_runtime_wifi_reset_platform(struct sq_vm_runtime *runtime)
{
	test_wifi_reset_platform_calls++;
	if (runtime == NULL) {
		return;
	}
	test_wifi_reset_platform_kind = runtime->wifi_op_kind;
	test_wifi_reset_platform_state = runtime->wifi_service_state;
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	test_wifi_reset_platform_ap_active = runtime->wifi_ap_active;
#else
	test_wifi_reset_platform_ap_active = false;
#endif
}

static void reset_wifi_reset_platform_observer(void)
{
	test_wifi_reset_platform_calls = 0;
	test_wifi_reset_platform_kind = SQ_VM_RUNTIME_WIFI_OP_NONE;
	test_wifi_reset_platform_state = SQ_VM_RUNTIME_WIFI_SERVICE_IDLE;
	test_wifi_reset_platform_ap_active = false;
}

static void write_u32_le(uint8_t *bytes, uint32_t value)
{
	bytes[0] = value & 0xff;
	bytes[1] = (value >> 8) & 0xff;
	bytes[2] = (value >> 16) & 0xff;
	bytes[3] = (value >> 24) & 0xff;
}

static int sq_protocol_next_field(const uint8_t *payload, size_t payload_len, size_t *offset,
				  struct sq_protocol_field *out)
{
	if (*offset == payload_len) {
		return SQ_PROTOCOL_DONE;
	}
	if (*offset > payload_len || payload_len - *offset < 4u) {
		return SQ_PROTOCOL_ERR_TRUNCATED_FIELD;
	}

	const uint8_t *field = &payload[*offset];
	uint16_t field_len = (uint16_t)field[2] | ((uint16_t)field[3] << 8);
	size_t next_offset = *offset + 4u + field_len;

	if (next_offset > payload_len) {
		return SQ_PROTOCOL_ERR_TRUNCATED_FIELD;
	}

	out->tag = field[0];
	out->type = field[1];
	out->len = field_len;
	out->value = &field[4];
	*offset = next_offset;

	return SQ_PROTOCOL_OK;
}

static uint64_t sq_protocol_read_u64_le(const uint8_t *bytes)
{
	uint64_t value = 0;

	for (int i = 7; i >= 0; i--) {
		value <<= 8;
		value |= bytes[i];
	}

	return value;
}

static uint32_t sq_protocol_read_u32_le(const uint8_t *bytes)
{
	uint32_t value = 0;

	for (int i = 3; i >= 0; i--) {
		value <<= 8;
		value |= bytes[i];
	}

	return value;
}

static int sq_protocol_append_bytes_field(uint8_t *payload, size_t cap, size_t *len, uint8_t tag,
					  const uint8_t *value, uint16_t value_len)
{
	size_t needed = *len + 4u + value_len;

	if (payload == NULL || len == NULL || value == NULL) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	if (needed > cap) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	payload[*len] = tag;
	payload[*len + 1u] = SQ_FIELD_BYTES;
	payload[*len + 2u] = value_len & 0xffu;
	payload[*len + 3u] = (value_len >> 8) & 0xffu;
	memcpy(&payload[*len + 4u], value, value_len);
	*len = needed;
	return SQ_PROTOCOL_OK;
}

static int sq_protocol_append_string_field(uint8_t *payload, size_t cap, size_t *len, uint8_t tag,
					   const char *value)
{
	size_t value_len;
	size_t needed;

	if (payload == NULL || len == NULL || value == NULL) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	value_len = strlen(value);
	if (value_len > UINT16_MAX) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}
	needed = *len + 4u + value_len;
	if (needed > cap) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	payload[*len] = tag;
	payload[*len + 1u] = SQ_FIELD_STRING;
	payload[*len + 2u] = value_len & 0xffu;
	payload[*len + 3u] = (value_len >> 8) & 0xffu;
	memcpy(&payload[*len + 4u], value, value_len);
	*len = needed;
	return SQ_PROTOCOL_OK;
}

static int sq_protocol_append_u64_field(uint8_t *payload, size_t cap, size_t *len, uint8_t tag,
					uint64_t value)
{
	uint8_t encoded[8] = {
		value & 0xffu,
		(value >> 8) & 0xffu,
		(value >> 16) & 0xffu,
		(value >> 24) & 0xffu,
		(value >> 32) & 0xffu,
		(value >> 40) & 0xffu,
		(value >> 48) & 0xffu,
		(value >> 56) & 0xffu,
	};

	size_t needed = *len + 4u + sizeof(encoded);
	if (payload == NULL || len == NULL || needed > cap) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	payload[*len] = tag;
	payload[*len + 1u] = SQ_FIELD_U64;
	payload[*len + 2u] = sizeof(encoded);
	payload[*len + 3u] = 0;
	memcpy(&payload[*len + 4u], encoded, sizeof(encoded));
	*len = needed;
	return SQ_PROTOCOL_OK;
}

static int sq_protocol_append_u32_field(uint8_t *payload, size_t cap, size_t *len, uint8_t tag,
					uint32_t value)
{
	uint8_t encoded[4] = {
		value & 0xffu,
		(value >> 8) & 0xffu,
		(value >> 16) & 0xffu,
		(value >> 24) & 0xffu,
	};
	size_t needed = *len + 4u + sizeof(encoded);

	if (payload == NULL || len == NULL || needed > cap) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	payload[*len] = tag;
	payload[*len + 1u] = SQ_FIELD_U32;
	payload[*len + 2u] = sizeof(encoded);
	payload[*len + 3u] = 0;
	memcpy(&payload[*len + 4u], encoded, sizeof(encoded));
	*len = needed;
	return SQ_PROTOCOL_OK;
}

static int sq_protocol_encode_frame_header(uint8_t kind, uint8_t opcode, uint8_t status,
					   uint32_t sequence, const uint8_t *payload,
					   size_t payload_len, uint8_t *out, size_t out_len)
{
	if (out_len < SQ_PROTOCOL_HEADER_LEN) {
		return SQ_PROTOCOL_ERR_BUFFER_TOO_SMALL;
	}

	memcpy(out, "SQDP", 4);
	out[4] = kind;
	out[5] = opcode;
	out[6] = status;
	out[7] = 0;
	write_u32_le(&out[8], sequence);
	write_u32_le(&out[12], (uint32_t)payload_len);
	write_u32_le(&out[16], sq_protocol_crc32(payload, payload_len));

	return SQ_PROTOCOL_OK;
}

static const uint8_t hello_frame[] = {
	0x53, 0x51, 0x44, 0x50, 0x01, 0x01, 0x00, 0x00,
	0x07, 0x00, 0x00, 0x00, 0x26, 0x00, 0x00, 0x00,
	0x43, 0xa5, 0x05, 0x5c, 0x01, 0x01, 0x11, 0x00,
	0x65, 0x73, 0x70, 0x33, 0x32, 0x63, 0x33, 0x2d,
	0x73, 0x75, 0x70, 0x65, 0x72, 0x6d, 0x69, 0x6e,
	0x69, 0x02, 0x03, 0x01, 0x00, 0x01, 0x03, 0x05,
	0x08, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x00,
};

static bool field_string_equals(const struct sq_protocol_field *field, const char *expected)
{
	return field->type == SQ_FIELD_STRING && field->len == strlen(expected) &&
	       memcmp(field->value, expected, field->len) == 0;
}

static uint8_t ffi_context_storage[65536] __aligned(8);
static uint8_t ffi_scratch[4096];

static struct fs_mount_t test_fs_mount = {
	.type = FS_NATIVE_MOUNT,
	.mnt_point = "/sqtest",
	.fs_data = TEST_FS_DIR,
};

ZTEST_SUITE(squidscript_protocol, NULL, NULL, NULL, NULL, NULL);

static int write_test_file(const char *path, const uint8_t *bytes, size_t len);
static int mount_test_fs(void);
static int format_test_app_store(void);

ZTEST(squidscript_protocol, test_planned_resume_record_round_trips)
{
	struct sq_device_planned_resume_record record = {0};
	struct sq_device_planned_resume_record decoded = {0};
	uint8_t bytes[256];
	size_t len = 0;

	strcpy(record.current_app, "reader");
	record.return_stack_count = 2;
	strcpy(record.return_stack[0], "main");
	strcpy(record.return_stack[1], "library");
	record.armed_app_count = 2;
	strcpy(record.armed_apps[0], "break-reminder");
	strcpy(record.armed_apps[1], "clock");

	zassert_equal(sq_device_protocol_encode_planned_resume(&record, bytes, sizeof(bytes), &len),
		      0);
	zassert_true(len > 0);
	zassert_equal(sq_device_protocol_decode_planned_resume(bytes, len, &decoded), 0);
	zassert_str_equal(decoded.current_app, "reader");
	zassert_equal(decoded.return_stack_count, 2);
	zassert_str_equal(decoded.return_stack[0], "main");
	zassert_str_equal(decoded.return_stack[1], "library");
	zassert_equal(decoded.armed_app_count, 2);
	zassert_str_equal(decoded.armed_apps[0], "break-reminder");
	zassert_str_equal(decoded.armed_apps[1], "clock");
}

ZTEST(squidscript_protocol, test_planned_resume_record_rejects_bad_magic)
{
	struct sq_device_planned_resume_record record = {0};
	struct sq_device_planned_resume_record decoded = {0};
	uint8_t bytes[256];
	size_t len = 0;

	strcpy(record.current_app, "reader");
	zassert_equal(sq_device_protocol_encode_planned_resume(&record, bytes, sizeof(bytes), &len),
		      0);
	bytes[0] = 'X';
	zassert_not_equal(sq_device_protocol_decode_planned_resume(bytes, len, &decoded), 0);
}

ZTEST(squidscript_protocol, test_planned_resume_record_deduplicates_armed_app_ids)
{
	struct sq_vm_runtime runtime = {0};
	struct sq_device_planned_resume_record record = {0};

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_reset(&runtime);
	strcpy(runtime.current_app, "reader");
	runtime.return_stack_count = 1;
	strcpy(runtime.return_stack[0], "main");
	zassert_equal(sq_vm_runtime_register_armed_timer(&runtime, "break-reminder",
							 "timer.break", strlen("timer.break"),
							 30000, true),
		      0);
	zassert_equal(sq_vm_runtime_register_armed_timer(&runtime, "break-reminder",
							 "timer.stretch", strlen("timer.stretch"),
							 60000, true),
		      0);

	zassert_equal(sq_device_protocol_planned_resume_from_runtime(&runtime, &record), 0);
	zassert_str_equal(record.current_app, "reader");
	zassert_equal(record.return_stack_count, 1);
	zassert_str_equal(record.return_stack[0], "main");
	zassert_equal(record.armed_app_count, 1);
	zassert_str_equal(record.armed_apps[0], "break-reminder");
}

ZTEST(squidscript_protocol, test_planned_resume_rejects_temp_foreground_app)
{
	struct sq_vm_runtime runtime = {0};
	struct sq_device_planned_resume_record record = {0};

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_reset(&runtime);
	strcpy(runtime.current_app, "temp-reader");
	runtime.current_app_temp = true;

	zassert_equal(sq_device_protocol_planned_resume_from_runtime(&runtime, &record), -ENOTSUP);
}

ZTEST(squidscript_protocol, test_lifecycle_state_machine_restores_planned_route)
{
	struct sq_vm_runtime runtime = {0};
	char return_stack[SQ_VM_RUNTIME_RETURN_STACK_MAX][SQ_APP_STORE_APP_ID_MAX] = {0};

	strncpy(runtime.current_app, "old-app", sizeof(runtime.current_app) - 1);
	runtime.return_stack_count = 1;
	strncpy(runtime.return_stack[0], "old-root", sizeof(runtime.return_stack[0]) - 1);
	runtime.lifecycle_phase = SQ_VM_RUNTIME_LIFECYCLE_LAUNCH_REQUESTED;
	strncpy(runtime.lifecycle_target_app, "stale-target",
		sizeof(runtime.lifecycle_target_app) - 1);
	runtime.dispatch_exited = true;

	strncpy(return_stack[0], "main", sizeof(return_stack[0]) - 1);
	strncpy(return_stack[1], "library", sizeof(return_stack[1]) - 1);

	zassert_equal(sq_app_lifecycle_restore_planned_route(&runtime, return_stack, 2), 0);
	zassert_equal(runtime.lifecycle_phase, SQ_VM_RUNTIME_LIFECYCLE_IDLE);
	zassert_false(runtime.dispatch_exited);
	zassert_str_equal(runtime.lifecycle_target_app, "");
	zassert_str_equal(runtime.start_reason, "wake");
	zassert_equal(runtime.return_stack_count, 2);
	zassert_str_equal(runtime.return_stack[0], "main");
	zassert_str_equal(runtime.return_stack[1], "library");
}

ZTEST(squidscript_protocol, test_planned_resume_scratch_rejects_overlap)
{
	struct sq_device_planned_resume_record record = {0};
	uint8_t bytes[SQ_DEVICE_PLANNED_RESUME_LEN];
	char path[SQ_APP_STORE_PLANNED_RESUME_PATH_MAX];
	size_t len = 0;
	struct sq_vm_runtime runtime = {0};
	struct sq_app_registry registry = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_scratch scratch = {
		.owner = SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME,
	};
	struct sq_device_protocol_context context = {
		.registry = &registry,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
		.scratch = &scratch,
		.fallback_app = &sq_zephyr_fallback_app,
	};

	strcpy(record.current_app, "reader");
	zassert_equal(sq_device_protocol_encode_planned_resume(&record, bytes, sizeof(bytes), &len),
		      0);
	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	zassert_equal(sq_app_store_planned_resume_path(test_fs_mount.mnt_point, path, sizeof(path)),
		      0);
	zassert_equal(write_test_file(path, bytes, len), 0);

	zassert_equal(sq_device_protocol_restore_planned_resume(&context), -EBUSY);
	zassert_equal(scratch.owner, SQ_DEVICE_PROTOCOL_SCRATCH_PLANNED_RESUME);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

static void wait_runtime_done(struct sq_vm_runtime *runtime)
{
	(void)sq_vm_runtime_wait_idle(runtime, 1000);
}

static int poll_until_current_app(const struct sq_device_protocol_context *context,
				  struct sq_vm_runtime *runtime, const char *app_id)
{
	for (int i = 0; i < 600; i++) {
		int result = sq_device_protocol_poll(context);

		if (result != 0) {
			return result;
		}
		wait_runtime_done(runtime);
		if (strcmp(runtime->current_app, app_id) == 0 &&
		    runtime->status != SQ_VM_RUNTIME_RUNNING) {
			return 0;
		}
		k_sleep(K_MSEC(1));
	}
	return -ETIMEDOUT;
}

static int poll_until_output_count(const struct sq_device_protocol_context *context,
				   struct sq_vm_runtime *runtime, uint8_t output_count)
{
	for (int i = 0; i < 600; i++) {
		int result = sq_device_protocol_poll(context);

		if (result != 0) {
			return result;
		}
		wait_runtime_done(runtime);
		if (runtime->output_count >= output_count &&
		    runtime->status != SQ_VM_RUNTIME_RUNNING) {
			return 0;
		}
		k_sleep(K_MSEC(1));
	}
	return -ETIMEDOUT;
}

static void clear_runtime_lines(struct sq_vm_runtime *runtime)
{
	memset(runtime->traces, 0, sizeof(runtime->traces));
	runtime->trace_count = 0;
	memset(runtime->outputs, 0, sizeof(runtime->outputs));
	runtime->output_count = 0;
	memset(runtime->drawlog, 0, sizeof(runtime->drawlog));
	runtime->drawlog_count = 0;
}

static void start_test_root(const struct sq_device_protocol_context *context,
			    struct sq_vm_runtime *runtime)
{
	zassert_equal(sq_device_protocol_start_root(context), 0);
	wait_runtime_done(runtime);
	zassert_str_equal(runtime->current_app, "main");
	clear_runtime_lines(runtime);
}

static int write_test_file(const char *path, const uint8_t *bytes, size_t len)
{
	struct fs_file_t file;
	int result;

	fs_file_t_init(&file);
	result = fs_open(&file, path, FS_O_CREATE | FS_O_WRITE | FS_O_TRUNC);
	if (result != 0) {
		return result;
	}

	ssize_t written = fs_write(&file, bytes, len);
	result = fs_close(&file);
	if (written < 0) {
		return (int)written;
	}
	if ((size_t)written != len) {
		return -EIO;
	}
	return result;
}

#define TEST_BINBOOK_HEADER_SIZE 256U
#define TEST_BINBOOK_SECTION_ENTRY_SIZE 40U
#define TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE 76U
#define TEST_BINBOOK_SECTION_COUNT 2U
#define TEST_BINBOOK_PAGE_INDEX_OFFSET \
	(TEST_BINBOOK_HEADER_SIZE + TEST_BINBOOK_SECTION_COUNT * TEST_BINBOOK_SECTION_ENTRY_SIZE)
#define TEST_BINBOOK_PAGE_DATA_OFFSET \
	(TEST_BINBOOK_PAGE_INDEX_OFFSET + TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE)
#define TEST_BINBOOK_PAGE_DATA_LEN 4U
#define TEST_BINBOOK_LEN (TEST_BINBOOK_PAGE_DATA_OFFSET + TEST_BINBOOK_PAGE_DATA_LEN)

static void test_write_le16(uint8_t *out, uint16_t value)
{
	out[0] = (uint8_t)(value & 0xff);
	out[1] = (uint8_t)(value >> 8);
}

static void test_write_le32(uint8_t *out, uint32_t value)
{
	out[0] = (uint8_t)(value & 0xff);
	out[1] = (uint8_t)((value >> 8) & 0xff);
	out[2] = (uint8_t)((value >> 16) & 0xff);
	out[3] = (uint8_t)(value >> 24);
}

static void test_write_le64(uint8_t *out, uint64_t value)
{
	test_write_le32(out, (uint32_t)value);
	test_write_le32(out + 4, (uint32_t)(value >> 32));
}

static void test_write_binbook_section(uint8_t *out, uint16_t section_id, uint64_t offset,
				       uint64_t length, uint32_t entry_size,
				       uint32_t record_count)
{
	memset(out, 0, TEST_BINBOOK_SECTION_ENTRY_SIZE);
	test_write_le16(&out[0], section_id);
	test_write_le64(&out[4], offset);
	test_write_le64(&out[12], length);
	test_write_le32(&out[20], entry_size);
	test_write_le32(&out[24], record_count);
}

static void build_test_binbook(uint8_t out[TEST_BINBOOK_LEN])
{
	memset(out, 0, TEST_BINBOOK_LEN);
	memcpy(&out[0], "BINBOOK", 7);
	test_write_le16(&out[8], 0);
	test_write_le16(&out[10], 1);
	test_write_le16(&out[12], TEST_BINBOOK_HEADER_SIZE);
	test_write_le64(&out[16], TEST_BINBOOK_LEN);
	test_write_le64(&out[24], TEST_BINBOOK_HEADER_SIZE);
	test_write_le32(&out[32], TEST_BINBOOK_SECTION_COUNT * TEST_BINBOOK_SECTION_ENTRY_SIZE);
	test_write_le16(&out[36], TEST_BINBOOK_SECTION_ENTRY_SIZE);
	test_write_le16(&out[38], TEST_BINBOOK_SECTION_COUNT);
	test_write_le16(&out[40], TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE);
	test_write_le16(&out[42], 48);
	test_write_le64(&out[44], TEST_BINBOOK_PAGE_DATA_OFFSET);
	test_write_le64(&out[52], TEST_BINBOOK_PAGE_DATA_LEN);

	test_write_binbook_section(&out[TEST_BINBOOK_HEADER_SIZE], 40,
				   TEST_BINBOOK_PAGE_INDEX_OFFSET,
				   TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE,
				   TEST_BINBOOK_PAGE_INDEX_ENTRY_SIZE, 1);
	test_write_binbook_section(&out[TEST_BINBOOK_HEADER_SIZE + TEST_BINBOOK_SECTION_ENTRY_SIZE],
				   50, TEST_BINBOOK_PAGE_DATA_OFFSET,
				   TEST_BINBOOK_PAGE_DATA_LEN, 0, 0);

	test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET], 0);
	test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 4], 1);
	test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 6], 2);
	test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 8], 1);
	test_write_le64(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 16], 0);
	test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 24], TEST_BINBOOK_PAGE_DATA_LEN);
	test_write_le32(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 28], 96000);
	test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 36], 800);
	test_write_le16(&out[TEST_BINBOOK_PAGE_INDEX_OFFSET + 38], 480);
	out[TEST_BINBOOK_PAGE_DATA_OFFSET] = 3;
	out[TEST_BINBOOK_PAGE_DATA_OFFSET + 1] = 0xff;
	out[TEST_BINBOOK_PAGE_DATA_OFFSET + 2] = 0xff;
	out[TEST_BINBOOK_PAGE_DATA_OFFSET + 3] = 0xff;
}

static int read_test_file(const char *path, uint8_t *bytes, size_t cap, size_t *out_len)
{
	struct fs_dirent entry;
	struct fs_file_t file;
	int result;

	if (bytes == NULL || out_len == NULL) {
		return -EINVAL;
	}
	*out_len = 0;
	result = fs_stat(path, &entry);
	if (result != 0) {
		return result;
	}
	if (entry.type != FS_DIR_ENTRY_FILE || entry.size > cap) {
		return -EINVAL;
	}

	fs_file_t_init(&file);
	result = fs_open(&file, path, FS_O_READ);
	if (result != 0) {
		return result;
	}
	ssize_t read = fs_read(&file, bytes, entry.size);
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

static int unlink_test_file_if_exists(const char *path)
{
	struct fs_dirent entry;
	int result = fs_stat(path, &entry);

	if (result == -ENOENT) {
		return 0;
	}
	if (result != 0) {
		return result;
	}
	return fs_unlink(path);
}

static bool resource_value_for_key(const struct sq_protocol_frame *frame, const char *key,
				   uint64_t *out)
{
	struct resource_metric_name {
		uint32_t id;
		const char *name;
	};
	static const struct resource_metric_name metric_names[] = {
		{1, "ram_total_bytes"},
		{2, "runtime_static_bytes"},
		{3, "vm_sqbc_chunk_bytes"},
		{4, "heap_count"},
		{5, "heap_free_bytes"},
		{6, "heap_alloc_bytes"},
		{7, "heap_max_alloc_bytes"},
		{8, "heap_largest_free_supported"},
		{9, "heap_largest_free_bytes"},
		{10, "last_dispatch_us"},
		{11, "last_dispatch_seq"},
		{12, "last_sqbc_reads"},
		{13, "last_sqbc_bytes"},
		{14, "runtime_status"},
		{15, "runtime_dispatch_started"},
		{16, "runtime_dispatch_age_us"},
		{17, "runtime_work_submitted"},
		{18, "runtime_current_app_present"},
		{19, "runtime_lifecycle_phase"},
		{20, "runtime_arm_phase"},
		{21, "cap.static.timer"},
		{22, "cap.static.armed_timer"},
		{23, "cap.static.input_button"},
		{24, "cap.static.binding"},
		{25, "cap.static.output"},
		{26, "cap.static.drawlog"},
		{27, "cap.static.device_error"},
		{28, "cap.active.timer"},
		{29, "cap.active.armed_timer"},
		{30, "cap.active.input_button"},
		{31, "cap.active.binding"},
		{32, "cap.active.output"},
		{33, "cap.active.drawlog"},
		{34, "proto_stack_size_bytes"},
		{35, "proto_stack_pre_unused_bytes"},
		{36, "proto_stack_pre_used_bytes"},
		{37, "proto_stack_unused_bytes"},
		{38, "proto_stack_used_bytes"},
		{39, "vm_stack_size_bytes"},
		{40, "vm_stack_unused_bytes"},
		{41, "vm_stack_used_bytes"},
		{42, "app_count"},
		{43, "input_button_state"},
	};
	size_t offset = 0;
	struct sq_protocol_field entry;
	uint32_t expected_id = 0;

	for (size_t i = 0; i < ARRAY_SIZE(metric_names); i++) {
		if (strcmp(metric_names[i].name, key) == 0) {
			expected_id = metric_names[i].id;
			break;
		}
	}

	while (sq_protocol_next_field(frame->payload, frame->payload_len, &offset, &entry) ==
	       SQ_PROTOCOL_OK) {
		size_t record_offset = 0;
		struct sq_protocol_field field;
		const char *record_key = NULL;
		size_t record_key_len = 0;
		uint32_t record_id = 0;
		uint64_t record_value = 0;
		bool has_value = false;

		if (entry.tag != SQ_DEVICE_RECORD_FIELD_ENTRY || entry.type != SQ_FIELD_RECORD) {
			continue;
		}

		while (sq_protocol_next_field(entry.value, entry.len, &record_offset, &field) ==
		       SQ_PROTOCOL_OK) {
			if (field.tag == SQ_DEVICE_RECORD_FIELD_KEY && field.type == SQ_FIELD_STRING) {
				record_key = (const char *)field.value;
				record_key_len = field.len;
			} else if (field.tag == SQ_DEVICE_RECORD_FIELD_KEY &&
				   field.type == SQ_FIELD_U32 && field.len == 4) {
				record_id = sq_protocol_read_u32_le(field.value);
			} else if (field.tag == SQ_DEVICE_RECORD_FIELD_VALUE) {
				if (field.type == SQ_FIELD_U64 && field.len == 8) {
					record_value = sq_protocol_read_u64_le(field.value);
					has_value = true;
				} else if (field.type == SQ_FIELD_U32 && field.len == 4) {
					record_value = sq_protocol_read_u32_le(field.value);
					has_value = true;
				}
			}
		}

		if (record_key != NULL && has_value && strlen(key) == record_key_len &&
		    memcmp(record_key, key, record_key_len) == 0) {
			*out = record_value;
			return true;
		}
		if (expected_id != 0 && record_id == expected_id && has_value) {
			*out = record_value;
			return true;
		}
	}

	return false;
}

static bool resource_value_equals(const struct sq_protocol_frame *frame, const char *key,
				  uint64_t expected)
{
	uint64_t actual = 0;

	return resource_value_for_key(frame, key, &actual) && actual == expected;
}

static int mount_test_fs(void)
{
	int result = fs_mount(&test_fs_mount);

	return result == -EALREADY ? 0 : result;
}

static int format_test_app_store(void)
{
	return sq_app_store_format_filesystem(test_fs_mount.mnt_point);
}

ZTEST(squidscript_protocol, test_decodes_rust_golden_hello_frame)
{
	struct sq_protocol_frame frame;
	struct sq_protocol_field field;
	size_t offset = 0;

	zassert_equal(sq_protocol_decode_frame(hello_frame, sizeof(hello_frame), &frame), 0);
	zassert_equal(frame.kind, SQ_FRAME_REQUEST);
	zassert_equal(frame.opcode, SQ_OPCODE_HELLO);
	zassert_equal(frame.status, SQ_STATUS_OK);
	zassert_equal(frame.sequence, 7);
	zassert_equal(frame.payload_len, 38);
	zassert_equal(frame.payload_crc, 0x5c05a543);

	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field), 0);
	zassert_equal(field.tag, 1);
	zassert_equal(field.type, SQ_FIELD_STRING);
	zassert_equal(field.len, 17);
	zassert_mem_equal(field.value, "esp32c3-supermini", 17);

	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field), 0);
	zassert_equal(field.tag, 2);
	zassert_equal(field.type, SQ_FIELD_BOOL);
	zassert_equal(field.len, 1);
	zassert_equal(field.value[0], 1);

	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field), 0);
	zassert_equal(field.tag, 3);
	zassert_equal(field.type, SQ_FIELD_U64);
	zassert_equal(field.len, 8);
	zassert_equal(sq_protocol_read_u64_le(field.value), 4096);

	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_DONE);
}

ZTEST(squidscript_protocol, test_rejects_payload_crc_mismatch)
{
	uint8_t corrupted[sizeof(hello_frame)];
	struct sq_protocol_frame frame;

	memcpy(corrupted, hello_frame, sizeof(corrupted));
	corrupted[sizeof(corrupted) - 1] ^= 0xff;

	zassert_equal(sq_protocol_decode_frame(corrupted, sizeof(corrupted), &frame),
		      SQ_PROTOCOL_ERR_PAYLOAD_CRC);
}

ZTEST(squidscript_protocol, test_encodes_header_for_existing_payload)
{
	const uint8_t payload[] = {
		0x01, 0x05, 0x08, 0x00, 0x00, 0x40, 0x06, 0x00,
		0x00, 0x00, 0x00, 0x00,
	};
	uint8_t encoded[SQ_PROTOCOL_HEADER_LEN + sizeof(payload)];
	struct sq_protocol_frame frame;

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_RESPONSE, SQ_OPCODE_RESOURCES_GET,
						      SQ_STATUS_OK, 12, payload, sizeof(payload),
						      encoded, sizeof(encoded)), 0);
	memcpy(encoded + SQ_PROTOCOL_HEADER_LEN, payload, sizeof(payload));

	zassert_equal(sq_protocol_decode_frame(encoded, sizeof(encoded), &frame), 0);
	zassert_equal(frame.kind, SQ_FRAME_RESPONSE);
	zassert_equal(frame.opcode, SQ_OPCODE_RESOURCES_GET);
	zassert_equal(frame.sequence, 12);
}

ZTEST(squidscript_protocol, test_handles_hello_request_with_identity_response)
{
	uint8_t response[512];
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct sq_protocol_field field;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_device_protocol_context context = {
		.identity = &identity,
	};
	size_t offset = 0;

	zassert_equal(sq_device_protocol_handle_frame(hello_frame, sizeof(hello_frame), &context,
						      response, sizeof(response), &response_len), 0);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), 0);
	zassert_equal(frame.kind, SQ_FRAME_RESPONSE);
	zassert_equal(frame.opcode, SQ_OPCODE_HELLO);
	zassert_equal(frame.status, SQ_STATUS_OK);
	zassert_equal(frame.sequence, 7);

	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field), 0);
	zassert_equal(field.tag, SQ_DEVICE_FIELD_TARGET);
	zassert_equal(field.type, SQ_FIELD_STRING);
	zassert_mem_equal(field.value, "esp32c3-supermini", 17);

	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field), 0);
	zassert_equal(field.tag, SQ_DEVICE_FIELD_FIRMWARE);
	zassert_equal(field.type, SQ_FIELD_STRING);
	zassert_mem_equal(field.value, "squidscript-zephyr", 18);

	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field), 0);
	zassert_equal(field.tag, SQ_DEVICE_FIELD_DIAGNOSTIC);
	zassert_equal(field.type, SQ_FIELD_BOOL);
	zassert_equal(field.value[0], 1);
}

ZTEST(squidscript_protocol, test_handles_app_list_request_with_registry_records)
{
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_app_registry registry = {
		.count = 2,
		.apps = {
			{.app_id = "alpha", .sqbc_len = 5},
			{.app_id = "beta", .sqbc_len = 6},
		},
	};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
	};
	uint8_t request[SQ_PROTOCOL_HEADER_LEN];
	uint8_t response[512];
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct sq_protocol_field app_record;
	struct sq_protocol_field app_field;
	size_t offset = 0;
	size_t record_offset = 0;

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_APP_LIST,
						      SQ_STATUS_OK, 22, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);

	zassert_equal(sq_device_protocol_handle_frame(request, sizeof(request), &context, response,
						      sizeof(response), &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.kind, SQ_FRAME_RESPONSE);
	zassert_equal(frame.opcode, SQ_OPCODE_APP_LIST);
	zassert_equal(frame.sequence, 22);
	zassert_equal(frame.status, SQ_STATUS_OK);

	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset,
					     &app_record),
		      SQ_PROTOCOL_OK);
	zassert_equal(app_record.tag, 1);
	zassert_equal(app_record.type, SQ_FIELD_RECORD);
	zassert_equal(sq_protocol_next_field(app_record.value, app_record.len, &record_offset,
					     &app_field),
		      SQ_PROTOCOL_OK);
	zassert_equal(app_field.tag, 1);
	zassert_equal(app_field.type, SQ_FIELD_STRING);
	zassert_mem_equal(app_field.value, "alpha", 5);
	zassert_equal(sq_protocol_next_field(app_record.value, app_record.len, &record_offset,
					     &app_field),
		      SQ_PROTOCOL_OK);
	zassert_equal(app_field.tag, 2);
	zassert_equal(app_field.type, SQ_FIELD_U64);
	zassert_equal(sq_protocol_read_u64_le(app_field.value), 5);
}

ZTEST(squidscript_protocol, test_handles_output_get_with_empty_framed_response)
{
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_device_protocol_context context = {
		.identity = &identity,
	};
	uint8_t request[SQ_PROTOCOL_HEADER_LEN];
	uint8_t response[64];
	size_t response_len = 0;
	struct sq_protocol_frame frame;

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_OUTPUT_GET,
						      SQ_STATUS_OK, 24, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, sizeof(request), &context,
						      response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.kind, SQ_FRAME_RESPONSE);
	zassert_equal(frame.opcode, SQ_OPCODE_OUTPUT_GET);
	zassert_equal(frame.status, SQ_STATUS_OK);
	zassert_equal(frame.sequence, 24);
	zassert_equal(frame.payload_len, 0);
}

ZTEST(squidscript_protocol, test_handles_trace_resources_and_wifi_error_frames)
{
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {
		.traces = {"app.start", "state.save"},
		.trace_count = 2,
		.drawlog = {"draw=clear color=gray0", "draw=text text=\"Hello\" x=10 y=20",
			    "draw=rect x=1 y=2 w=3 h=4", "draw=line x1=5 y1=6 x2=7 y2=8"},
		.drawlog_count = 4,
	};
	struct sq_app_registry registry = {.count = 1};
	struct sq_device_install_session install_session = {0};
	struct sq_device_temp_session temp_session = {0};
	struct sq_device_resource_session resource_session = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
		.install_session = &install_session,
		.temp_session = &temp_session,
		.resource_session = &resource_session,
		.runtime = &runtime,
	};
	uint8_t request[SQ_PROTOCOL_HEADER_LEN];
	uint8_t response[512];
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct sq_protocol_field field;
	size_t offset = 0;

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_TRACE_GET,
						      SQ_STATUS_OK, 61, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, sizeof(request), &context,
						      response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_TRACE_GET);
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_equal(field.tag, SQ_DEVICE_LINE_FIELD_VALUE);
	zassert_mem_equal(field.value, "app.start", 9);

	offset = 0;
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_DRAWLOG_GET,
						      SQ_STATUS_OK, 64, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, sizeof(request), &context,
						      response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_DRAWLOG_GET);
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_equal(field.tag, SQ_DEVICE_LINE_FIELD_VALUE);
	zassert_mem_equal(field.value, "draw=clear color=gray0", strlen("draw=clear color=gray0"));
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_mem_equal(field.value, "draw=text text=\"Hello\" x=10 y=20",
			  strlen("draw=text text=\"Hello\" x=10 y=20"));
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_mem_equal(field.value, "draw=rect x=1 y=2 w=3 h=4",
			  strlen("draw=rect x=1 y=2 w=3 h=4"));
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_mem_equal(field.value, "draw=line x1=5 y1=6 x2=7 y2=8",
			  strlen("draw=line x1=5 y1=6 x2=7 y2=8"));

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_RESOURCES_GET,
						      SQ_STATUS_OK, 62, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, sizeof(request), &context,
						      response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_RESOURCES_GET);
	zassert_true(frame.payload_len > 0);

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_WIFI_PROFILE_SET,
						      SQ_STATUS_OK, 63, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, sizeof(request), &context,
						      response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_WIFI_PROFILE_SET);
	zassert_equal(frame.status, SQ_STATUS_ERROR);
}

ZTEST(squidscript_protocol, test_errors_get_reports_vm_status_label_and_errno)
{
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {
		.status = SQ_VM_RUNTIME_ERROR,
		.result_code = -EINVAL,
		.result = {
			.status = SQVM_STATUS_INVALID_ARGUMENT,
		},
	};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.runtime = &runtime,
	};
	uint8_t request[SQ_PROTOCOL_HEADER_LEN];
	uint8_t response[512];
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct sq_protocol_field field;
	size_t offset = 0;

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_ERRORS_GET,
						      SQ_STATUS_OK, 65, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, sizeof(request), &context,
						      response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_ERRORS_GET);
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	/* The numeric code is paired with its errno name so the diagnostic is legible
	 * without an external lookup. */
	zassert_true(field_string_equals(&field, "runtime=invalid_argument code=-22 (EINVAL)"));
}

ZTEST(squidscript_protocol, test_errors_get_reports_retained_device_diagnostics_without_runtime_error)
{
	struct sq_device_identity identity = {
		.target = "xiao-esp32c3-gdeq0426t82-sd",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.runtime = &runtime,
	};
	uint8_t request[SQ_PROTOCOL_HEADER_LEN];
	uint8_t response[512];
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct sq_protocol_field field;
	size_t offset = 0;

	sq_vm_runtime_record_device_error(&runtime, "display=unavailable code=-19");

	zassert_equal(runtime.status, SQ_VM_RUNTIME_IDLE);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_ERRORS_GET,
						      SQ_STATUS_OK, 66, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, sizeof(request), &context,
						      response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_ERRORS_GET);
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_true(field_string_equals(&field, "display=unavailable code=-19"));
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_DONE);
}

ZTEST(squidscript_protocol, test_errors_get_truncates_retained_device_diagnostics_to_response_cap)
{
	struct sq_device_identity identity = {
		.target = "xiao-esp32c3-gdeq0426t82-sd",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.runtime = &runtime,
	};
	uint8_t request[SQ_PROTOCOL_HEADER_LEN];
	uint8_t response[192];
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct sq_protocol_field field;
	size_t offset = 0;
	bool saw_truncation = false;
	bool saw_newest_error = false;

	for (size_t i = 0; i < SQ_VM_RUNTIME_DEVICE_ERROR_MAX; i++) {
		char line[SQ_VM_RUNTIME_DEVICE_ERROR_LEN + 8];

		snprintk(line, sizeof(line), "device-error-index-%02zu detail-abcdefghijklmnop", i);
		zassert_equal(sq_vm_runtime_record_device_error(&runtime, line), 0);
	}

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_ERRORS_GET,
						      SQ_STATUS_OK, 67, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, sizeof(request), &context,
						      response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_true(response_len <= sizeof(response));
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_ERRORS_GET);
	zassert_equal(frame.status, SQ_STATUS_OK);

	while (sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field) ==
	       SQ_PROTOCOL_OK) {
		if (field_string_equals(&field, "errors_truncated=5")) {
			saw_truncation = true;
		}
		if (field.len >= strlen("device-error-index-07") &&
		    memcmp(field.value, "device-error-index-07", strlen("device-error-index-07")) ==
			    0) {
			saw_newest_error = true;
		}
	}
	zassert_true(saw_truncation);
	zassert_true(saw_newest_error);
}

ZTEST(squidscript_protocol, test_wifi_profile_set_stores_volatile_profile_without_echoing_secret)
{
	uint8_t payload[96];
	uint8_t request[SQ_PROTOCOL_HEADER_LEN + sizeof(payload)];
	uint8_t response[128];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct sq_vm_runtime runtime = {0};
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.runtime = &runtime,
	};

	sq_vm_runtime_init(&runtime);
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "dev"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 2,
						      "ExampleSSID"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 3,
						      "secret-pass"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_WIFI_PROFILE_SET,
						      SQ_STATUS_OK, 76, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);

	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_WIFI_PROFILE_SET);
	zassert_equal(frame.status, SQ_STATUS_OK);
	zassert_equal(frame.payload_len, 0);
	zassert_equal(runtime.wifi_profile_len, 3);
	zassert_equal(runtime.wifi_profile_ssid_len, 11);
	zassert_equal(runtime.wifi_profile_password_len, 11);
	zassert_mem_equal(runtime.wifi_profile, "dev", 3);
	zassert_mem_equal(runtime.wifi_profile_ssid, "ExampleSSID", 11);
	zassert_mem_equal(runtime.wifi_profile_password, "secret-pass", 11);
}

ZTEST(squidscript_protocol, test_storage_format_clears_runtime_before_erasing_files)
{
	const uint8_t sqbc[] = {'s', 'q', 'b', 'c'};
	uint8_t request[SQ_PROTOCOL_HEADER_LEN];
	uint8_t response[128];
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct fs_dirent entry;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {
		.status = SQ_VM_RUNTIME_COMPLETE,
		.trace_count = 1,
		.traces = {"state.save"},
	};
	struct sq_app_registry registry = {.count = 1};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_install_session install_session = {.active = true};
	struct sq_device_temp_session temp_session = {.active = true};
	struct sq_device_resource_session resource_session = {.active = true};
	struct sq_device_protocol_scratch scratch = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
		.mutable_registry = &registry,
		.install_session = &install_session,
		.temp_session = &temp_session,
		.resource_session = &resource_session,
		.scratch = &scratch,
		.runtime = &runtime,
		.store_mount_point = test_fs_mount.mnt_point,
		.launch_storage = &launch_storage,
	};
	int handle_result;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "main", sqbc, sizeof(sqbc)),
		      0);
	zassert_equal(sq_app_store_install_resource(test_fs_mount.mnt_point, "main",
						    "icons/main.bin", sqbc, sizeof(sqbc)),
		      0);
	zassert_equal(write_test_file("/sqtest/state/main.state", sqbc, sizeof(sqbc)), 0);
	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point, "main",
						     &launch_storage),
		      0);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_STORAGE_FORMAT,
						      SQ_STATUS_OK, 64, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);

	handle_result = sq_device_protocol_handle_frame(request, sizeof(request), &context, response,
						       sizeof(response), &response_len);
	zassert_equal(handle_result, SQ_PROTOCOL_OK, "handle result %d", handle_result);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_STORAGE_FORMAT);
	zassert_equal(frame.status, SQ_STATUS_PENDING);
	zassert_true(frame.payload_len > 0);
	zassert_equal(registry.count, 0);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_IDLE);
	zassert_false(install_session.active);
	zassert_false(temp_session.active);
	zassert_false(resource_session.active);
	zassert_equal(launch_storage.sqbc_path[0], '\0');

	for (size_t i = 0; i < 32 && frame.status == SQ_STATUS_PENDING; i++) {
		handle_result = sq_device_protocol_handle_frame(request, sizeof(request), &context,
							       response, sizeof(response),
							       &response_len);
		zassert_equal(handle_result, SQ_PROTOCOL_OK, "handle result %d", handle_result);
		zassert_equal(sq_protocol_decode_frame(response, response_len, &frame),
			      SQ_PROTOCOL_OK);
		zassert_equal(frame.opcode, SQ_OPCODE_STORAGE_FORMAT);
	}
	zassert_equal(frame.status, SQ_STATUS_OK);
	zassert_equal(frame.payload_len, 0);
	zassert_equal(fs_stat("/sqtest/apps/main/main.sqbc", &entry), -ENOENT);
	zassert_equal(fs_stat("/sqtest/apps/main", &entry), -ENOENT);
	zassert_equal(fs_stat("/sqtest/state/main.state", &entry), -ENOENT);
	zassert_equal(fs_stat("/sqtest/apps", &entry), 0);
	zassert_equal(fs_stat("/sqtest/state", &entry), 0);
	zassert_equal(fs_stat("/sqtest/tmp", &entry), 0);
	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_storage_format_rejects_while_runtime_worker_is_running)
{
	uint8_t request[SQ_PROTOCOL_HEADER_LEN];
	uint8_t response[128];
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {
		.status = SQ_VM_RUNTIME_RUNNING,
		.trace_count = 1,
		.traces = {"still-running"},
	};
	struct sq_app_registry registry = {.count = 1};
	struct sq_app_store_vm_storage launch_storage = {
		.sqbc_path = "/sqtest/apps/main/main.sqbc",
	};
	struct sq_device_install_session install_session = {.active = true};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
		.mutable_registry = &registry,
		.install_session = &install_session,
		.runtime = &runtime,
		.store_mount_point = test_fs_mount.mnt_point,
		.launch_storage = &launch_storage,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_STORAGE_FORMAT,
						      SQ_STATUS_OK, 65, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);

	zassert_equal(sq_device_protocol_handle_frame(request, sizeof(request), &context, response,
						      sizeof(response), &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_STORAGE_FORMAT);
	zassert_equal(frame.status, SQ_STATUS_ERROR);
	zassert_equal(registry.count, 1);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_RUNNING);
	zassert_equal(runtime.trace_count, 1);
	zassert_str_equal(runtime.traces[0], "still-running");
	zassert_true(install_session.active);
	zassert_str_equal(launch_storage.sqbc_path, "/sqtest/apps/main/main.sqbc");

	runtime.status = SQ_VM_RUNTIME_COMPLETE;
	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_handles_installed_app_begin_chunk_commit)
{
	const uint8_t chunk_a[] = {'h', 'e', 'l'};
	const uint8_t chunk_b[] = {'l', 'o'};
	uint8_t begin_payload[64];
	uint8_t chunk_payload[32];
	uint8_t request[128];
	uint8_t response[128];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_app_registry registry = {0};
	struct sq_device_install_session install_session = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
		.install_session = &install_session,
		.store_mount_point = test_fs_mount.mnt_point,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	zassert_true(sizeof(install_session) < 512,
		     "installed app write session must not reserve full SQBC payload RAM");

	zassert_equal(sq_protocol_append_string_field(begin_payload, sizeof(begin_payload),
						     &payload_len, 1, "framed-app"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_u64_field(begin_payload, sizeof(begin_payload),
						  &payload_len, 2, 5),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_u64_field(begin_payload, sizeof(begin_payload),
						  &payload_len, 3,
						  sq_protocol_crc32((const uint8_t *)"hello", 5)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST,
						      SQ_OPCODE_APP_INSTALL_BEGIN,
						      SQ_STATUS_OK, 30, begin_payload,
						      payload_len, request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], begin_payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);

	payload_len = 0;
	zassert_equal(sq_protocol_append_u64_field(chunk_payload, sizeof(chunk_payload),
						  &payload_len, 1, 0),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_bytes_field(chunk_payload, sizeof(chunk_payload),
						    &payload_len, 2, chunk_a,
						    sizeof(chunk_a)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST,
						      SQ_OPCODE_APP_INSTALL_CHUNK,
						      SQ_STATUS_OK, 31, chunk_payload,
						      payload_len, request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], chunk_payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);

	payload_len = 0;
	zassert_equal(sq_protocol_append_u64_field(chunk_payload, sizeof(chunk_payload),
						  &payload_len, 1, 3),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_bytes_field(chunk_payload, sizeof(chunk_payload),
						    &payload_len, 2, chunk_b,
						    sizeof(chunk_b)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST,
						      SQ_OPCODE_APP_INSTALL_CHUNK,
						      SQ_STATUS_OK, 32, chunk_payload,
						      payload_len, request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], chunk_payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST,
						      SQ_OPCODE_APP_INSTALL_COMMIT,
						      SQ_STATUS_OK, 33, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN, &context,
						      response, sizeof(response), &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_app_store_scan_registry(test_fs_mount.mnt_point, &registry), 0);
	zassert_not_null(sq_app_registry_find(&registry, "framed-app"));

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_handles_app_launch_dispatches_installed_app_start)
{
	uint8_t payload[32];
	uint8_t request[64];
	uint8_t response[128];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_registry registry = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_app_store_vm_storage trigger_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
		.trigger_storage = &trigger_storage,
		.fallback_app = &sq_zephyr_fallback_app,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "launch-app",
					       headless_counter_sqbc,
					       sizeof(headless_counter_sqbc)),
		      0);
	zassert_equal(sq_app_store_scan_registry(test_fs_mount.mnt_point, &registry), 0);
	start_test_root(&context, &runtime);
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "launch-app"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_APP_LAUNCH,
						      SQ_STATUS_OK, 40, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);

	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_true(response_len >= SQ_PROTOCOL_HEADER_LEN);
	wait_runtime_done(&runtime);
	zassert_equal(poll_until_current_app(&context, &runtime, "launch-app"), 0);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);
	zassert_equal(runtime.result_code, 0);
	zassert_str_equal(runtime.current_app, "launch-app");
	zassert_equal(runtime.return_stack_count, 1);
	zassert_str_equal(runtime.return_stack[0], "main");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_app_launch_reports_unsupported_inline_gpio_binding)
{
	uint8_t payload[48];
	uint8_t request[96];
	uint8_t response[256];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_registry registry = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
		.fallback_app = &sq_zephyr_fallback_app,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point,
					       "unsupported-inline-gpio-binding",
					       unsupported_inline_gpio_binding_sqbc,
					       sizeof(unsupported_inline_gpio_binding_sqbc)),
		      0);
	zassert_equal(sq_app_store_scan_registry(test_fs_mount.mnt_point, &registry), 0);
	sq_vm_runtime_set_store_mount_point(&runtime, test_fs_mount.mnt_point);
	start_test_root(&context, &runtime);
	zassert_str_equal(runtime.current_app, "main");

	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "unsupported-inline-gpio-binding"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_APP_LAUNCH,
						      SQ_STATUS_OK, 41, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);

	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_APP_LAUNCH);
	zassert_true(frame.status == SQ_STATUS_OK,
		     "frame.status=%u runtime.status=%d result=%d current=%s stack=%u output=%u",
		     frame.status, runtime.status, runtime.result_code, runtime.current_app,
		     runtime.return_stack_count, runtime.output_count);
	zassert_equal(poll_until_current_app(&context, &runtime, "unsupported-inline-gpio-binding"),
		      0);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_ERROR);
	zassert_equal(runtime.result_code, -ENOTSUP);
	zassert_str_equal(runtime.current_app, "unsupported-inline-gpio-binding");
	zassert_equal(runtime.output_count, 0);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_launch_root_uses_fallback_main_when_installed_main_is_absent)
{
	struct sq_vm_runtime runtime = {0};
	struct sq_app_registry registry = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.registry = &registry,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
		.fallback_app = &sq_zephyr_fallback_app,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	zassert_equal(sq_app_store_scan_registry(test_fs_mount.mnt_point, &registry), 0);

	zassert_equal(sq_device_protocol_start_root(&context), 0);
	wait_runtime_done(&runtime);
	zassert_str_equal(runtime.current_app, "main");
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "fallback app launched: no foreground app");
	zassert_is_null(sq_app_registry_find(&registry, "main"));

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_launch_root_prefers_installed_main_over_fallback)
{
	struct sq_vm_runtime runtime = {0};
	struct sq_app_registry registry = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.registry = &registry,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
		.fallback_app = &sq_zephyr_fallback_app,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "main", reader_exit_sqbc,
					       sizeof(reader_exit_sqbc)),
		      0);
	zassert_equal(sq_app_store_scan_registry(test_fs_mount.mnt_point, &registry), 0);

	zassert_equal(sq_device_protocol_start_root(&context), 0);
	wait_runtime_done(&runtime);
	zassert_str_equal(runtime.current_app, "main");
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "reader start");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_lifecycle_state_machine_host_launch_from_empty_pushes_root)
{
	struct sq_vm_runtime runtime = {0};
	struct sq_app_lifecycle_step step = {0};

	zassert_equal(sq_app_lifecycle_request_launch(&runtime, (const uint8_t *)"reader",
						      sizeof("reader") - 1),
		      0);
	zassert_true(sq_vm_runtime_lifecycle_busy(&runtime));

	zassert_equal(sq_app_lifecycle_next_step(&runtime, NULL, NULL, &step), 0);
	zassert_equal(step.kind, SQ_APP_LIFECYCLE_STEP_START_APP);
	zassert_str_equal(step.app_id, "reader");
	zassert_str_equal(step.event, "app.start");
	zassert_true(step.set_current);
	zassert_equal(runtime.return_stack_count, 1);
	zassert_str_equal(runtime.return_stack[0], "main");
	zassert_str_equal(runtime.start_reason, "launch");
	zassert_false(sq_vm_runtime_lifecycle_busy(&runtime));
}

ZTEST(squidscript_protocol, test_lifecycle_state_machine_host_launch_from_current_dispatches_exit)
{
	struct sq_vm_runtime runtime = {0};
	struct sq_app_lifecycle_step step = {0};

	strncpy(runtime.current_app, "reader", sizeof(runtime.current_app) - 1);
	zassert_equal(sq_app_lifecycle_request_launch(&runtime, (const uint8_t *)"settings",
						      sizeof("settings") - 1),
		      0);

	zassert_equal(sq_app_lifecycle_next_step(&runtime, NULL, NULL, &step), 0);
	zassert_equal(step.kind, SQ_APP_LIFECYCLE_STEP_START_APP);
	zassert_str_equal(step.app_id, "reader");
	zassert_str_equal(step.event, "app.exit");
	zassert_false(step.set_current);
	zassert_equal(runtime.lifecycle_phase, SQ_VM_RUNTIME_LIFECYCLE_EXIT_FOR_LAUNCH);
	zassert_equal(runtime.return_stack_count, 0);

	memset(&step, 0, sizeof(step));
	zassert_equal(sq_app_lifecycle_next_step(&runtime, NULL, NULL, &step), 0);
	zassert_equal(step.kind, SQ_APP_LIFECYCLE_STEP_START_APP);
	zassert_str_equal(step.app_id, "settings");
	zassert_str_equal(step.event, "app.start");
	zassert_true(step.set_current);
	zassert_equal(runtime.return_stack_count, 1);
	zassert_str_equal(runtime.return_stack[0], "reader");
	zassert_str_equal(runtime.start_reason, "launch");
	zassert_false(sq_vm_runtime_lifecycle_busy(&runtime));
}

ZTEST(squidscript_protocol, test_lifecycle_state_machine_dispatch_error_preserves_exit_handoff)
{
	struct sq_vm_runtime runtime = {0};
	struct sq_app_lifecycle_step step = {0};

	strncpy(runtime.current_app, "reader", sizeof(runtime.current_app) - 1);
	zassert_equal(sq_app_lifecycle_request_launch(&runtime, (const uint8_t *)"settings",
						      sizeof("settings") - 1),
		      0);
	zassert_equal(sq_app_lifecycle_next_step(&runtime, NULL, NULL, &step), 0);
	zassert_equal(runtime.lifecycle_phase, SQ_VM_RUNTIME_LIFECYCLE_EXIT_FOR_LAUNCH);

	sq_app_lifecycle_cancel_pending_after_dispatch_error(&runtime, -EIO);
	zassert_equal(runtime.lifecycle_phase, SQ_VM_RUNTIME_LIFECYCLE_EXIT_FOR_LAUNCH);
	zassert_str_equal(runtime.lifecycle_target_app, "settings");
}

ZTEST(squidscript_protocol, test_lifecycle_state_machine_due_event_for_current_app_does_not_relaunch)
{
	struct sq_vm_runtime runtime = {0};
	struct sq_app_lifecycle_step step = {0};

	strncpy(runtime.current_app, "ble-install", sizeof(runtime.current_app) - 1);
	strncpy(runtime.start_reason, "boot", sizeof(runtime.start_reason) - 1);

	zassert_equal(sq_app_lifecycle_next_step(&runtime, "ble-install",
						 "ble.file.complete", &step),
		      0);
	zassert_equal(step.kind, SQ_APP_LIFECYCLE_STEP_START_APP);
	zassert_str_equal(step.app_id, "ble-install");
	zassert_str_equal(step.event, "ble.file.complete");
	zassert_false(step.set_current);
	zassert_equal(runtime.return_stack_count, 0);
	zassert_str_equal(runtime.start_reason, "boot");
	zassert_false(sq_vm_runtime_lifecycle_busy(&runtime));
}

ZTEST(squidscript_protocol, test_host_app_launch_uses_lifecycle_chain_from_fallback_root)
{
	uint8_t payload[32];
	uint8_t request[64];
	uint8_t response[512];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_registry registry = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_app_store_vm_storage trigger_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
		.trigger_storage = &trigger_storage,
		.fallback_app = &sq_zephyr_fallback_app,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "reader", reader_exit_sqbc,
					       sizeof(reader_exit_sqbc)),
		      0);
	zassert_equal(sq_app_store_scan_registry(test_fs_mount.mnt_point, &registry), 0);

	zassert_equal(sq_device_protocol_start_root(&context), 0);
	wait_runtime_done(&runtime);
	zassert_str_equal(runtime.current_app, "main");
	zassert_equal(runtime.return_stack_count, 0);
	clear_runtime_lines(&runtime);

	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "reader"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_APP_LAUNCH,
						      SQ_STATUS_OK, 140, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	wait_runtime_done(&runtime);
	zassert_equal(poll_until_current_app(&context, &runtime, "reader"), 0);
	zassert_str_equal(runtime.current_app, "reader");
	zassert_equal(runtime.return_stack_count, 1);
	zassert_str_equal(runtime.return_stack[0], "main");
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "reader start");
	zassert_true(runtime.trace_count >= 1);
	zassert_str_equal(runtime.traces[0], "app.exit");

	runtime.dispatch_exited = true;
	runtime.status = SQ_VM_RUNTIME_COMPLETE;
	zassert_equal(poll_until_current_app(&context, &runtime, "main"), 0);
	zassert_str_equal(runtime.current_app, "main");
	zassert_equal(runtime.return_stack_count, 0);
	zassert_str_equal(runtime.outputs[runtime.output_count - 1],
			  "fallback app launched: no foreground app");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_host_app_launch_without_current_app_pushes_logical_root)
{
	uint8_t payload[32];
	uint8_t request[64];
	uint8_t response[512];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_registry registry = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_app_store_vm_storage trigger_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
		.trigger_storage = &trigger_storage,
		.fallback_app = &sq_zephyr_fallback_app,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "reader", reader_exit_sqbc,
					       sizeof(reader_exit_sqbc)),
		      0);
	zassert_equal(sq_app_store_scan_registry(test_fs_mount.mnt_point, &registry), 0);

	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "reader"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_APP_LAUNCH,
						      SQ_STATUS_OK, 141, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_true(sq_vm_runtime_lifecycle_busy(&runtime));
	zassert_equal(poll_until_current_app(&context, &runtime, "reader"), 0);
	zassert_str_equal(runtime.current_app, "reader");
	zassert_equal(runtime.return_stack_count, 1);
	zassert_str_equal(runtime.return_stack[0], "main");
	zassert_str_equal(runtime.outputs[runtime.output_count - 1], "reader start");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_host_app_launch_without_current_app_starts_stateful_app)
{
	uint8_t payload[32];
	uint8_t request[64];
	uint8_t response[512];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_registry registry = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
		.fallback_app = &sq_zephyr_fallback_app,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "state-counter",
					       state_counter_sqbc, sizeof(state_counter_sqbc)),
		      0);
	zassert_equal(sq_app_store_scan_registry(test_fs_mount.mnt_point, &registry), 0);

	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "state-counter"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_APP_LAUNCH,
						      SQ_STATUS_OK, 142, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);

	zassert_true(sq_vm_runtime_lifecycle_busy(&runtime));
	zassert_equal(poll_until_current_app(&context, &runtime, "state-counter"), 0);
	zassert_false(sq_vm_runtime_lifecycle_busy(&runtime));
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);
	zassert_equal(runtime.result_code, 0);
	zassert_str_equal(runtime.current_app, "state-counter");
	zassert_equal(runtime.return_stack_count, 1);
	zassert_str_equal(runtime.return_stack[0], "main");
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "count 0");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_host_app_launch_noop_from_fallback_stays_protocol_responsive)
{
	uint8_t payload[32];
	uint8_t request[64];
	uint8_t response[512];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct sq_protocol_field field;
	size_t offset = 0;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_registry registry = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
		.fallback_app = &sq_zephyr_fallback_app,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "noop", noop_sqbc,
					       sizeof(noop_sqbc)),
		      0);
	zassert_equal(sq_app_store_scan_registry(test_fs_mount.mnt_point, &registry), 0);
	zassert_equal(sq_device_protocol_start_root(&context), 0);

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_OUTPUT_GET,
						      SQ_STATUS_OK, 142, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN, &context,
						      response, sizeof(response), &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_LIFECYCLE_GET,
						      SQ_STATUS_OK, 143, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN, &context,
						      response, sizeof(response), &response_len),
		      SQ_PROTOCOL_OK);
	zassert_str_equal(runtime.current_app, "main");

	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "noop"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_APP_LAUNCH,
						      SQ_STATUS_OK, 144, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_true(sq_vm_runtime_lifecycle_busy(&runtime));
	zassert_equal(poll_until_current_app(&context, &runtime, "noop"), 0);
	zassert_str_equal(runtime.current_app, "noop");
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_LIFECYCLE_GET,
						      SQ_STATUS_OK, 145, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN, &context,
						      response, sizeof(response), &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_LIFECYCLE_GET);
	zassert_equal(frame.status, SQ_STATUS_OK);
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_true(field_string_equals(&field, "active=noop"));
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_true(field_string_equals(&field, "process_stack[0]=main"));

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_foreground_events_preserve_vm_memory_until_relaunch)
{
	uint8_t payload[96];
	uint8_t request[144];
	uint8_t response[256];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_registry registry = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
		.fallback_app = &sq_zephyr_fallback_app,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "foreground-memory",
					       foreground_memory_sqbc,
					       sizeof(foreground_memory_sqbc)),
		      0);
	zassert_equal(sq_app_store_scan_registry(test_fs_mount.mnt_point, &registry), 0);
	start_test_root(&context, &runtime);

	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "foreground-memory"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_APP_LAUNCH,
						      SQ_STATUS_OK, 440, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	wait_runtime_done(&runtime);
	zassert_equal(poll_until_current_app(&context, &runtime, "foreground-memory"), 0);
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "memory start 1");

	for (uint32_t sequence = 441; sequence <= 442; sequence++) {
		payload_len = 0;
		zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload),
							      &payload_len, 1,
							      "foreground-memory"),
			      SQ_PROTOCOL_OK);
		zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload),
							      &payload_len, 2,
							      "key.SELECT"),
			      SQ_PROTOCOL_OK);
		zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST,
							      SQ_OPCODE_EVENT_DISPATCH,
							      SQ_STATUS_OK, sequence, payload,
							      payload_len, request,
							      sizeof(request)),
			      SQ_PROTOCOL_OK);
		memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
		zassert_equal(sq_device_protocol_handle_frame(
				      request, SQ_PROTOCOL_HEADER_LEN + payload_len, &context,
				      response, sizeof(response), &response_len),
			      SQ_PROTOCOL_OK);
		wait_runtime_done(&runtime);
	}
	zassert_equal(runtime.output_count, 3);
	zassert_str_equal(runtime.outputs[1], "memory select 2");
	zassert_str_equal(runtime.outputs[2], "memory select 3");

	payload_len = 0;
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "foreground-memory"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_APP_LAUNCH,
						      SQ_STATUS_OK, 443, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	wait_runtime_done(&runtime);
	zassert_equal(poll_until_output_count(&context, &runtime, 4), 0);
	zassert_equal(runtime.output_count, 4);
	zassert_str_equal(runtime.outputs[3], "memory start 1");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_event_dispatch_rejects_non_foreground_app_target)
{
	uint8_t payload[80];
	uint8_t request[128];
	uint8_t response[256];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_registry registry = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
		.fallback_app = &sq_zephyr_fallback_app,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "headless-counter",
					       headless_counter_sqbc,
					       sizeof(headless_counter_sqbc)),
		      0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "reader", reader_exit_sqbc,
					       sizeof(reader_exit_sqbc)),
		      0);

	strncpy(runtime.current_app, "headless-counter", sizeof(runtime.current_app) - 1);
	runtime.context_ready = true;
	zassert_str_equal(runtime.current_app, "headless-counter");
	zassert_true(runtime.context_ready);

	payload_len = 0;
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "reader"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 2,
						      "repl"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_EVENT_DISPATCH,
						      SQ_STATUS_OK, 402, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);

	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_EVENT_DISPATCH);
	zassert_equal(frame.status, SQ_STATUS_ERROR);
	zassert_str_equal(runtime.current_app, "headless-counter");
	zassert_true(runtime.context_ready);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_event_dispatch_exposes_lifecycle_trace_records)
{
	uint8_t payload[80];
	uint8_t request[128];
	uint8_t response[512];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct sq_protocol_field field;
	size_t offset = 0;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "lifecycle",
					       lifecycle_trace_sqbc,
					       sizeof(lifecycle_trace_sqbc)),
		      0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "reader", reader_exit_sqbc,
					       sizeof(reader_exit_sqbc)),
		      0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "break-reminder",
					       break_reminder_sqbc, sizeof(break_reminder_sqbc)),
		      0);
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "lifecycle"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 2,
						      "repl"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_EVENT_DISPATCH,
						      SQ_STATUS_OK, 41, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);

	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	wait_runtime_done(&runtime);
	zassert_true(runtime.status == SQ_VM_RUNTIME_COMPLETE,
		     "status=%d result=%d ffi_status=%d outcome=%d storage_kind=%d storage_offset=%u storage_len=%u current=%s traces=%u trace0=%s trace1=%s trace2=%s trace3=%s",
		     runtime.status, runtime.result_code, runtime.result.status, runtime.result.outcome,
		     runtime.result.storage.kind, (unsigned int)runtime.result.storage.offset,
		     (unsigned int)runtime.result.storage.len, runtime.current_app,
		     runtime.trace_count, runtime.traces[0], runtime.traces[1], runtime.traces[2],
		     runtime.traces[3]);
	zassert_equal(runtime.result_code, 0);

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_TRACE_GET,
						      SQ_STATUS_OK, 42, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN, &context,
						      response, sizeof(response), &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_TRACE_GET);
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_mem_equal(field.value, "repl", strlen("repl"));
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_mem_equal(field.value, "app.launch reader", strlen("app.launch reader"));

	k_sleep(K_MSEC(300));
	zassert_equal(sq_vm_runtime_poll(&runtime), 0);
	wait_runtime_done(&runtime);

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_TRACE_GET,
						      SQ_STATUS_OK, 43, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN, &context,
						      response, sizeof(response), &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	offset = 0;
	bool saw_launch = false;
	while (sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field) ==
	       SQ_PROTOCOL_OK) {
		saw_launch = saw_launch || field_string_equals(&field, "app.launch reader");
	}
	zassert_true(saw_launch);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_second_lifecycle_request_in_one_dispatch_is_rejected)
{
	uint8_t payload[80];
	uint8_t request[128];
	uint8_t response[512];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point,
					       "double-lifecycle-request",
					       double_lifecycle_request_sqbc,
					       sizeof(double_lifecycle_request_sqbc)),
		      0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "reader", reader_exit_sqbc,
					       sizeof(reader_exit_sqbc)),
		      0);
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "double-lifecycle-request"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 2,
						      "repl"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_EVENT_DISPATCH,
						      SQ_STATUS_OK, 48, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);

	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	wait_runtime_done(&runtime);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_ERROR,
		      "status=%d result=%d phase=%d target=%s current=%s trace_count=%u trace0=%s trace1=%s trace2=%s trace3=%s",
		      runtime.status, runtime.result_code, runtime.lifecycle_phase,
		      runtime.lifecycle_target_app, runtime.current_app, runtime.trace_count,
		      runtime.traces[0], runtime.traces[1], runtime.traces[2],
		      runtime.traces[3]);
	zassert_not_equal(runtime.result_code, 0,
			  "status=%d result=%d phase=%d target=%s current=%s trace_count=%u trace0=%s trace1=%s trace2=%s trace3=%s",
			  runtime.status, runtime.result_code, runtime.lifecycle_phase,
			  runtime.lifecycle_target_app, runtime.current_app, runtime.trace_count,
			  runtime.traces[0], runtime.traces[1], runtime.traces[2],
			  runtime.traces[3]);
	zassert_false(sq_vm_runtime_lifecycle_busy(&runtime));
	zassert_str_equal(runtime.current_app, "");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_app_launch_and_exit_update_foreground_stack)
{
	uint8_t payload[80];
	uint8_t request[128];
	uint8_t response[512];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_registry registry = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
		.fallback_app = &sq_zephyr_fallback_app,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "lifecycle", lifecycle_sqbc,
					       sizeof(lifecycle_sqbc)),
		      0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "reader", reader_exit_sqbc,
					       sizeof(reader_exit_sqbc)),
		      0);
	zassert_equal(sq_app_store_scan_registry(test_fs_mount.mnt_point, &registry), 0);
	start_test_root(&context, &runtime);

	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "lifecycle"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_APP_LAUNCH,
						      SQ_STATUS_OK, 44, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	wait_runtime_done(&runtime);
	zassert_equal(poll_until_current_app(&context, &runtime, "lifecycle"), 0);
	zassert_true(strcmp(runtime.current_app, "lifecycle") == 0,
		     "current=%s status=%d exited=%d stack=%u output=%u trace_count=%u trace0=%s trace1=%s trace2=%s trace3=%s",
		     runtime.current_app, runtime.status, runtime.dispatch_exited,
		     runtime.return_stack_count, runtime.output_count, runtime.trace_count,
		     runtime.traces[0], runtime.traces[1], runtime.traces[2], runtime.traces[3]);
	zassert_equal(runtime.return_stack_count, 1);
	zassert_str_equal(runtime.return_stack[0], "main");

	payload_len = 0;
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "reader"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_APP_LAUNCH,
						      SQ_STATUS_OK, 45, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	wait_runtime_done(&runtime);
	int poll_result = poll_until_current_app(&context, &runtime, "reader");
	zassert_true(poll_result == 0,
		     "poll_result=%d current=%s status=%d result=%d phase=%d target=%s previous=%s exited=%d stack=%u output=%u trace_count=%u trace0=%s trace1=%s trace2=%s trace3=%s",
		     poll_result, runtime.current_app, runtime.status, runtime.result_code,
		     runtime.lifecycle_phase, runtime.lifecycle_target_app,
		     runtime.lifecycle_previous_app, runtime.dispatch_exited,
		     runtime.return_stack_count, runtime.output_count, runtime.trace_count,
		     runtime.traces[0], runtime.traces[1], runtime.traces[2], runtime.traces[3]);
	zassert_true(strcmp(runtime.current_app, "reader") == 0,
		     "current=%s status=%d phase=%d target_app=%s exited=%d stack=%u output=%u trace_count=%u trace0=%s trace1=%s trace2=%s trace3=%s",
		     runtime.current_app, runtime.status, runtime.lifecycle_phase,
		     runtime.lifecycle_target_app, runtime.dispatch_exited, runtime.return_stack_count,
		     runtime.output_count, runtime.trace_count, runtime.traces[0], runtime.traces[1],
		     runtime.traces[2], runtime.traces[3]);
	zassert_equal(runtime.return_stack_count, 2);
	zassert_str_equal(runtime.return_stack[0], "main");
	zassert_str_equal(runtime.return_stack[1], "lifecycle");
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "reader start");

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_LIFECYCLE_GET,
						      SQ_STATUS_OK, 47, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN, &context,
						      response, sizeof(response), &response_len),
		      SQ_PROTOCOL_OK);
	struct sq_protocol_frame frame;
	struct sq_protocol_field field;
	size_t offset = 0;
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_true(field_string_equals(&field, "active=reader"));
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_true(field_string_equals(&field, "process_stack[0]=main"));
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_true(field_string_equals(&field, "process_stack[1]=lifecycle"));
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_true(field_string_equals(&field, "armed_stack="));

	runtime.dispatch_exited = true;
	runtime.status = SQ_VM_RUNTIME_COMPLETE;
	for (int i = 0; i < 20; i++) {
		zassert_equal(sq_device_protocol_poll(&context), 0);
		wait_runtime_done(&runtime);
		if (runtime.status != SQ_VM_RUNTIME_RUNNING &&
		    strcmp(runtime.current_app, "lifecycle") == 0) {
			break;
		}
		k_sleep(K_MSEC(1));
	}
	zassert_str_equal(runtime.current_app, "lifecycle");
	zassert_equal(runtime.return_stack_count, 1);
	zassert_str_equal(runtime.return_stack[0], "main");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_app_arm_registers_timer_and_dispatches_armed_app)
{
	enum { PADDED_TRIGGER_SQBC_LEN = 4096 + SQVM_STORAGE_TRANSFER_CAPACITY };
	static uint8_t padded_break_reminder_sqbc[PADDED_TRIGGER_SQBC_LEN];
	uint8_t request[128];
	uint8_t response[512];
	size_t response_len = 0;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_registry registry = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_app_store_vm_storage trigger_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
		.trigger_storage = &trigger_storage,
		.fallback_app = &sq_zephyr_fallback_app,
	};

	memcpy(padded_break_reminder_sqbc, break_reminder_sqbc, sizeof(break_reminder_sqbc));
	memset(&padded_break_reminder_sqbc[sizeof(break_reminder_sqbc)], 0xa5,
	       sizeof(padded_break_reminder_sqbc) - sizeof(break_reminder_sqbc));
	padded_break_reminder_sqbc[6] = PADDED_TRIGGER_SQBC_LEN & 0xff;
	padded_break_reminder_sqbc[7] = (PADDED_TRIGGER_SQBC_LEN >> 8) & 0xff;
	padded_break_reminder_sqbc[8] = (PADDED_TRIGGER_SQBC_LEN >> 16) & 0xff;
	padded_break_reminder_sqbc[9] = (PADDED_TRIGGER_SQBC_LEN >> 24) & 0xff;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "armer", armer_sqbc,
					       sizeof(armer_sqbc)),
		      0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "break-reminder",
					       padded_break_reminder_sqbc,
					       sizeof(padded_break_reminder_sqbc)),
		      0);
	zassert_equal(sq_app_store_scan_registry(test_fs_mount.mnt_point, &registry), 0);

	strncpy(runtime.current_app, "armer", sizeof(runtime.current_app) - 1);
	strncpy(runtime.arm_target_app, "break-reminder", sizeof(runtime.arm_target_app) - 1);
	runtime.arm_phase = SQ_VM_RUNTIME_ARM_REQUESTED;
	zassert_equal(sq_device_protocol_poll(&context), 0);
	zassert_str_equal(runtime.current_app, "armer");
	zassert_equal(runtime.armed_timer_count, 1);

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_LIFECYCLE_GET,
						      SQ_STATUS_OK, 91, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN, &context,
						      response, sizeof(response), &response_len),
		      SQ_PROTOCOL_OK);
	struct sq_protocol_frame frame;
	struct sq_protocol_field field;
	size_t offset = 0;
	bool saw_armed = false;
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	while (sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field) ==
	       SQ_PROTOCOL_OK) {
		saw_armed = saw_armed ||
			    field_string_equals(&field,
						"armed_stack[0]=break-reminder timer.break");
	}
	zassert_true(saw_armed);

	k_sleep(K_MSEC(5));
	for (int i = 0; i < 40; i++) {
		zassert_equal(sq_device_protocol_poll(&context), 0);
		if (runtime.status != SQ_VM_RUNTIME_RUNNING &&
		    strcmp(runtime.current_app, "break-reminder") == 0) {
			break;
		}
		k_sleep(K_MSEC(1));
	}
	zassert_str_equal(runtime.current_app, "break-reminder");
	zassert_equal(runtime.return_stack_count, 1);
	zassert_str_equal(runtime.return_stack[0], "armer");
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "break fired 1");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_app_exit_return_takes_priority_over_due_foreground_timer)
{
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "reader", reader_exit_sqbc,
					       sizeof(reader_exit_sqbc)),
		      0);

	sq_vm_runtime_init(&runtime);
	strncpy(runtime.current_app, "break-reminder", sizeof(runtime.current_app) - 1);
	strncpy(runtime.return_stack[0], "reader", sizeof(runtime.return_stack[0]) - 1);
	runtime.return_stack_count = 1;
	runtime.dispatch_exited = true;
	runtime.status = SQ_VM_RUNTIME_COMPLETE;
	runtime.job_backend = sq_app_store_vm_storage_backend(&launch_storage);
	runtime.timers[0].active = true;
	runtime.timers[0].repeating = true;
	runtime.timers[0].interval_ms = 500;
	runtime.timers[0].due_ms = k_uptime_get() - 1;
	strncpy(runtime.timers[0].event, "timer.clock", sizeof(runtime.timers[0].event) - 1);

	zassert_equal(sq_device_protocol_poll(&context), 0);
	zassert_str_equal(runtime.current_app, "reader");
	zassert_equal(runtime.return_stack_count, 0);
	wait_runtime_done(&runtime);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_foreground_timers_clear_when_armed_app_takes_foreground)
{
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "break-reminder",
					       break_reminder_sqbc, sizeof(break_reminder_sqbc)),
		      0);

	sq_vm_runtime_init(&runtime);
	strncpy(runtime.current_app, "reader", sizeof(runtime.current_app) - 1);
	runtime.status = SQ_VM_RUNTIME_COMPLETE;
	runtime.timers[0].active = true;
	runtime.timers[0].repeating = true;
	runtime.timers[0].interval_ms = 500;
	runtime.timers[0].due_ms = k_uptime_get() - 1;
	strncpy(runtime.timers[0].event, "timer.clock", sizeof(runtime.timers[0].event) - 1);
	runtime.armed_timers[0].active = true;
	runtime.armed_timers[0].repeating = false;
	runtime.armed_timers[0].interval_ms = 1000;
	runtime.armed_timers[0].due_ms = k_uptime_get() - 1;
	strncpy(runtime.armed_timers[0].app_id, "break-reminder",
		sizeof(runtime.armed_timers[0].app_id) - 1);
	strncpy(runtime.armed_timers[0].event, "timer.break",
		sizeof(runtime.armed_timers[0].event) - 1);

	zassert_equal(sq_device_protocol_poll(&context), 0);
	zassert_str_equal(runtime.current_app, "break-reminder");
	zassert_false(runtime.timers[0].active);
	wait_runtime_done(&runtime);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

static int run_temp_fixture(const struct sq_device_protocol_context *context, const char *app_id,
			    const uint8_t *sqbc, size_t sqbc_len)
{
	uint8_t begin_payload[64];
	uint8_t chunk_payload[1024];
	uint8_t request[1200];
	uint8_t response[128];
	size_t payload_len = 0;
	size_t response_len = 0;
	int result;

	if (sqbc_len > sizeof(chunk_payload) - 32) {
		return -ENOSPC;
	}

	result = sq_protocol_append_string_field(begin_payload, sizeof(begin_payload),
						 &payload_len, 1, app_id);
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	result = sq_protocol_append_u64_field(begin_payload, sizeof(begin_payload), &payload_len, 2,
					      sqbc_len);
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	result = sq_protocol_append_u64_field(begin_payload, sizeof(begin_payload), &payload_len, 3,
					      sq_protocol_crc32(sqbc, sqbc_len));
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	result = sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_TEMP_RUN_BEGIN,
						 SQ_STATUS_OK, 50, begin_payload, payload_len,
						 request, sizeof(request));
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], begin_payload, payload_len);
	result = sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						 context, response, sizeof(response),
						 &response_len);
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}

	payload_len = 0;
	result = sq_protocol_append_u64_field(chunk_payload, sizeof(chunk_payload), &payload_len, 1,
					      0);
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	result = sq_protocol_append_bytes_field(chunk_payload, sizeof(chunk_payload), &payload_len,
						2, sqbc, sqbc_len);
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	result = sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_TEMP_RUN_CHUNK,
						 SQ_STATUS_OK, 51, chunk_payload, payload_len,
						 request, sizeof(request));
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], chunk_payload, payload_len);
	result = sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						 context, response, sizeof(response),
						 &response_len);
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}

	result = sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_TEMP_RUN_COMMIT,
						 SQ_STATUS_OK, 52, NULL, 0, request,
						 sizeof(request));
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	return sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN, context, response,
					       sizeof(response), &response_len);
}

static int dispatch_select_key(const struct sq_device_protocol_context *context)
{
	uint8_t payload[32];
	uint8_t request[128];
	uint8_t response[512];
	size_t payload_len = 0;
	size_t response_len = 0;
	int result;

	result = sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						 "SELECT");
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	result = sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_KEY,
						 SQ_STATUS_OK, 60, payload, payload_len,
						 request, sizeof(request));
	if (result != SQ_PROTOCOL_OK) {
		return result;
	}
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	return sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
					       context, response, sizeof(response), &response_len);
}

ZTEST(squidscript_protocol, test_handles_temp_run_commit_dispatches_file_staged_app_start)
{
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_device_temp_session temp_session = {0};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.temp_session = &temp_session,
		.runtime = &runtime,
		.store_mount_point = test_fs_mount.mnt_point,
		.launch_storage = &launch_storage,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	zassert_true(sizeof(temp_session) < 512,
		     "temp-run session must not reserve full SQBC payload RAM");

	zassert_equal(run_temp_fixture(&context, "temp-app", headless_counter_sqbc,
				       sizeof(headless_counter_sqbc)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_poll(&context), 0);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_RUNNING);
	zassert_str_equal(runtime.current_app, "temp-app");
	zassert_true(runtime.current_app_temp);
	wait_runtime_done(&runtime);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);
	zassert_equal(runtime.result_code, 0);
	zassert_equal(runtime.trace_count, 1);
	zassert_str_equal(runtime.traces[0], "app.start");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_temp_run_foreground_timer_reuses_temp_backend)
{
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_device_temp_session temp_session = {0};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.temp_session = &temp_session,
		.runtime = &runtime,
		.store_mount_point = test_fs_mount.mnt_point,
		.launch_storage = &launch_storage,
		.fallback_app = &sq_zephyr_fallback_app,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	zassert_equal(run_temp_fixture(&context, "temp-foreground-timer",
				       temp_foreground_timer_sqbc,
				       sizeof(temp_foreground_timer_sqbc)),
		      SQ_PROTOCOL_OK);
	zassert_equal(poll_until_current_app(&context, &runtime, "temp-foreground-timer"), 0);
	wait_runtime_done(&runtime);
	zassert_str_equal(runtime.current_app, "temp-foreground-timer");
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "temp start");
	zassert_equal(poll_until_output_count(&context, &runtime, 2), 0);
	zassert_str_equal(runtime.current_app, "temp-foreground-timer");
	zassert_equal(runtime.output_count, 2);
	zassert_str_equal(runtime.outputs[1], "temp timer");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_temp_run_can_launch_installed_app_and_return_to_temp)
{
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_device_temp_session temp_session = {0};
	struct sq_vm_runtime runtime = {0};
	struct sq_app_registry registry = {0};
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
		.temp_session = &temp_session,
		.runtime = &runtime,
		.store_mount_point = test_fs_mount.mnt_point,
		.launch_storage = &launch_storage,
		.fallback_app = &sq_zephyr_fallback_app,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "reader", reader_exit_sqbc,
					       sizeof(reader_exit_sqbc)),
		      0);
	zassert_equal(sq_app_store_scan_registry(test_fs_mount.mnt_point, &registry), 0);
	zassert_equal(run_temp_fixture(&context, "temp-foreground-launcher",
				       temp_foreground_launcher_sqbc,
				       sizeof(temp_foreground_launcher_sqbc)),
		      SQ_PROTOCOL_OK);
	zassert_equal(poll_until_current_app(&context, &runtime, "temp-foreground-launcher"), 0);
	wait_runtime_done(&runtime);
	zassert_str_equal(runtime.current_app, "temp-foreground-launcher");
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "temp start");

	zassert_equal(dispatch_select_key(&context), SQ_PROTOCOL_OK);
	wait_runtime_done(&runtime);
	zassert_equal(poll_until_current_app(&context, &runtime, "reader"), 0);
	zassert_equal(runtime.return_stack_count, 2);
	zassert_str_equal(runtime.return_stack[0], "main");
	zassert_str_equal(runtime.return_stack[1], "temp-foreground-launcher");
	zassert_equal(runtime.output_count, 3);
	zassert_str_equal(runtime.outputs[1], "temp exit");
	zassert_str_equal(runtime.outputs[2], "reader start");

	zassert_equal(dispatch_select_key(&context), SQ_PROTOCOL_OK);
	wait_runtime_done(&runtime);
	zassert_equal(poll_until_current_app(&context, &runtime, "temp-foreground-launcher"), 0);
	zassert_equal(runtime.return_stack_count, 1);
	zassert_str_equal(runtime.return_stack[0], "main");
	zassert_str_equal(runtime.current_app, "temp-foreground-launcher");
	zassert_str_equal(runtime.outputs[runtime.output_count - 1], "temp start");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_serial_transport_accumulates_one_complete_frame)
{
	struct sq_serial_transport transport;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_device_protocol_context context = {
		.identity = &identity,
	};
	uint8_t response[128];
	size_t response_len = 0;
	int completed = 0;

	sq_serial_transport_init(&transport);

	for (size_t i = 0; i < sizeof(hello_frame); i++) {
		int result = sq_serial_transport_push_byte(&transport, hello_frame[i], &context,
							   response, sizeof(response), &response_len);
		zassert_true(result >= 0, "transport rejected byte %zu with %d", i, result);
		completed += result;
	}

	zassert_equal(completed, 1);
	zassert_true(response_len > SQ_PROTOCOL_HEADER_LEN);

	struct sq_protocol_frame frame;
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), 0);
	zassert_equal(frame.kind, SQ_FRAME_RESPONSE);
	zassert_equal(frame.opcode, SQ_OPCODE_HELLO);
	zassert_equal(frame.sequence, 7);
}

ZTEST(squidscript_protocol, test_serial_transport_resynchronizes_after_leading_noise)
{
	struct sq_serial_transport transport;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_device_protocol_context context = {
		.identity = &identity,
	};
	const uint8_t noise[] = {0x00, 0xff, 'E', 'S', 'P', '\r', '\n'};
	uint8_t response[128];
	size_t response_len = 0;
	int completed = 0;

	sq_serial_transport_init(&transport);

	for (size_t i = 0; i < sizeof(noise); i++) {
		int result = sq_serial_transport_push_byte(&transport, noise[i], &context,
							   response, sizeof(response), &response_len);
		zassert_true(result >= 0, "transport rejected noise byte %zu with %d", i,
			     result);
		completed += result;
	}
	for (size_t i = 0; i < sizeof(hello_frame); i++) {
		int result = sq_serial_transport_push_byte(&transport, hello_frame[i], &context,
							   response, sizeof(response), &response_len);
		zassert_true(result >= 0, "transport rejected frame byte %zu with %d", i,
			     result);
		completed += result;
	}

	zassert_equal(completed, 1);
	zassert_true(response_len > SQ_PROTOCOL_HEADER_LEN);
	struct sq_protocol_frame frame;
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), 0);
	zassert_equal(frame.opcode, SQ_OPCODE_HELLO);
}

ZTEST(squidscript_protocol, test_links_squidvm_ffi_context_metadata)
{
	zassert_true(sqvm_context_size() > 0);
	zassert_true(sqvm_context_align() > 0);
	zassert_true(sqvm_context_size() <= SQ_VM_RUNTIME_CONTEXT_BYTES);
#if !defined(CONFIG_BOARD_NATIVE_SIM)
	zassert_true(SQ_VM_RUNTIME_CONTEXT_BYTES <= 7872);
#endif
	zassert_true(SQ_VM_RUNTIME_WORK_STACK_SIZE <= 24576);
}

ZTEST(squidscript_protocol, test_runtime_wait_idle_times_out_while_worker_is_running)
{
	struct sq_vm_runtime runtime = {
		.status = SQ_VM_RUNTIME_RUNNING,
	};

	zassert_equal(sq_vm_runtime_wait_idle(&runtime, 0), -ETIMEDOUT);
}

ZTEST(squidscript_protocol, test_runtime_wait_idle_handles_initialized_unsubmitted_work)
{
	static struct sq_vm_runtime runtime;

	memset(&runtime, 0, sizeof(runtime));
	sq_vm_runtime_init(&runtime);

	zassert_false(runtime.work_submitted);
	zassert_equal(sq_vm_runtime_wait_idle(&runtime, 0), 0);
	zassert_false(runtime.work_submitted);
}

ZTEST(squidscript_protocol, test_runtime_reuses_transfer_storage_for_init_scratch_and_completion)
{
	static struct sq_vm_runtime runtime;

	zassert_equal(sizeof(runtime.transfer.init_scratch), SQ_VM_RUNTIME_SCRATCH_BYTES);
	zassert_true(sizeof(runtime.transfer) >= sizeof(runtime.transfer.init_scratch));
	zassert_true(sizeof(runtime.transfer) >= sizeof(runtime.transfer.completion));
#if !defined(CONFIG_BOARD_NATIVE_SIM)
	size_t runtime_static = sizeof(runtime);
	zassert_true(runtime_static <= 12160, "runtime_static=%zu", runtime_static);
#endif
}

ZTEST(squidscript_protocol, test_squidscript_owned_fixed_buffer_budgets)
{
	zassert_true(sizeof(struct sq_device_protocol_scratch) <= 552,
		     "protocol scratch=%zu", sizeof(struct sq_device_protocol_scratch));
	zassert_true(sizeof(struct sq_device_install_session) <= 160,
		     "install session=%zu", sizeof(struct sq_device_install_session));
	zassert_true(sizeof(struct sq_device_temp_session) <= 160,
		     "temp session=%zu", sizeof(struct sq_device_temp_session));
	zassert_true(sizeof(struct sq_device_resource_session) <= 240,
		     "resource session=%zu", sizeof(struct sq_device_resource_session));
	zassert_true(sizeof(struct sq_app_registry) <= 356,
		     "app registry=%zu", sizeof(struct sq_app_registry));
	zassert_true(sizeof(struct sq_app_store_vm_storage) <= 168,
		     "app storage=%zu", sizeof(struct sq_app_store_vm_storage));
}

ZTEST(squidscript_protocol, test_output_history_retains_current_lifecycle_assertion_window)
{
	struct sq_vm_runtime runtime = {0};
	char line[24];

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_reset(&runtime);

	for (size_t i = 0; i < SQ_VM_RUNTIME_OUTPUT_MAX + 1; i++) {
		int written = snprintf(line, sizeof(line), "lifecycle line %zu", i);
		zassert_true(written > 0 && (size_t)written < sizeof(line));
		zassert_equal(sq_vm_runtime_record_output(&runtime, (const uint8_t *)line,
							  (size_t)written),
			      0);
	}

	zassert_equal(runtime.output_count, 6);
	zassert_str_equal(runtime.outputs[0], "lifecycle line 1");
	zassert_str_equal(runtime.outputs[1], "lifecycle line 2");
	zassert_str_equal(runtime.outputs[2], "lifecycle line 3");
	zassert_str_equal(runtime.outputs[3], "lifecycle line 4");
	zassert_str_equal(runtime.outputs[4], "lifecycle line 5");
	zassert_str_equal(runtime.outputs[5], "lifecycle line 6");
}

ZTEST(squidscript_protocol, test_runtime_active_caps_default_to_hard_caps_and_gate_runtime_tables)
{
	struct sq_vm_runtime runtime = {0};

	sq_vm_runtime_init(&runtime);

	zassert_equal(runtime.active_timer_max, SQ_VM_RUNTIME_TIMER_MAX);
	zassert_equal(runtime.active_armed_timer_max, SQ_VM_RUNTIME_ARMED_TIMER_MAX);
	zassert_equal(runtime.active_input_button_max, SQ_VM_RUNTIME_INPUT_BUTTON_MAX);
	zassert_equal(runtime.active_binding_max, SQ_VM_RUNTIME_ACTIVE_BINDING_MAX);
	zassert_equal(runtime.active_output_max, SQ_VM_RUNTIME_OUTPUT_MAX);
	zassert_equal(runtime.active_drawlog_max, SQ_VM_RUNTIME_DRAWLOG_MAX);

	runtime.active_timer_max = 2;
	zassert_equal(sq_vm_runtime_register_timer(&runtime, (const uint8_t *)"timer.a",
						   strlen("timer.a"), 100, true),
		      0);
	zassert_equal(sq_vm_runtime_register_timer(&runtime, (const uint8_t *)"timer.b",
						   strlen("timer.b"), 100, true),
		      0);
	zassert_equal(sq_vm_runtime_register_timer(&runtime, (const uint8_t *)"timer.c",
						   strlen("timer.c"), 100, true),
		      -ENOSPC);

	runtime.active_output_max = 2;
	zassert_equal(sq_vm_runtime_record_output(&runtime, (const uint8_t *)"out.1",
						  strlen("out.1")),
		      0);
	zassert_equal(sq_vm_runtime_record_output(&runtime, (const uint8_t *)"out.2",
						  strlen("out.2")),
		      0);
	zassert_equal(sq_vm_runtime_record_output(&runtime, (const uint8_t *)"out.3",
						  strlen("out.3")),
		      0);
	zassert_equal(runtime.output_count, 2);
	zassert_str_equal(runtime.outputs[0], "out.2");
	zassert_str_equal(runtime.outputs[1], "out.3");

	runtime.active_drawlog_max = 1;
	zassert_equal(sq_vm_runtime_record_drawlog(&runtime, "draw.1"), 0);
	zassert_equal(sq_vm_runtime_record_drawlog(&runtime, "draw.2"), 0);
	zassert_equal(runtime.drawlog_count, 1);
	zassert_str_equal(runtime.drawlog[0], "draw.2");
}

ZTEST(squidscript_protocol, test_runtime_cap_protocol_get_set_and_clear_active_caps)
{
	uint8_t request[128];
	uint8_t payload[96];
	uint8_t response[SQ_DEVICE_RESPONSE_BYTES];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	struct sq_protocol_field field;
	size_t offset = 0;
	static struct sq_vm_runtime runtime;
	struct sq_device_identity identity = {
		.target = "native-test",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.runtime = &runtime,
	};

	memset(&runtime, 0, sizeof(runtime));
	sq_vm_runtime_init(&runtime);

	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "vm_runtime.timer_max"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_u32_field(payload, sizeof(payload), &payload_len, 2, 2),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_RUNTIME_CAP_SET,
						      SQ_STATUS_OK, 82, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(runtime.active_timer_max, 2);

	payload_len = 0;
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "vm_runtime.timer_max"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_RUNTIME_CAP_GET,
						      SQ_STATUS_OK, 83, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_RUNTIME_CAP_GET);
	zassert_equal(frame.status, SQ_STATUS_OK);
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_equal(field.type, SQ_FIELD_STRING);
	zassert_mem_equal(field.value, "vm_runtime.timer_max=2", strlen("vm_runtime.timer_max=2"));

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_RUNTIME_CAP_CLEAR,
						      SQ_STATUS_OK, 84, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(runtime.active_timer_max, SQ_VM_RUNTIME_TIMER_MAX);
}

ZTEST(squidscript_protocol, test_runtime_cap_protocol_set_persists_active_caps)
{
	uint8_t request[128];
	uint8_t payload[96];
	uint8_t response[SQ_DEVICE_RESPONSE_BYTES];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_vm_runtime runtime = {0};
	struct sq_vm_runtime reloaded = {0};
	uint16_t timer_cap = 0;
	struct sq_device_identity identity = {
		.target = "native-test",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.runtime = &runtime,
		.store_mount_point = test_fs_mount.mnt_point,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_set_store_mount_point(&runtime, test_fs_mount.mnt_point);

	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "vm_runtime.timer_max"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_u32_field(payload, sizeof(payload), &payload_len, 2, 2),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_RUNTIME_CAP_SET,
						      SQ_STATUS_OK, 85, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);

	sq_vm_runtime_init(&reloaded);
	sq_vm_runtime_set_store_mount_point(&reloaded, test_fs_mount.mnt_point);
	zassert_equal(sq_vm_runtime_cap_get(&reloaded, "vm_runtime.timer_max", &timer_cap), 0);
	zassert_equal(timer_cap, 2);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_runtime_transfer_owner_rejects_overlap)
{
	static struct sq_vm_runtime runtime;

	memset(&runtime, 0, sizeof(runtime));
	zassert_equal(sq_vm_runtime_transfer_acquire(&runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH), 0);
	zassert_equal(sq_vm_runtime_transfer_acquire(&runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION),
		      -EBUSY);
	zassert_equal(sq_vm_runtime_transfer_release(&runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION),
		      -EBUSY);
	zassert_equal(sq_vm_runtime_transfer_release(&runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH), 0);
	zassert_equal(sq_vm_runtime_transfer_acquire(&runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION),
		      0);
	zassert_equal(sq_vm_runtime_transfer_release(&runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION),
		      0);
}

ZTEST(squidscript_protocol, test_protocol_transfer_session_phases_track_begin_chunk_commit)
{
	struct sq_device_install_session install = {0};
	struct sq_device_temp_session temp = {0};
	struct sq_device_resource_session resource = {0};

	zassert_equal(install.phase, SQ_DEVICE_TRANSFER_IDLE);
	zassert_equal(temp.phase, SQ_DEVICE_TRANSFER_IDLE);
	zassert_equal(resource.phase, SQ_DEVICE_TRANSFER_IDLE);

	transfer_session_begin_receiving(&install);
	transfer_session_begin_receiving(&temp);
	transfer_session_begin_receiving(&resource);
	zassert_true(install.active);
	zassert_true(temp.active);
	zassert_true(resource.active);
	zassert_equal(install.phase, SQ_DEVICE_TRANSFER_RECEIVING);
	zassert_equal(temp.phase, SQ_DEVICE_TRANSFER_RECEIVING);
	zassert_equal(resource.phase, SQ_DEVICE_TRANSFER_RECEIVING);

	zassert_equal(transfer_session_begin_committing(&install), 0);
	zassert_equal(transfer_session_begin_committing(&temp), 0);
	zassert_equal(transfer_session_begin_committing(&resource), 0);
	zassert_equal(install.phase, SQ_DEVICE_TRANSFER_COMMITTING);
	zassert_equal(temp.phase, SQ_DEVICE_TRANSFER_COMMITTING);
	zassert_equal(resource.phase, SQ_DEVICE_TRANSFER_COMMITTING);

	transfer_session_finish_idle(&install);
	transfer_session_finish_idle(&temp);
	transfer_session_finish_idle(&resource);
	zassert_false(install.active);
	zassert_false(temp.active);
	zassert_false(resource.active);
	zassert_equal(install.phase, SQ_DEVICE_TRANSFER_IDLE);
	zassert_equal(temp.phase, SQ_DEVICE_TRANSFER_IDLE);
	zassert_equal(resource.phase, SQ_DEVICE_TRANSFER_IDLE);
}

ZTEST(squidscript_protocol, test_input_button_phase_tracks_press_and_release_without_release_dispatch)
{
	struct sq_vm_runtime_input_button button = {0};

	zassert_equal(button.phase, SQ_VM_RUNTIME_INPUT_INACTIVE);
	button.active = true;
	button.phase = SQ_VM_RUNTIME_INPUT_RELEASED;
	button.pressed = false;
	button.phase = SQ_VM_RUNTIME_INPUT_DEBOUNCING_PRESS;
	zassert_equal(button.phase, SQ_VM_RUNTIME_INPUT_DEBOUNCING_PRESS);
	button.pressed = true;
	button.phase = SQ_VM_RUNTIME_INPUT_PRESSED;
	zassert_equal(button.phase, SQ_VM_RUNTIME_INPUT_PRESSED);
	button.phase = SQ_VM_RUNTIME_INPUT_DEBOUNCING_RELEASE;
	zassert_equal(button.phase, SQ_VM_RUNTIME_INPUT_DEBOUNCING_RELEASE);
	button.pressed = false;
	button.phase = SQ_VM_RUNTIME_INPUT_RELEASED;
	zassert_equal(button.phase, SQ_VM_RUNTIME_INPUT_RELEASED);
}

ZTEST(squidscript_protocol, test_resources_report_vm_worker_stack_diagnostics)
{
	uint8_t request[SQ_PROTOCOL_HEADER_LEN];
	uint8_t response[SQ_DEVICE_RESPONSE_BYTES];
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	static struct sq_vm_runtime runtime;
	struct sq_device_identity identity = {
		.target = "native-test",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.runtime = &runtime,
	};
	uint64_t stack_unused = 0;
	uint64_t stack_used = 0;
	uint64_t protocol_stack_unused = 0;
	uint64_t protocol_stack_used = 0;
	uint64_t protocol_stack_pre_resources_unused = 0;
	uint64_t protocol_stack_pre_resources_used = 0;
	uint64_t vm_sqbc_chunk = 0;
	uint64_t heap_largest_free_supported = 99;
	uint64_t heap_largest_free_bytes = 99;
	uint64_t last_dispatch_sequence = 99;
	uint64_t last_dispatch_elapsed_us = 99;
	uint64_t last_dispatch_sqbc_read_count = 99;
	uint64_t last_dispatch_sqbc_read_bytes = 99;
	uint64_t runtime_status = 99;
	uint64_t runtime_dispatch_started = 99;
	uint64_t runtime_dispatch_age_us = 99;
	uint64_t runtime_work_submitted = 99;
	uint64_t runtime_current_app_present = 99;
	uint64_t runtime_lifecycle_phase = 99;
	uint64_t runtime_arm_phase = 99;
	uint64_t active_timer_max = 99;
	uint64_t active_output_max = 99;
	int result;

	memset(&runtime, 0, sizeof(runtime));
	sq_vm_runtime_init(&runtime);
	runtime.active_timer_max = 2;
	runtime.active_output_max = 3;
	runtime.status = SQ_VM_RUNTIME_RUNNING;
	runtime.dispatch_started = true;
	runtime.dispatch_start_cycles = 0;
	runtime.work_submitted = true;
	runtime.lifecycle_phase = SQ_VM_RUNTIME_LIFECYCLE_SLEEP_REQUESTED;
	runtime.arm_phase = SQ_VM_RUNTIME_ARM_REQUESTED;
	strncpy(runtime.current_app, "triage-app", sizeof(runtime.current_app) - 1);
	runtime.last_dispatch_sequence = 7;
	runtime.last_dispatch_elapsed_us = 1234;
	runtime.last_dispatch_sqbc_read_count = 2;
	runtime.last_dispatch_sqbc_read_bytes = 2048;

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_RESOURCES_GET,
						      SQ_STATUS_OK, 73, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(request, sizeof(request), &frame), SQ_PROTOCOL_OK,
		      "request decode result before handle");
	result = sq_device_protocol_handle_frame(request, sizeof(request), &context, response,
						 sizeof(response), &response_len);
	zassert_equal(result, SQ_PROTOCOL_OK, "resources result %d", result);
	zassert_true(response_len <= SQ_DEVICE_RESPONSE_BYTES, "resources response_len=%zu",
		     response_len);
	zassert_true(response_len <= SQ_DEVICE_RESPONSE_BYTES);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_RESOURCES_GET);
	zassert_equal(frame.status, SQ_STATUS_OK);

	zassert_true(resource_value_equals(&frame, "vm_stack_size_bytes",
					   SQ_VM_RUNTIME_WORK_STACK_SIZE));
	zassert_true(resource_value_equals(&frame, "proto_stack_size_bytes",
					   CONFIG_MAIN_STACK_SIZE));
	zassert_true(resource_value_for_key(&frame, "vm_sqbc_chunk_bytes", &vm_sqbc_chunk));
	zassert_equal(vm_sqbc_chunk, SQVM_STORAGE_TRANSFER_CAPACITY);
	zassert_true(resource_value_for_key(&frame, "heap_largest_free_supported",
					    &heap_largest_free_supported));
	zassert_equal(heap_largest_free_supported, 0);
	zassert_true(resource_value_for_key(&frame, "heap_largest_free_bytes",
					    &heap_largest_free_bytes));
	zassert_equal(heap_largest_free_bytes, 0);
	zassert_true(resource_value_for_key(&frame, "last_dispatch_us",
					    &last_dispatch_elapsed_us));
	zassert_equal(last_dispatch_elapsed_us, 1234);
	zassert_true(resource_value_for_key(&frame, "last_dispatch_seq",
					    &last_dispatch_sequence));
	zassert_equal(last_dispatch_sequence, 7);
	zassert_true(resource_value_for_key(&frame, "last_sqbc_reads",
					    &last_dispatch_sqbc_read_count));
	zassert_equal(last_dispatch_sqbc_read_count, 2);
	zassert_true(resource_value_for_key(&frame, "last_sqbc_bytes",
					    &last_dispatch_sqbc_read_bytes));
	zassert_equal(last_dispatch_sqbc_read_bytes, 2048);
	zassert_true(resource_value_for_key(&frame, "runtime_status",
					    &runtime_status));
	zassert_equal(runtime_status, SQ_VM_RUNTIME_RUNNING);
	zassert_true(resource_value_for_key(&frame, "runtime_dispatch_started",
					    &runtime_dispatch_started));
	zassert_equal(runtime_dispatch_started, 1);
	zassert_true(resource_value_for_key(&frame, "runtime_dispatch_age_us",
					    &runtime_dispatch_age_us));
	zassert_true(runtime_dispatch_age_us > 0);
	zassert_true(resource_value_for_key(&frame, "runtime_work_submitted",
					    &runtime_work_submitted));
	zassert_equal(runtime_work_submitted, 1);
	zassert_true(resource_value_for_key(&frame, "runtime_current_app_present",
					    &runtime_current_app_present));
	zassert_equal(runtime_current_app_present, 1);
	zassert_true(resource_value_for_key(&frame, "runtime_lifecycle_phase",
					    &runtime_lifecycle_phase));
	zassert_equal(runtime_lifecycle_phase, SQ_VM_RUNTIME_LIFECYCLE_SLEEP_REQUESTED);
	zassert_true(resource_value_for_key(&frame, "runtime_arm_phase",
					    &runtime_arm_phase));
	zassert_equal(runtime_arm_phase, SQ_VM_RUNTIME_ARM_REQUESTED);
	zassert_true(resource_value_for_key(&frame, "cap.active.timer",
					    &active_timer_max));
	zassert_equal(active_timer_max, 2);
	zassert_true(resource_value_for_key(&frame, "cap.active.output",
					    &active_output_max));
	zassert_equal(active_output_max, 3);
	zassert_true(resource_value_equals(&frame, "cap.static.timer", SQ_VM_RUNTIME_TIMER_MAX));
	zassert_true(resource_value_equals(&frame, "cap.static.device_error",
					   SQ_VM_RUNTIME_DEVICE_ERROR_MAX));
	zassert_true(resource_value_for_key(&frame, "proto_stack_unused_bytes",
					    &protocol_stack_unused));
	zassert_true(resource_value_for_key(&frame, "proto_stack_used_bytes",
					    &protocol_stack_used));
	zassert_true(resource_value_for_key(&frame,
					    "proto_stack_pre_unused_bytes",
					    &protocol_stack_pre_resources_unused));
	zassert_true(resource_value_for_key(&frame,
					    "proto_stack_pre_used_bytes",
					    &protocol_stack_pre_resources_used));
	zassert_true(protocol_stack_unused <= CONFIG_MAIN_STACK_SIZE);
	zassert_true(protocol_stack_used <= CONFIG_MAIN_STACK_SIZE);
	zassert_true(protocol_stack_pre_resources_unused <= CONFIG_MAIN_STACK_SIZE);
	zassert_true(protocol_stack_pre_resources_used <= CONFIG_MAIN_STACK_SIZE);
	if (protocol_stack_unused != 0 || protocol_stack_used != 0) {
		zassert_equal(protocol_stack_unused + protocol_stack_used, CONFIG_MAIN_STACK_SIZE,
			      "unused=%llu used=%llu", protocol_stack_unused, protocol_stack_used);
	}
	if (protocol_stack_pre_resources_unused != 0 || protocol_stack_pre_resources_used != 0) {
		zassert_equal(protocol_stack_pre_resources_unused + protocol_stack_pre_resources_used,
			      CONFIG_MAIN_STACK_SIZE, "unused=%llu used=%llu",
			      protocol_stack_pre_resources_unused,
			      protocol_stack_pre_resources_used);
	}
	zassert_true(resource_value_for_key(&frame, "vm_stack_unused_bytes", &stack_unused));
	zassert_true(resource_value_for_key(&frame, "vm_stack_used_bytes", &stack_used));
	zassert_true(stack_unused <= SQ_VM_RUNTIME_WORK_STACK_SIZE);
	zassert_true(stack_used <= SQ_VM_RUNTIME_WORK_STACK_SIZE);
	zassert_equal(stack_unused + stack_used, SQ_VM_RUNTIME_WORK_STACK_SIZE,
		      "unused=%llu used=%llu", stack_unused, stack_used);
}

ZTEST(squidscript_protocol, test_resources_request_accepts_heap_max_reset_option)
{
	const uint8_t payload[] = {1, SQ_FIELD_BOOL, 1, 0, 1};
	uint8_t request[SQ_PROTOCOL_HEADER_LEN + sizeof(payload)];
	uint8_t response[SQ_DEVICE_RESPONSE_BYTES];
	size_t response_len = 0;
	struct sq_protocol_frame frame;
	static struct sq_vm_runtime runtime;
	struct sq_device_identity identity = {
		.target = "native-test",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.runtime = &runtime,
	};

	memset(&runtime, 0, sizeof(runtime));
	sq_vm_runtime_init(&runtime);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_RESOURCES_GET,
						      SQ_STATUS_OK, 74, payload,
						      sizeof(payload), request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, sizeof(payload));

	zassert_equal(sq_device_protocol_handle_frame(request, sizeof(request), &context,
						      response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_decode_frame(response, response_len, &frame), SQ_PROTOCOL_OK);
	zassert_equal(frame.opcode, SQ_OPCODE_RESOURCES_GET);
	zassert_equal(frame.status, SQ_STATUS_OK);
	zassert_true(resource_value_equals(&frame, "heap_largest_free_supported", 0));
}

ZTEST(squidscript_protocol, test_exposes_resumable_squidvm_ffi_abi)
{
	SqvmCallbacks callbacks = {0};
	SqvmDispatchResult result = {0};
	SqvmStorageCompletion completion = {0};

	zassert_equal(sqvm_storage_transfer_capacity(), SQVM_STORAGE_TRANSFER_CAPACITY);
	zassert_true(SQVM_STORAGE_TRANSFER_CAPACITY <= 640);
	zassert_equal(sqvm_saved_state_capacity(), SQVM_SAVED_STATE_CAPACITY);
	zassert_equal(sizeof(result.storage.bytes), SQVM_STORAGE_TRANSFER_CAPACITY);
	zassert_equal(sizeof(completion.bytes), SQVM_STORAGE_TRANSFER_CAPACITY);
	zassert_true(SQ_VM_RUNTIME_SCRATCH_BYTES >= SQVM_STORAGE_TRANSFER_CAPACITY);
	zassert_equal(SQ_DEVICE_TEMP_STATE_BYTES, SQVM_SAVED_STATE_CAPACITY);

	zassert_equal(sqvm_dispatch_start_resumable(NULL, NULL, &callbacks,
						    (const uint8_t *)"app.start", 9, &result),
		      SQVM_STATUS_INVALID_ARGUMENT);
	zassert_equal(sqvm_dispatch_resume_storage(NULL, NULL, &callbacks, &completion, &result),
		      SQVM_STATUS_INVALID_ARGUMENT);
	zassert_str_equal(sq_vm_runtime_status_name(SQVM_STATUS_INVALID_ARGUMENT),
			  "invalid_argument");
	zassert_equal(sq_vm_runtime_status_to_errno(SQVM_STATUS_INVALID_ARGUMENT), -EINVAL);
	zassert_str_equal(sq_vm_runtime_status_name(SQVM_STATUS_VM_ERROR), "vm_error");
	zassert_equal(sq_vm_runtime_status_to_errno(SQVM_STATUS_VM_ERROR), -EIO);
}

ZTEST(squidscript_protocol, test_vm_runtime_callback_boundary_statuses)
{
	static const char *timer_events[] = {
		"timer.0",  "timer.1",  "timer.2",  "timer.3",  "timer.4",  "timer.5",
		"timer.6",  "timer.7",  "timer.8",  "timer.9",  "timer.10", "timer.11",
		"timer.12", "timer.13", "timer.14", "timer.15",
	};
	struct sq_vm_runtime runtime = {0};
	bool indicator = false;

	zassert_true(ARRAY_SIZE(timer_events) >= SQ_VM_RUNTIME_TIMER_MAX);
	zassert_true(strlen("timer.breathe.marker") < SQ_VM_RUNTIME_EVENT_LEN);
	sq_vm_runtime_init(&runtime);

	zassert_equal(sq_vm_runtime_indicator_read(NULL, &indicator), -EINVAL);
	zassert_equal(sq_vm_runtime_indicator_read(&runtime, NULL), -EINVAL);
	zassert_equal(sq_vm_runtime_indicator_blink(&runtime, 0, 80), -EINVAL);
	zassert_equal(sq_vm_runtime_indicator_write(&runtime, true), -ENODEV);
	zassert_equal(sq_vm_runtime_hardware_gpio_write(NULL, (const uint8_t *)"GPIO8", 5, true),
		      -EINVAL);
	zassert_equal(sq_vm_runtime_hardware_gpio_write(&runtime, NULL, 0, true), -EINVAL);
	zassert_equal(sq_vm_runtime_hardware_gpio_write(&runtime, (const uint8_t *)"BAD8", 4,
							true),
		      -EINVAL);
	zassert_equal(sq_vm_runtime_hardware_gpio_write(&runtime, (const uint8_t *)"GPIO26", 6,
							true),
		      -EINVAL);

	zassert_equal(sq_vm_runtime_register_timer(NULL, (const uint8_t *)"timer.ok", 8, 100,
						   true),
		      -EINVAL);
	zassert_equal(sq_vm_runtime_register_timer(&runtime, NULL, 0, 100, true), -EINVAL);
	zassert_equal(sq_vm_runtime_register_timer(&runtime, (const uint8_t *)"timer.ok", 8, 0,
						   true),
		      -EINVAL);

	for (size_t i = 0; i < SQ_VM_RUNTIME_TIMER_MAX; i++) {
		zassert_equal(sq_vm_runtime_register_timer(&runtime,
							   (const uint8_t *)timer_events[i],
							   strlen(timer_events[i]), 100, true),
			      0);
	}
	zassert_equal(sq_vm_runtime_register_timer(&runtime, (const uint8_t *)"timer.overflow",
						   strlen("timer.overflow"), 100, true),
		      -ENOSPC);
	zassert_equal(sq_vm_runtime_register_timer(&runtime, (const uint8_t *)timer_events[0],
						   strlen(timer_events[0]), 200, false),
		      0);
	zassert_false(runtime.timers[0].repeating);
	zassert_equal(runtime.timers[0].interval_ms, 200);
}

ZTEST(squidscript_protocol, test_transfer_sessions_use_bounded_internal_path_capacity)
{
	char max_app_id[SQ_APP_STORE_APP_ID_MAX];
	char staging_path[SQ_DEVICE_STAGING_PATH_BYTES];

	zassert_equal(SQ_DEVICE_STAGING_PATH_BYTES, 80);
	zassert_equal(SQ_DEVICE_RESOURCE_PATH_BYTES, 80);
	zassert_true(SQ_DEVICE_STAGING_PATH_BYTES < SQ_APP_STORE_PATH_MAX);
	zassert_true(SQ_DEVICE_RESOURCE_PATH_BYTES < SQ_APP_STORE_PATH_MAX);
	zassert_equal(sizeof(((struct sq_device_install_session *)0)->staging_path),
		      SQ_DEVICE_STAGING_PATH_BYTES);
	zassert_equal(sizeof(((struct sq_device_temp_session *)0)->staging_path),
		      SQ_DEVICE_STAGING_PATH_BYTES);
	zassert_equal(sizeof(((struct sq_device_resource_session *)0)->staging_path),
		      SQ_DEVICE_STAGING_PATH_BYTES);
	zassert_equal(sizeof(((struct sq_device_resource_session *)0)->resource_path),
		      SQ_DEVICE_RESOURCE_PATH_BYTES);

	memset(max_app_id, 'a', sizeof(max_app_id) - 1);
	max_app_id[sizeof(max_app_id) - 1] = '\0';
	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_begin_staged_install(test_fs_mount.mnt_point, max_app_id,
							staging_path, sizeof(staging_path)),
		      0);
	zassert_true(strlen(staging_path) < sizeof(staging_path));
	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

struct vm_storage_fixture {
	const uint8_t *sqbc;
	size_t sqbc_len;
	uint8_t state[SQVM_STORAGE_TRANSFER_CAPACITY];
	size_t state_len;
	bool state_present;
	bool reset_called;
};

struct delayed_vm_storage_fixture {
	struct vm_storage_fixture storage;
	int32_t read_delay_ms;
};

struct budgeted_vm_storage_fixture {
	struct vm_storage_fixture storage;
	size_t read_budget;
	size_t read_count;
	bool over_budget;
};

struct ffi_vm_fixture {
	struct vm_storage_fixture storage;
	char traces[4][32];
	size_t trace_count;
};

static int fixture_read_sqbc(void *user_data, size_t offset, uint8_t *out, size_t len);

static void ffi_trace(void *user_data, const uint8_t *message, size_t message_len)
{
	struct ffi_vm_fixture *fixture = user_data;

	if (fixture->trace_count >= ARRAY_SIZE(fixture->traces)) {
		return;
	}
	size_t len = MIN(message_len, sizeof(fixture->traces[0]) - 1);
	memcpy(fixture->traces[fixture->trace_count], message, len);
	fixture->traces[fixture->trace_count][len] = '\0';
	fixture->trace_count++;
}

static int32_t ffi_read_exact_at(void *user_data, size_t offset, uint8_t *out, size_t out_len)
{
	struct ffi_vm_fixture *fixture = user_data;

	return fixture_read_sqbc(&fixture->storage, offset, out, out_len);
}

static int32_t ffi_app_lifecycle(void *user_data, const char *action, const uint8_t *app,
				 size_t app_len)
{
	struct ffi_vm_fixture *fixture = user_data;
	char line[32];
	int written = snprintf(line, sizeof(line), "%s %.*s", action, (int)app_len, app);

	if (written <= 0) {
		return -EINVAL;
	}
	ffi_trace(fixture, (const uint8_t *)line, strlen(line));
	return 0;
}

static int32_t ffi_app_arm(void *user_data, const uint8_t *app, size_t app_len)
{
	return ffi_app_lifecycle(user_data, "arm", app, app_len);
}

static int32_t ffi_app_launch(void *user_data, const uint8_t *app, size_t app_len)
{
	return ffi_app_lifecycle(user_data, "launch", app, app_len);
}

static int32_t ffi_app_disarm(void *user_data, const uint8_t *app, size_t app_len)
{
	return ffi_app_lifecycle(user_data, "disarm", app, app_len);
}

static int fixture_read_sqbc(void *user_data, size_t offset, uint8_t *out, size_t len)
{
	struct vm_storage_fixture *fixture = user_data;

	if (offset > fixture->sqbc_len || len > fixture->sqbc_len - offset) {
		return -EINVAL;
	}
	memcpy(out, fixture->sqbc + offset, len);
	return 0;
}

static int delayed_fixture_read_sqbc(void *user_data, size_t offset, uint8_t *out, size_t len)
{
	struct delayed_vm_storage_fixture *fixture = user_data;

	if (fixture->read_delay_ms > 0) {
		k_sleep(K_MSEC(fixture->read_delay_ms));
	}
	return fixture_read_sqbc(&fixture->storage, offset, out, len);
}

static int budgeted_fixture_read_sqbc(void *user_data, size_t offset, uint8_t *out, size_t len)
{
	struct budgeted_vm_storage_fixture *fixture = user_data;

	if (fixture->read_budget == 0) {
		fixture->over_budget = true;
		return -EAGAIN;
	}
	fixture->read_budget--;
	fixture->read_count++;
	return fixture_read_sqbc(&fixture->storage, offset, out, len);
}

static int fixture_load_state(void *user_data, uint8_t *out, size_t out_len, size_t *len)
{
	struct vm_storage_fixture *fixture = user_data;

	if (!fixture->state_present) {
		*len = 0;
		return 0;
	}
	if (fixture->state_len > out_len) {
		return -ENOSPC;
	}
	memcpy(out, fixture->state, fixture->state_len);
	*len = fixture->state_len;
	return 0;
}

static int fixture_save_state(void *user_data, const uint8_t *bytes, size_t len)
{
	struct vm_storage_fixture *fixture = user_data;

	if (len > sizeof(fixture->state)) {
		return -ENOSPC;
	}
	memcpy(fixture->state, bytes, len);
	fixture->state_len = len;
	fixture->state_present = true;
	return 0;
}

static int fixture_reset_state(void *user_data)
{
	struct vm_storage_fixture *fixture = user_data;

	fixture->state_len = 0;
	fixture->state_present = false;
	fixture->reset_called = true;
	return 0;
}

ZTEST(squidscript_protocol, test_app_start_returns_without_waiting_for_binding_setup)
{
	struct delayed_vm_storage_fixture fixture = {
		.storage = {
			.sqbc = headless_counter_sqbc,
			.sqbc_len = sizeof(headless_counter_sqbc),
		},
		.read_delay_ms = 50,
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = delayed_fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	static struct sq_vm_runtime runtime;
	int64_t start_ms;
	int64_t elapsed_ms;

	memset(&runtime, 0, sizeof(runtime));
	runtime.store_mount_point = test_fs_mount.mnt_point;
	strncpy(runtime.current_app, "headless-counter", sizeof(runtime.current_app) - 1);

	start_ms = k_uptime_get();
	zassert_equal(sq_vm_runtime_start_event(&runtime, &backend,
						(const uint8_t *)"app.start",
						sizeof("app.start") - 1),
		      0);
	elapsed_ms = k_uptime_get() - start_ms;
	zassert_true(elapsed_ms < fixture.read_delay_ms,
		     "app.start scheduling waited %lld ms for delayed setup", elapsed_ms);

	wait_runtime_done(&runtime);
	zassert_true(runtime.status != SQ_VM_RUNTIME_RUNNING,
		     "worker remained running after async app.start setup");
	zassert_equal(sq_vm_runtime_wait_idle(&runtime, 250), 0);
	zassert_false(runtime.work_submitted);

	sq_vm_runtime_reset(&runtime);
}

ZTEST(squidscript_protocol, test_runtime_dispatch_slice_completes_one_storage_request)
{
	struct budgeted_vm_storage_fixture fixture = {
		.storage = {
			.sqbc = headless_counter_sqbc,
			.sqbc_len = sizeof(headless_counter_sqbc),
		},
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = budgeted_fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	static struct sq_vm_runtime runtime;
	bool complete = false;

	memset(&runtime, 0, sizeof(runtime));
	runtime.store_mount_point = test_fs_mount.mnt_point;
	strncpy(runtime.current_app, "headless-counter", sizeof(runtime.current_app) - 1);

	fixture.read_budget = SIZE_MAX;
	int result = sq_vm_runtime_dispatch_slice(&runtime, &backend, "app.start", 0, &complete);
	zassert_equal(result, 0, "dispatch_slice result %d status %d", result, runtime.status);
	zassert_false(complete);
	zassert_false(fixture.over_budget);
	size_t setup_reads = fixture.read_count;
	zassert_true(setup_reads > 0);

	fixture.read_budget = 1;
	result = sq_vm_runtime_dispatch_slice(&runtime, &backend, "app.start", 1, &complete);
	zassert_equal(result, 0, "dispatch_slice result %d status %d", result, runtime.status);
	zassert_false(fixture.over_budget);
	zassert_true(fixture.read_count <= setup_reads + 1);

	zassert_true(complete);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_IDLE);

	sq_vm_runtime_reset(&runtime);
}

ZTEST(squidscript_protocol, test_runtime_wait_idle_allows_immediate_consecutive_worker_starts)
{
	struct vm_storage_fixture fixture = {
		.sqbc = noop_sqbc,
		.sqbc_len = sizeof(noop_sqbc),
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	static struct sq_vm_runtime runtime;

	memset(&runtime, 0, sizeof(runtime));
	strncpy(runtime.current_app, "noop", sizeof(runtime.current_app) - 1);

	zassert_equal(sq_vm_runtime_start_event(&runtime, &backend,
						(const uint8_t *)"app.start",
						sizeof("app.start") - 1),
		      0);
	zassert_equal(sq_vm_runtime_wait_idle(&runtime, 1000), 0);
	zassert_false(runtime.work_submitted);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);

	zassert_equal(sq_vm_runtime_start_event(&runtime, &backend,
						(const uint8_t *)"app.start",
						sizeof("app.start") - 1),
		      0);
	zassert_equal(sq_vm_runtime_wait_idle(&runtime, 1000), 0);
	zassert_false(runtime.work_submitted);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);

	sq_vm_runtime_reset(&runtime);
}

ZTEST(squidscript_protocol, test_runtime_start_event_joins_completed_worker_before_reuse)
{
	struct vm_storage_fixture fixture = {
		.sqbc = noop_sqbc,
		.sqbc_len = sizeof(noop_sqbc),
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	static struct sq_vm_runtime runtime;

	memset(&runtime, 0, sizeof(runtime));
	strncpy(runtime.current_app, "noop", sizeof(runtime.current_app) - 1);

	zassert_equal(sq_vm_runtime_start_event(&runtime, &backend,
						(const uint8_t *)"app.start",
						sizeof("app.start") - 1),
		      0);
	for (int i = 0; i < 100 && runtime.status == SQ_VM_RUNTIME_RUNNING; i++) {
		k_sleep(K_MSEC(1));
	}
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);
	zassert_true(runtime.work_submitted);

	zassert_equal(sq_vm_runtime_start_event(&runtime, &backend,
						(const uint8_t *)"app.start",
						sizeof("app.start") - 1),
		      0);
	zassert_equal(sq_vm_runtime_wait_idle(&runtime, 1000), 0);
	zassert_false(runtime.work_submitted);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);

	sq_vm_runtime_reset(&runtime);
}

ZTEST(squidscript_protocol, test_vm_storage_adapter_completes_sqbc_and_state_requests)
{
	const uint8_t sqbc[] = {0x10, 0x20, 0x30, 0x40, 0x50};
	struct vm_storage_fixture fixture = {
		.sqbc = sqbc,
		.sqbc_len = sizeof(sqbc),
		.state = {0xaa, 0xbb, 0xcc},
		.state_len = 3,
		.state_present = true,
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	SqvmStorageCompletion completion = {0};
	SqvmStorageRequest request = {
		.kind = SQVM_STORAGE_REQUEST_SQBC_READ,
		.offset = 1,
		.len = 3,
	};

	zassert_equal(sq_vm_storage_complete_request(&backend, &request, &completion), 0);
	zassert_true(completion.has_len);
	zassert_equal(completion.len, 3);
	zassert_mem_equal(completion.bytes, &sqbc[1], 3);

	request = (SqvmStorageRequest){.kind = SQVM_STORAGE_REQUEST_STATE_LOAD};
	memset(&completion, 0, sizeof(completion));
	zassert_equal(sq_vm_storage_complete_request(&backend, &request, &completion), 0);
	zassert_true(completion.has_len);
	zassert_equal(completion.len, 3);
	zassert_mem_equal(completion.bytes, fixture.state, 3);

	request = (SqvmStorageRequest){.kind = SQVM_STORAGE_REQUEST_STATE_SAVE, .len = 2};
	request.bytes[0] = 0x7a;
	request.bytes[1] = 0x7b;
	memset(&completion, 0xff, sizeof(completion));
	zassert_equal(sq_vm_storage_complete_request(&backend, &request, &completion), 0);
	zassert_false(completion.has_len);
	zassert_equal(fixture.state_len, 2);
	zassert_mem_equal(fixture.state, request.bytes, 2);

	request = (SqvmStorageRequest){.kind = SQVM_STORAGE_REQUEST_STATE_RESET};
	zassert_equal(sq_vm_storage_complete_request(&backend, &request, &completion), 0);
	zassert_true(fixture.reset_called);
	zassert_false(fixture.state_present);
}

ZTEST(squidscript_protocol, test_vm_fs_storage_reads_sqbc_and_persists_state)
{
	const char *sqbc_path = "/sqtest/app.sqbc";
	const char *state_path = "/sqtest/app.state";
	const uint8_t sqbc[] = {0x10, 0x11, 0x12, 0x13, 0x14};
	const uint8_t saved_state[] = {0xa0, 0xa1, 0xa2, 0xa3};
	struct sq_vm_fs_storage storage = {
		.sqbc_path = sqbc_path,
		.state_path = state_path,
	};
	struct sq_vm_storage_backend backend = sq_vm_fs_storage_backend(&storage);
	SqvmStorageRequest request = {0};
	SqvmStorageCompletion completion = {0};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(unlink_test_file_if_exists(sqbc_path), 0);
	zassert_equal(unlink_test_file_if_exists(state_path), 0);
	zassert_equal(write_test_file(sqbc_path, sqbc, sizeof(sqbc)), 0);

	request = (SqvmStorageRequest){
		.kind = SQVM_STORAGE_REQUEST_SQBC_READ,
		.offset = 2,
		.len = 3,
	};
	zassert_equal(sq_vm_storage_complete_request(&backend, &request, &completion), 0);
	zassert_true(completion.has_len);
	zassert_equal(completion.len, 3);
	zassert_mem_equal(completion.bytes, &sqbc[2], 3);

	request = (SqvmStorageRequest){
		.kind = SQVM_STORAGE_REQUEST_STATE_SAVE,
		.len = sizeof(saved_state),
	};
	memcpy(request.bytes, saved_state, sizeof(saved_state));
	memset(&completion, 0, sizeof(completion));
	zassert_equal(sq_vm_storage_complete_request(&backend, &request, &completion), 0);
	zassert_false(completion.has_len);

	request = (SqvmStorageRequest){.kind = SQVM_STORAGE_REQUEST_STATE_LOAD};
	zassert_equal(sq_vm_storage_complete_request(&backend, &request, &completion), 0);
	zassert_true(completion.has_len);
	zassert_equal(completion.len, sizeof(saved_state));
	zassert_mem_equal(completion.bytes, saved_state, sizeof(saved_state));

	request = (SqvmStorageRequest){.kind = SQVM_STORAGE_REQUEST_STATE_RESET};
	zassert_equal(sq_vm_storage_complete_request(&backend, &request, &completion), 0);

	request = (SqvmStorageRequest){.kind = SQVM_STORAGE_REQUEST_STATE_LOAD};
	zassert_equal(sq_vm_storage_complete_request(&backend, &request, &completion), 0);
	zassert_false(completion.has_len);
	zassert_equal(completion.len, 0);

	zassert_equal(unlink_test_file_if_exists(sqbc_path), 0);
	zassert_equal(unlink_test_file_if_exists(state_path), 0);
	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_app_store_derives_vm_storage_paths_from_mount)
{
	struct sq_app_store_vm_storage app_storage = {0};
	struct sq_vm_storage_backend backend;
	struct fs_dirent entry;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);

	zassert_equal(fs_stat("/sqtest/apps", &entry), 0);
	zassert_equal(entry.type, FS_DIR_ENTRY_DIR);
	zassert_equal(fs_stat("/sqtest/state", &entry), 0);
	zassert_equal(entry.type, FS_DIR_ENTRY_DIR);

	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point,
						      "headless-counter", &app_storage),
		      0);
	zassert_str_equal(app_storage.sqbc_path, "/sqtest/apps/headless-counter/main.sqbc");
	zassert_str_equal(app_storage.state_path, "/sqtest/state/headless-counter.state");

	backend = sq_app_store_vm_storage_backend(&app_storage);
	zassert_not_null(backend.read_sqbc);
	zassert_not_null(backend.load_state);
	zassert_equal(backend.user_data, &app_storage.fs_storage);

	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point,
						      "../bad", &app_storage),
		      -EINVAL);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_installed_app_launch_reads_sqbc_in_bounded_file_chunks)
{
	enum { PADDED_SQBC_LEN = sizeof(headless_counter_sqbc) + SQVM_STORAGE_TRANSFER_CAPACITY };
	uint8_t padded_sqbc[PADDED_SQBC_LEN];
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_vm_storage_backend backend;
	static struct sq_vm_runtime runtime;

	memcpy(padded_sqbc, headless_counter_sqbc, sizeof(headless_counter_sqbc));
	memset(&padded_sqbc[sizeof(headless_counter_sqbc)], 0xa5,
	       sizeof(padded_sqbc) - sizeof(headless_counter_sqbc));
	padded_sqbc[6] = PADDED_SQBC_LEN & 0xff;
	padded_sqbc[7] = (PADDED_SQBC_LEN >> 8) & 0xff;
	padded_sqbc[8] = (PADDED_SQBC_LEN >> 16) & 0xff;
	padded_sqbc[9] = (PADDED_SQBC_LEN >> 24) & 0xff;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "padded", padded_sqbc,
					       sizeof(padded_sqbc)),
		      0);
	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point, "padded",
						      &launch_storage),
		      0);
	backend = sq_app_store_vm_storage_backend(&launch_storage);

	memset(&runtime, 0, sizeof(runtime));
	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), 0);
	zassert_equal(runtime.result_code, 0);
	zassert_equal(runtime.trace_count, 1);
	zassert_true(launch_storage.fs_storage.sqbc_read_count > 0);
	zassert_true(launch_storage.fs_storage.sqbc_max_read_len <= SQVM_STORAGE_TRANSFER_CAPACITY);
	zassert_true(launch_storage.fs_storage.sqbc_total_read_len < sizeof(padded_sqbc));
	zassert_equal(runtime.last_dispatch_sqbc_read_count,
		      launch_storage.fs_storage.sqbc_read_count);
	zassert_equal(runtime.last_dispatch_sqbc_read_bytes,
		      launch_storage.fs_storage.sqbc_total_read_len);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_app_store_installs_app_and_rebuilds_registry)
{
	const uint8_t sqbc_a[] = {0x53, 0x51, 0x42, 0x43, 0x01};
	const uint8_t sqbc_b[] = {0x53, 0x51, 0x42, 0x43, 0x02, 0x03};
	struct sq_app_registry registry = {0};
	struct fs_dirent entry;
	const struct sq_app_registry_entry *installed;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);

	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "alpha", sqbc_a,
					       sizeof(sqbc_a)),
		      0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "beta", sqbc_b,
					       sizeof(sqbc_b)),
		      0);

	zassert_equal(fs_stat("/sqtest/apps/alpha", &entry), 0);
	zassert_equal(entry.type, FS_DIR_ENTRY_DIR);
	zassert_equal(fs_stat("/sqtest/apps/alpha/main.sqbc", &entry), 0);
	zassert_equal(entry.type, FS_DIR_ENTRY_FILE);
	zassert_equal(entry.size, sizeof(sqbc_a));

	zassert_equal(sq_app_store_scan_registry(test_fs_mount.mnt_point, &registry), 0);
	zassert_true(registry.count >= 2);

	installed = sq_app_registry_find(&registry, "alpha");
	zassert_not_null(installed);
	zassert_str_equal(installed->app_id, "alpha");
	zassert_equal(installed->sqbc_len, sizeof(sqbc_a));

	installed = sq_app_registry_find(&registry, "beta");
	zassert_not_null(installed);
	zassert_str_equal(installed->app_id, "beta");
	zassert_equal(installed->sqbc_len, sizeof(sqbc_b));

	zassert_is_null(sq_app_registry_find(&registry, "../bad"));
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "../bad", sqbc_a,
					       sizeof(sqbc_a)),
		      -EINVAL);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_app_store_installs_and_resolves_package_resources)
{
	const uint8_t sqbc[] = {0x53, 0x51, 0x42, 0x43};
	const uint8_t resource[] = {0xde, 0xad, 0xbe, 0xef};
	char resource_path[SQ_APP_STORE_PATH_MAX];
	struct fs_dirent entry;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "resource-app", sqbc,
					       sizeof(sqbc)),
		      0);
	zassert_equal(sq_app_store_install_resource(test_fs_mount.mnt_point, "resource-app",
						    "icons/main.bin", resource,
						    sizeof(resource)),
		      0);
	zassert_equal(sq_app_store_resource_path(test_fs_mount.mnt_point, "resource-app",
						 "icons/main.bin", resource_path,
						 sizeof(resource_path)),
		      0);
	zassert_str_equal(resource_path, "/sqtest/apps/resource-app/resources/icons/main.bin");
	zassert_equal(fs_stat(resource_path, &entry), 0);
	zassert_equal(entry.type, FS_DIR_ENTRY_FILE);
	zassert_equal(entry.size, sizeof(resource));

	zassert_equal(sq_app_store_install_resource(test_fs_mount.mnt_point, "resource-app",
						    "../escape.bin", resource,
						    sizeof(resource)),
		      -EINVAL);
	zassert_equal(sq_app_store_resource_path(test_fs_mount.mnt_point, "resource-app",
						 "/absolute.bin", resource_path,
						 sizeof(resource_path)),
		      -EINVAL);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_vm_runtime_loads_package_sqdevice_resource_into_draft)
{
	const uint8_t sqbc[] = {0x53, 0x51, 0x42, 0x43};
	const uint8_t sqdevice[] = "SQDEVICE\n"
				   "service string 17:indicator.default\n"
				   "mode string 4:gpio\n"
				   "activeLow bool true\n";
	struct sq_vm_runtime runtime = {0};
	SqvmDeviceConfigResult result = {0};
	SqvmDeviceConfigValue value = {
		.kind = SQVM_DEVICE_CONFIG_VALUE_STRING,
		.string = (const uint8_t *)"GPIO8",
		.string_len = 5,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "device-config-app", sqbc,
					       sizeof(sqbc)),
		      0);
	zassert_equal(sq_app_store_install_resource(test_fs_mount.mnt_point, "device-config-app",
						    "device/indicator.sqdevice", sqdevice,
						    sizeof(sqdevice) - 1),
		      0);

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_set_store_mount_point(&runtime, test_fs_mount.mnt_point);
	strncpy(runtime.current_app, "device-config-app", sizeof(runtime.current_app) - 1);

	zassert_equal(sq_vm_runtime_device_config_load(
			      &runtime, (const uint8_t *)"package:device/indicator.sqdevice",
			      strlen("package:device/indicator.sqdevice"), &result),
		      0);
	zassert_true(result.ok);
	zassert_true(runtime.device_config_draft_loaded);
	zassert_equal(runtime.device_config_draft.count, 3);

	memset(&result, 0, sizeof(result));
	zassert_equal(sq_vm_runtime_device_config_set(&runtime, (const uint8_t *)"pinName",
						      strlen("pinName"), value, &result),
		      0);
	zassert_true(result.ok);
	zassert_equal(runtime.device_config_draft.records[3].value.kind, SQDC_VALUE_STRING);
	zassert_mem_equal(runtime.device_config_draft.records[3].value.string, "GPIO8", 5);

	memset(&result, 0, sizeof(result));
	zassert_equal(sq_vm_runtime_device_config_rebind(
			      &runtime, (const uint8_t *)"indicator.default",
			      strlen("indicator.default"), &result),
		      0);
	zassert_true(result.ok);
	zassert_true(runtime.indicator_binding_active);
	zassert_equal(runtime.indicator_binding_pin, 8);
	zassert_true(runtime.indicator_binding_active_low);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_vm_runtime_rejects_target_unknown_gpio_binding)
{
	struct sq_vm_runtime runtime = {0};
	SqvmDeviceConfigResult result = {0};
	SqvmDeviceConfigValue value = {
		.kind = SQVM_DEVICE_CONFIG_VALUE_STRING,
		.string = (const uint8_t *)"GPIO18",
		.string_len = strlen("GPIO18"),
	};

	sq_vm_runtime_init(&runtime);
	zassert_equal(sqdc_config_clear(&runtime.device_config_draft), SQDC_STATUS_OK);
	runtime.device_config_draft_loaded = true;
	zassert_equal(sqdc_config_set_string(&runtime.device_config_draft, (const uint8_t *)"service",
					     strlen("service"),
					     (const uint8_t *)"indicator.default",
					     strlen("indicator.default")),
		      SQDC_STATUS_OK);
	zassert_equal(sqdc_config_set_string(&runtime.device_config_draft, (const uint8_t *)"mode",
					     strlen("mode"), (const uint8_t *)"gpio",
					     strlen("gpio")),
		      SQDC_STATUS_OK);
	zassert_equal(sq_vm_runtime_device_config_set(&runtime, (const uint8_t *)"pinName",
						      strlen("pinName"), value, &result),
		      0);
	zassert_true(result.ok);
	memset(&result, 0, sizeof(result));
	zassert_equal(sqdc_config_set_bool(&runtime.device_config_draft,
					   (const uint8_t *)"activeLow", strlen("activeLow"),
					   false),
		      SQDC_STATUS_OK);

	zassert_equal(sq_vm_runtime_device_config_rebind(
			      &runtime, (const uint8_t *)"indicator.default",
			      strlen("indicator.default"), &result),
		      0);
	zassert_false(result.ok);
	zassert_mem_equal(result.error, "unsupported target gpio",
			  strlen("unsupported target gpio"));
	zassert_false(runtime.indicator_binding_active);
}

ZTEST(squidscript_protocol, test_vm_runtime_rebinds_display_device_config)
{
	struct sq_vm_runtime runtime = {0};
	SqvmDeviceConfigResult result = {0};

	sq_vm_runtime_init(&runtime);
	zassert_equal(sqdc_config_clear(&runtime.device_config_draft), SQDC_STATUS_OK);
	runtime.device_config_draft_loaded = true;
	zassert_equal(sqdc_config_set_string(&runtime.device_config_draft, (const uint8_t *)"service",
					     strlen("service"),
					     (const uint8_t *)"display.status",
					     strlen("display.status")),
		      SQDC_STATUS_OK);
	zassert_equal(sqdc_config_set_string(&runtime.device_config_draft, (const uint8_t *)"mode",
					     strlen("mode"), (const uint8_t *)"drawlog",
					     strlen("drawlog")),
		      SQDC_STATUS_OK);

	zassert_equal(sq_vm_runtime_device_config_rebind(
			      &runtime, (const uint8_t *)"display.status",
			      strlen("display.status"), &result),
		      0);
	zassert_true(result.ok);
	zassert_equal(runtime.active_binding_count, 1);
	zassert_true(runtime.active_bindings[0].active);
	zassert_str_equal(runtime.active_bindings[0].alias, "display.status");
}

ZTEST(squidscript_protocol, test_vm_runtime_resets_target_indicator_default_as_device_config)
{
	struct sq_vm_runtime runtime = {0};

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_reset(&runtime);

#if SQ_TARGET_INDICATOR_DEFAULT_HAS_GPIO
	zassert_true(runtime.device_config_draft_loaded);
	zassert_equal(runtime.device_config_draft.count, 4);
	zassert_true(runtime.indicator_binding_active);
	zassert_equal(runtime.indicator_binding_pin, SQ_TARGET_INDICATOR_DEFAULT_GPIO_PIN);
	zassert_equal(runtime.indicator_binding_active_low,
		      SQ_TARGET_INDICATOR_DEFAULT_ACTIVE_LOW != 0);
#else
	zassert_false(runtime.device_config_draft_loaded);
	zassert_false(runtime.indicator_binding_active);
#endif
}

ZTEST(squidscript_protocol, test_vm_runtime_rebuilds_indicator_default_on_app_start)
{
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage storage = {0};
	struct sq_vm_storage_backend backend;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "default-indicator-app",
					       headless_counter_sqbc,
					       sizeof(headless_counter_sqbc)),
		      0);
	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point,
						      "default-indicator-app", &storage),
		      0);
	backend = sq_app_store_vm_storage_backend(&storage);

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_set_store_mount_point(&runtime, test_fs_mount.mnt_point);
	strncpy(runtime.current_app, "default-indicator-app", sizeof(runtime.current_app) - 1);
	runtime.indicator_binding_active = true;
	runtime.indicator_binding_pin = 10;
	runtime.indicator_binding_active_low = false;

	zassert_equal(sq_vm_runtime_start(&runtime, &backend, "app.start"), 0);
	wait_runtime_done(&runtime);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);
	zassert_equal(runtime.result_code, 0);

#if SQ_TARGET_INDICATOR_DEFAULT_HAS_GPIO
	zassert_true(runtime.indicator_binding_active);
	zassert_equal(runtime.indicator_binding_pin, SQ_TARGET_INDICATOR_DEFAULT_GPIO_PIN);
	zassert_equal(runtime.indicator_binding_active_low,
		      SQ_TARGET_INDICATOR_DEFAULT_ACTIVE_LOW != 0);
#else
	zassert_false(runtime.indicator_binding_active);
#endif

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_vm_runtime_saves_device_config_draft_to_flash_sqdc)
{
	const uint8_t sqbc[] = {0x53, 0x51, 0x42, 0x43};
	const uint8_t sqdevice[] = "SQDEVICE\n"
				   "service string 17:indicator.default\n"
				   "mode string 4:gpio\n"
				   "pinName string 5:GPIO8\n"
				   "activeLow bool true\n";
	struct sq_vm_runtime runtime = {0};
	SqvmDeviceConfigResult result = {0};
	uint8_t saved[256];
	size_t saved_len = 0;
	SqdcConfig decoded = {0};
	char path[SQ_APP_STORE_PATH_MAX];

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "device-save-app", sqbc,
					       sizeof(sqbc)),
		      0);
	zassert_equal(sq_app_store_install_resource(test_fs_mount.mnt_point, "device-save-app",
						    "device/indicator.sqdevice", sqdevice,
						    sizeof(sqdevice) - 1),
		      0);

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_set_store_mount_point(&runtime, test_fs_mount.mnt_point);
	strncpy(runtime.current_app, "device-save-app", sizeof(runtime.current_app) - 1);

	zassert_equal(sq_vm_runtime_device_config_load(
			      &runtime, (const uint8_t *)"package:device/indicator.sqdevice",
			      strlen("package:device/indicator.sqdevice"), &result),
		      0);
	zassert_true(result.ok);

	memset(&result, 0, sizeof(result));
	zassert_equal(sq_vm_runtime_device_config_save(&runtime, (const uint8_t *)"flash",
						       strlen("flash"), &result),
		      0);
	zassert_true(result.ok);

	zassert_equal(sq_app_store_device_config_path(test_fs_mount.mnt_point, path, sizeof(path)),
		      0);
	zassert_equal(read_test_file(path, saved, sizeof(saved), &saved_len), 0);
	zassert_true(saved_len > 0);
	zassert_equal(sqdc_decode_sqdc(saved, saved_len, &decoded), SQDC_STATUS_OK);
	zassert_equal(decoded.count, 4);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_vm_runtime_persists_active_caps_to_flash_sqdc)
{
	struct sq_vm_runtime save_runtime = {0};
	struct sq_vm_runtime load_runtime = {0};
	uint8_t saved[256];
	size_t saved_len = 0;
	SqdcConfig decoded = {0};
	char path[SQ_APP_STORE_RUNTIME_CONFIG_PATH_MAX];
	uint16_t timer_cap = 0;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);

	sq_vm_runtime_init(&save_runtime);
	sq_vm_runtime_set_store_mount_point(&save_runtime, test_fs_mount.mnt_point);
	zassert_equal(sq_vm_runtime_cap_set(&save_runtime, "vm_runtime.timer_max", 2), 0);
	zassert_equal(sq_vm_runtime_cap_save(&save_runtime), 0);

	zassert_equal(sq_app_store_runtime_config_path(test_fs_mount.mnt_point, path,
						       sizeof(path)),
		      0);
	zassert_equal(read_test_file(path, saved, sizeof(saved), &saved_len), 0);
	zassert_true(saved_len > 0);
	zassert_equal(sqdc_decode_sqdc(saved, saved_len, &decoded), SQDC_STATUS_OK);
	zassert_equal(decoded.count, 1);
	zassert_mem_equal(decoded.records[0].key, "vm_runtime.timer_max",
			  strlen("vm_runtime.timer_max"));
	zassert_equal(decoded.records[0].value.kind, SQDC_VALUE_I32);
	zassert_equal(decoded.records[0].value.i32_value, 2);

	sq_vm_runtime_init(&load_runtime);
	sq_vm_runtime_set_store_mount_point(&load_runtime, test_fs_mount.mnt_point);
	zassert_equal(sq_vm_runtime_cap_load(&load_runtime), 0);
	zassert_equal(sq_vm_runtime_cap_get(&load_runtime, "vm_runtime.timer_max", &timer_cap),
		      0);
	zassert_equal(timer_cap, 2);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_vm_runtime_applies_saved_device_config_before_app_start)
{
	const uint8_t sqdevice[] = "SQDEVICE\n"
				   "service string 17:indicator.default\n"
				   "mode string 4:gpio\n"
				   "pinName string 5:GPIO8\n"
				   "activeLow bool true\n";
	struct sq_vm_runtime save_runtime = {0};
	struct sq_vm_runtime launch_runtime = {0};
	struct sq_app_store_vm_storage storage = {0};
	struct sq_vm_storage_backend backend;
	SqvmDeviceConfigResult result = {0};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "saved-default-app",
					       headless_counter_sqbc,
					       sizeof(headless_counter_sqbc)),
		      0);
	zassert_equal(sq_app_store_install_resource(test_fs_mount.mnt_point, "saved-default-app",
						    "device/indicator.sqdevice", sqdevice,
						    sizeof(sqdevice) - 1),
		      0);
	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point,
						      "saved-default-app", &storage),
		      0);
	backend = sq_app_store_vm_storage_backend(&storage);

	sq_vm_runtime_init(&save_runtime);
	sq_vm_runtime_set_store_mount_point(&save_runtime, test_fs_mount.mnt_point);
	strncpy(save_runtime.current_app, "saved-default-app", sizeof(save_runtime.current_app) - 1);
	zassert_equal(sq_vm_runtime_device_config_load(
			      &save_runtime, (const uint8_t *)"package:device/indicator.sqdevice",
			      strlen("package:device/indicator.sqdevice"), &result),
		      0);
	zassert_true(result.ok);
	memset(&result, 0, sizeof(result));
	zassert_equal(sq_vm_runtime_device_config_save(&save_runtime, (const uint8_t *)"flash",
						       strlen("flash"), &result),
		      0);
	zassert_true(result.ok);

	sq_vm_runtime_init(&launch_runtime);
	sq_vm_runtime_set_store_mount_point(&launch_runtime, test_fs_mount.mnt_point);
	strncpy(launch_runtime.current_app, "saved-default-app",
		sizeof(launch_runtime.current_app) - 1);
	zassert_equal(sq_vm_runtime_start(&launch_runtime, &backend, "app.start"), 0);
	wait_runtime_done(&launch_runtime);
	zassert_equal(launch_runtime.status, SQ_VM_RUNTIME_COMPLETE);
	zassert_equal(launch_runtime.result_code, 0);
	zassert_true(launch_runtime.indicator_binding_active);
	zassert_equal(launch_runtime.indicator_binding_pin, 8);
	zassert_true(launch_runtime.indicator_binding_active_low);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_vm_runtime_applies_packaged_device_binding_before_app_start)
{
	const uint8_t sqdevice[] = "SQDEVICE\n"
				   "service string 17:indicator.default\n"
				   "mode string 4:gpio\n"
				   "pinName string 5:GPIO8\n"
				   "activeLow bool false\n";
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage storage = {0};
	struct sq_vm_storage_backend backend;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "device-binding-app",
					       device_binding_app_sqbc,
					       sizeof(device_binding_app_sqbc)),
		      0);
	zassert_equal(sq_app_store_install_resource(test_fs_mount.mnt_point, "device-binding-app",
						    "device/indicator.sqdevice", sqdevice,
						    sizeof(sqdevice) - 1),
		      0);
	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point, "device-binding-app",
						      &storage),
		      0);
	backend = sq_app_store_vm_storage_backend(&storage);

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_set_store_mount_point(&runtime, test_fs_mount.mnt_point);
	strncpy(runtime.current_app, "device-binding-app", sizeof(runtime.current_app) - 1);

	zassert_equal(sq_vm_runtime_start(&runtime, &backend, "app.start"), 0);
	wait_runtime_done(&runtime);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);
	zassert_equal(runtime.result_code, 0);
	zassert_true(runtime.indicator_binding_active);
	zassert_equal(runtime.indicator_binding_pin, 8);
	zassert_false(runtime.indicator_binding_active_low);
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "binding ready");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_vm_runtime_applies_packaged_display_binding_before_app_start)
{
	const uint8_t sqdevice[] = "SQDEVICE\n"
				   "service string 14:display.status\n"
				   "mode string 7:drawlog\n";
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage storage = {0};
	struct sq_vm_storage_backend backend;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "display-binding-app",
					       display_binding_app_sqbc,
					       sizeof(display_binding_app_sqbc)),
		      0);
	zassert_equal(sq_app_store_install_resource(test_fs_mount.mnt_point, "display-binding-app",
						    "device/status-display.sqdevice", sqdevice,
						    sizeof(sqdevice) - 1),
		      0);
	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point,
						      "display-binding-app", &storage),
		      0);
	backend = sq_app_store_vm_storage_backend(&storage);

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_set_store_mount_point(&runtime, test_fs_mount.mnt_point);
	strncpy(runtime.current_app, "display-binding-app", sizeof(runtime.current_app) - 1);

	zassert_equal(sq_vm_runtime_start(&runtime, &backend, "app.start"), 0);
	wait_runtime_done(&runtime);
	zassert_true(runtime.status == SQ_VM_RUNTIME_COMPLETE,
		     "status=%d result=%d ffi_status=%d output_count=%u drawlog_count=%u output0=%s draw0=%s",
		     runtime.status, runtime.result_code, runtime.result.status,
		     runtime.output_count, runtime.drawlog_count, runtime.outputs[0],
		     runtime.drawlog[0]);
	zassert_equal(runtime.result_code, 0);
	zassert_true(runtime_has_active_binding(&runtime, "indicator.default"));
	zassert_true(runtime_has_active_binding(&runtime, "display.status"));
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "display binding ready");
	zassert_equal(runtime.drawlog_count, 1);
	zassert_str_equal(runtime.drawlog[0], "draw=select name=status");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_vm_runtime_applies_inline_gpio_device_binding_before_app_start)
{
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage storage = {0};
	struct sq_vm_storage_backend backend;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "inline-gpio-binding-app",
					       inline_gpio_binding_app_sqbc,
					       sizeof(inline_gpio_binding_app_sqbc)),
		      0);
	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point,
						      "inline-gpio-binding-app", &storage),
		      0);
	backend = sq_app_store_vm_storage_backend(&storage);

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_set_store_mount_point(&runtime, test_fs_mount.mnt_point);
	strncpy(runtime.current_app, "inline-gpio-binding-app", sizeof(runtime.current_app) - 1);

	zassert_equal(sq_vm_runtime_start(&runtime, &backend, "app.start"), 0);
	wait_runtime_done(&runtime);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);
	zassert_equal(runtime.result_code, 0);
	zassert_true(runtime.indicator_binding_active);
	zassert_equal(runtime.indicator_binding_pin, 8);
	zassert_false(runtime.indicator_binding_active_low);
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "inline binding ready");

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_vm_runtime_rebinds_inline_gpio_button_input)
{
	struct sq_vm_runtime runtime = {0};
	SqvmDeviceConfigResult result = {0};

	sq_vm_runtime_init(&runtime);
	zassert_equal(sqdc_config_clear(&runtime.device_config_draft), SQDC_STATUS_OK);
	runtime.device_config_draft_loaded = true;
	zassert_equal(sqdc_config_set_string(&runtime.device_config_draft, (const uint8_t *)"service",
					     strlen("service"), (const uint8_t *)"input.default",
					     strlen("input.default")),
		      SQDC_STATUS_OK);
	zassert_equal(sqdc_config_set_string(&runtime.device_config_draft, (const uint8_t *)"mode",
					     strlen("mode"), (const uint8_t *)"gpio-button",
					     strlen("gpio-button")),
		      SQDC_STATUS_OK);
	zassert_equal(sqdc_config_set_string(&runtime.device_config_draft, (const uint8_t *)"pinName",
					     strlen("pinName"), (const uint8_t *)"GPIO9",
					     strlen("GPIO9")),
		      SQDC_STATUS_OK);
	zassert_equal(sqdc_config_set_string(&runtime.device_config_draft, (const uint8_t *)"event",
					     strlen("event"), (const uint8_t *)"key.SELECT",
					     strlen("key.SELECT")),
		      SQDC_STATUS_OK);
	zassert_equal(sqdc_config_set_bool(&runtime.device_config_draft,
					   (const uint8_t *)"activeLow", strlen("activeLow"),
					   true),
		      SQDC_STATUS_OK);

	zassert_equal(sq_vm_runtime_device_config_rebind(
			      &runtime, (const uint8_t *)"input.default",
			      strlen("input.default"), &result),
		      0);
	zassert_true(result.ok);
	zassert_true(runtime_has_active_binding(&runtime, "input.default"));
	zassert_equal(runtime.input_button_count, 1);
	zassert_true(runtime.input_buttons[0].active);
	zassert_equal(runtime.input_buttons[0].pin, 9);
	zassert_true(runtime.input_buttons[0].active_low);
	zassert_str_equal(runtime.input_buttons[0].event, "key.SELECT");
}

ZTEST(squidscript_protocol, test_vm_runtime_rejects_invalid_gpio_button_input_binding)
{
	struct sq_vm_runtime runtime = {0};
	SqvmDeviceConfigResult result = {0};

	sq_vm_runtime_init(&runtime);
	zassert_equal(sqdc_config_clear(&runtime.device_config_draft), SQDC_STATUS_OK);
	runtime.device_config_draft_loaded = true;
	zassert_equal(sqdc_config_set_string(&runtime.device_config_draft, (const uint8_t *)"service",
					     strlen("service"), (const uint8_t *)"input.default",
					     strlen("input.default")),
		      SQDC_STATUS_OK);
	zassert_equal(sqdc_config_set_string(&runtime.device_config_draft, (const uint8_t *)"mode",
					     strlen("mode"), (const uint8_t *)"gpio-button",
					     strlen("gpio-button")),
		      SQDC_STATUS_OK);
	zassert_equal(sqdc_config_set_string(&runtime.device_config_draft, (const uint8_t *)"pinName",
					     strlen("pinName"), (const uint8_t *)"GPIO18",
					     strlen("GPIO18")),
		      SQDC_STATUS_OK);
	zassert_equal(sqdc_config_set_string(&runtime.device_config_draft, (const uint8_t *)"event",
					     strlen("event"), (const uint8_t *)"key.SELECT",
					     strlen("key.SELECT")),
		      SQDC_STATUS_OK);
	zassert_equal(sqdc_config_set_bool(&runtime.device_config_draft,
					   (const uint8_t *)"activeLow", strlen("activeLow"),
					   true),
		      SQDC_STATUS_OK);

	zassert_equal(sq_vm_runtime_device_config_rebind(
			      &runtime, (const uint8_t *)"input.default",
			      strlen("input.default"), &result),
		      0);
	zassert_false(result.ok);
	zassert_mem_equal(result.error, "unsupported target gpio",
			  strlen("unsupported target gpio"));
	zassert_equal(runtime.input_button_count, 0);
}

ZTEST(squidscript_protocol, test_vm_runtime_rejects_unsupported_packaged_gpio_as_unsupported)
{
	const uint8_t sqdevice[] = "SQDEVICE\n"
				   "service string 17:indicator.default\n"
				   "mode string 4:gpio\n"
				   "pinName string 6:GPIO18\n"
				   "activeLow bool false\n";
	struct sq_vm_runtime runtime = {0};
	struct sq_app_store_vm_storage storage = {0};
	struct sq_vm_storage_backend backend;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "unsupported-gpio-binding-app",
					       device_binding_app_sqbc,
					       sizeof(device_binding_app_sqbc)),
		      0);
	zassert_equal(sq_app_store_install_resource(test_fs_mount.mnt_point,
						    "unsupported-gpio-binding-app",
						    "device/indicator.sqdevice", sqdevice,
						    sizeof(sqdevice) - 1),
		      0);
	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point,
						      "unsupported-gpio-binding-app", &storage),
		      0);
	backend = sq_app_store_vm_storage_backend(&storage);

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_set_store_mount_point(&runtime, test_fs_mount.mnt_point);
	strncpy(runtime.current_app, "unsupported-gpio-binding-app", sizeof(runtime.current_app) - 1);

	zassert_equal(sq_vm_runtime_start(&runtime, &backend, "app.start"), 0);
	wait_runtime_done(&runtime);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_ERROR);
	zassert_equal(runtime.result_code, -ENOTSUP);
	zassert_true(runtime.indicator_binding_active);
	zassert_equal(runtime.indicator_binding_pin, 8);
	zassert_equal(runtime.output_count, 0);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_zephyr_calls_squidvm_ffi_with_storage_adapter)
{
	struct ffi_vm_fixture fixture = {
		.storage = {
			.sqbc = headless_counter_sqbc,
			.sqbc_len = sizeof(headless_counter_sqbc),
		},
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture.storage,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	SqvmCallbacks callbacks = {
		.trace = ffi_trace,
		.read_exact_at = ffi_read_exact_at,
	};
	SqvmDispatchResult result = {0};
	SqvmStorageCompletion completion = {0};
	zassert_true(sqvm_context_size() <= sizeof(ffi_context_storage));
	zassert_equal(sqvm_context_prepare(ffi_context_storage, sizeof(ffi_context_storage)),
		      SQVM_STATUS_OK);
	zassert_equal(sqvm_context_init_in_place(ffi_context_storage, &fixture, &callbacks, ffi_scratch,
						 sizeof(ffi_scratch)),
		      SQVM_STATUS_OK);

	zassert_equal(sqvm_dispatch_start_resumable(ffi_context_storage, &fixture, &callbacks,
						    (const uint8_t *)"app.start", 9, &result),
		      SQVM_STATUS_OK);

	while (result.outcome == SQVM_DISPATCH_PENDING_STORAGE) {
		zassert_equal(sq_vm_storage_complete_request(&backend, &result.storage, &completion),
			      0);
		zassert_equal(sqvm_dispatch_resume_storage(ffi_context_storage, &fixture, &callbacks,
							   &completion, &result),
			      SQVM_STATUS_OK);
	}

	zassert_equal(result.outcome, SQVM_DISPATCH_COMPLETE);
	zassert_equal(fixture.trace_count, 1,
		      "trace_count=%u trace0=%s trace1=%s trace2=%s trace3=%s",
		      fixture.trace_count, fixture.traces[0], fixture.traces[1],
		      fixture.traces[2], fixture.traces[3]);
	zassert_str_equal(fixture.traces[0], "app.start");
}

ZTEST(squidscript_protocol, test_zephyr_calls_squidvm_ffi_lifecycle_callbacks)
{
	struct ffi_vm_fixture fixture = {
		.storage = {
			.sqbc = lifecycle_callbacks_sqbc,
			.sqbc_len = sizeof(lifecycle_callbacks_sqbc),
		},
	};
	SqvmCallbacks callbacks = {
		.trace = ffi_trace,
		.read_exact_at = ffi_read_exact_at,
		.app_launch = ffi_app_launch,
		.app_arm = ffi_app_arm,
		.app_disarm = ffi_app_disarm,
	};
	SqvmDispatchResult result = {0};
	SqvmStorageCompletion completion = {0};

	zassert_true(sqvm_context_size() <= sizeof(ffi_context_storage));
	zassert_equal(sqvm_context_prepare(ffi_context_storage, sizeof(ffi_context_storage)),
		      SQVM_STATUS_OK);
	zassert_equal(sqvm_context_init_in_place(ffi_context_storage, &fixture, &callbacks, ffi_scratch,
						 sizeof(ffi_scratch)),
		      SQVM_STATUS_OK);

	zassert_equal(sqvm_dispatch_start_resumable(ffi_context_storage, &fixture, &callbacks,
						    (const uint8_t *)"repl", 4, &result),
		      SQVM_STATUS_OK);

	while (result.outcome == SQVM_DISPATCH_PENDING_STORAGE) {
		struct sq_vm_storage_backend backend = {
			.user_data = &fixture.storage,
			.read_sqbc = fixture_read_sqbc,
			.load_state = fixture_load_state,
			.save_state = fixture_save_state,
			.reset_state = fixture_reset_state,
		};
		zassert_equal(sq_vm_storage_complete_request(&backend, &result.storage, &completion),
			      0);
		zassert_equal(sqvm_dispatch_resume_storage(ffi_context_storage, &fixture, &callbacks,
							   &completion, &result),
			      SQVM_STATUS_OK);
	}

	zassert_equal(result.status, SQVM_STATUS_OK);
	zassert_equal(result.outcome, SQVM_DISPATCH_COMPLETE);
	zassert_equal(fixture.trace_count, 4,
		      "trace_count=%u trace0=%s trace1=%s trace2=%s trace3=%s",
		      fixture.trace_count, fixture.traces[0], fixture.traces[1],
		      fixture.traces[2], fixture.traces[3]);
	zassert_str_equal(fixture.traces[0], "repl");
	zassert_str_equal(fixture.traces[1], "arm break-reminder");
	zassert_str_equal(fixture.traces[2], "launch reader");
	zassert_str_equal(fixture.traces[3], "disarm break-reminder");
}

ZTEST(squidscript_protocol, test_vm_runtime_dispatches_app_start_and_records_trace)
{
	struct vm_storage_fixture fixture = {
		.sqbc = headless_counter_sqbc,
		.sqbc_len = sizeof(headless_counter_sqbc),
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	struct sq_vm_runtime runtime = {0};

	int result = sq_vm_runtime_dispatch(&runtime, &backend, "app.start");
	zassert_equal(result, 0, "dispatch result %d outcome %d status %d", result,
		      runtime.result.outcome, runtime.result.status);
	zassert_equal(runtime.trace_count, 1,
		      "trace_count=%u trace0=%s trace1=%s trace2=%s trace3=%s",
		      runtime.trace_count, runtime.traces[0], runtime.traces[1],
		      runtime.traces[2], runtime.traces[3]);
	zassert_str_equal(runtime.traces[0], "app.start");
}

ZTEST(squidscript_protocol, test_vm_runtime_dispatches_system_resource_callbacks)
{
	struct vm_storage_fixture fixture = {
		.sqbc = system_resources_sqbc,
		.sqbc_len = sizeof(system_resources_sqbc),
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	static struct sq_vm_runtime runtime;

	memset(&runtime, 0, sizeof(runtime));
	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), -EIO);
	zassert_equal(runtime.output_count, 1);
	zassert_true(strncmp(runtime.outputs[0], "system memory RAM ", strlen("system memory RAM ")) ==
			     0,
		     "memory output was %s", runtime.outputs[0]);

	memset(&runtime, 0, sizeof(runtime));
	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(format_test_app_store(), 0);
	sq_vm_runtime_set_store_mount_point(&runtime, test_fs_mount.mnt_point);
	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), 0);
	zassert_equal(runtime.output_count, 2);
	zassert_true(strncmp(runtime.outputs[0], "system memory RAM ", strlen("system memory RAM ")) ==
			     0,
		     "memory output was %s", runtime.outputs[0]);
	zassert_true(strncmp(runtime.outputs[1], "system apps Apps ", strlen("system apps Apps ")) ==
			     0,
		     "storage output was %s", runtime.outputs[1]);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_vm_runtime_dispatches_app_registry_callbacks)
{
	struct vm_storage_fixture fixture = {
		.sqbc = app_registry_summary_sqbc,
		.sqbc_len = sizeof(app_registry_summary_sqbc),
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	struct sq_app_registry registry = {
		.count = 2,
		.apps = {
			{.app_id = "alpha", .sqbc_len = 5},
			{.app_id = "beta", .sqbc_len = 6},
		},
	};
	static struct sq_vm_runtime runtime;

	memset(&runtime, 0, sizeof(runtime));
	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), -EIO);
	zassert_equal(runtime.output_count, 0);

	memset(&runtime, 0, sizeof(runtime));
	sq_vm_runtime_set_registry(&runtime, &registry);
	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), 0);
	zassert_equal(runtime.output_count, 4);
	zassert_str_equal(runtime.outputs[0], "registry app alpha");
	zassert_str_equal(runtime.outputs[1], "registry app beta");
	zassert_str_equal(runtime.outputs[2], "registry selected id alpha");
	zassert_str_equal(runtime.outputs[3], "registry selected name alpha");
}

ZTEST(squidscript_protocol, test_vm_runtime_dispatches_stack_inspection_callbacks)
{
	struct vm_storage_fixture fixture = {
		.sqbc = stack_inspect_sqbc,
		.sqbc_len = sizeof(stack_inspect_sqbc),
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	static struct sq_vm_runtime runtime;

	memset(&runtime, 0, sizeof(runtime));
	strcpy(runtime.return_stack[0], "launcher");
	strcpy(runtime.return_stack[1], "parent");
	runtime.return_stack_count = 2;
	runtime.armed_timers[0] = (struct sq_vm_runtime_armed_timer){
		.active = true,
		.app_id = "break-reminder",
		.event = "timer.break",
	};
	runtime.armed_timers[1] = (struct sq_vm_runtime_armed_timer){
		.active = true,
		.app_id = "reader-clock",
		.event = "timer.clock",
	};
	runtime.armed_timer_count = 2;

	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), 0);
	zassert_equal(runtime.output_count, 5);
	zassert_str_equal(runtime.outputs[0], "process launcher");
	zassert_str_equal(runtime.outputs[1], "process parent");
	zassert_str_equal(runtime.outputs[2], "armed break-reminder timer.break");
	zassert_str_equal(runtime.outputs[3], "armed reader-clock timer.clock");
	zassert_str_equal(runtime.outputs[4], "selected reader-clock timer.clock");
}

ZTEST(squidscript_protocol, test_vm_runtime_dispatches_display_drawlog_callbacks)
{
	struct vm_storage_fixture fixture = {
		.sqbc = display_drawlog_sqbc,
		.sqbc_len = sizeof(display_drawlog_sqbc),
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	static struct sq_vm_runtime runtime;

	memset(&runtime, 0, sizeof(runtime));
	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), 0);
	zassert_equal(runtime.drawlog_count, 3);
	zassert_str_equal(runtime.drawlog[0], "draw=clear color=gray0");
	zassert_str_equal(runtime.drawlog[1], "draw=select name=status");
	zassert_str_equal(runtime.drawlog[2], "draw=image path=\"data/icon.bmp\" x=20 y=24");

	fixture.sqbc = display_primitives_sqbc;
	fixture.sqbc_len = sizeof(display_primitives_sqbc);
	memset(&runtime, 0, sizeof(runtime));
	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), 0);
	zassert_equal(runtime.drawlog_count, 4);
	zassert_str_equal(runtime.drawlog[0], "draw=clear color=gray0");
	zassert_str_equal(runtime.drawlog[1], "draw=text text=\"Hello\" x=10 y=20");
	zassert_str_equal(runtime.drawlog[2], "draw=rect x=1 y=2 w=3 h=4");
	zassert_str_equal(runtime.drawlog[3], "draw=line x1=5 y1=6 x2=7 y2=8");
}

ZTEST(squidscript_protocol, test_vm_runtime_records_physical_display_clear_and_text_ops)
{
	static struct sq_vm_runtime runtime;
	const SqvmDisplayTextOptions text_options = {
		.x = 10,
		.y = 20,
		.font_height = 24,
	};

	memset(&runtime, 0, sizeof(runtime));
	runtime_display_clear(&runtime, (const uint8_t *)"white", strlen("white"));
	runtime_display_text(&runtime, (const uint8_t *)"Hello", strlen("Hello"),
			     &text_options);

	zassert_equal(runtime.drawlog_count, 2);
	zassert_str_equal(runtime.drawlog[0], "draw=clear color=white");
	zassert_str_equal(runtime.drawlog[1], "draw=text text=\"Hello\" x=10 y=20");
	zassert_true(runtime.display_dirty);
	zassert_equal(runtime.display_op_count, 2);
	zassert_equal(runtime.display_ops[0].kind, SQ_VM_RUNTIME_DISPLAY_OP_CLEAR);
	zassert_str_equal(runtime.display_ops[0].text, "white");
	zassert_equal(runtime.display_ops[1].kind, SQ_VM_RUNTIME_DISPLAY_OP_TEXT);
	zassert_str_equal(runtime.display_ops[1].text, "Hello");
	zassert_equal(runtime.display_ops[1].x, 10);
	zassert_equal(runtime.display_ops[1].y, 20);
	zassert_equal(runtime.display_ops[1].font_height, 24);
}

ZTEST(squidscript_protocol, test_vm_runtime_dispatches_binbook_resource_drawable)
{
	uint8_t book[TEST_BINBOOK_LEN];
	struct sq_app_store_vm_storage app_storage = {0};
	struct sq_vm_storage_backend backend;
	static struct sq_vm_runtime runtime;

	build_test_binbook(book);
	zassert_equal(mount_test_fs(), 0);
	zassert_equal(format_test_app_store(), 0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "binbook-reader",
					       binbook_reader_sqbc, sizeof(binbook_reader_sqbc)),
		      0);
	zassert_equal(sq_app_store_install_resource(test_fs_mount.mnt_point, "binbook-reader",
						    "books/sample.binbook", book, sizeof(book)),
		      0);

	memset(&runtime, 0, sizeof(runtime));
	zassert_equal(sq_app_store_vm_storage_for_app(test_fs_mount.mnt_point, "binbook-reader",
						      &app_storage),
		      0);
	backend = sq_app_store_vm_storage_backend(&app_storage);
	sq_vm_runtime_set_store_mount_point(&runtime, test_fs_mount.mnt_point);
	strncpy(runtime.current_app, "binbook-reader", sizeof(runtime.current_app) - 1);

	zassert_equal(sq_vm_runtime_start(&runtime, &backend, "app.start"), 0);
	wait_runtime_done(&runtime);
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "pages 1");
	zassert_equal(runtime.drawlog_count, 1);
	zassert_str_equal(runtime.drawlog[0], "draw=binbook id=1 x=0 y=0");
	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_vm_runtime_records_binbook_drawable_display_op)
{
	static struct sq_vm_runtime runtime;
	const SqvmDisplayResourceOptions options = {0};
	const SqvmHandle drawable = {
		.kind = SQVM_HANDLE_DRAWABLE,
		.id = 1,
	};

	memset(&runtime, 0, sizeof(runtime));
	runtime.drawable.active = true;
	runtime.drawable.page = (struct sq_vm_runtime_binbook_page){
		.blob_offset = 412,
		.compressed_size = 4,
		.uncompressed_size = 96000,
		.page_index = 0,
		.pixel_format = 2,
		.compression_method = 1,
		.stored_width = 800,
		.stored_height = 480,
	};
	strncpy(runtime.drawable.page.path, "/sqtest/apps/binbook-reader/resources/books/sample.binbook",
		sizeof(runtime.drawable.page.path) - 1);

	runtime_display_draw(&runtime, drawable, &options);

	zassert_equal(runtime.drawlog_count, 1);
	zassert_str_equal(runtime.drawlog[0], "draw=binbook id=1 x=0 y=0");
	zassert_true(runtime.display_dirty);
	zassert_equal(runtime.display_op_count, 1);
	zassert_equal(runtime.display_ops[0].kind, SQ_VM_RUNTIME_DISPLAY_OP_BINBOOK_DRAWABLE);
	zassert_str_equal(runtime.display_ops[0].binbook_page.path,
			  "/sqtest/apps/binbook-reader/resources/books/sample.binbook");
	zassert_equal(runtime.display_ops[0].binbook_page.blob_offset, 412);
	zassert_equal(runtime.display_ops[0].binbook_page.compressed_size, 4);
	zassert_equal(runtime.display_ops[0].binbook_page.uncompressed_size, 96000);
	zassert_equal(runtime.display_ops[0].binbook_page.stored_width, 800);
	zassert_equal(runtime.display_ops[0].binbook_page.stored_height, 480);
}

ZTEST(squidscript_protocol, test_vm_runtime_dispatches_wifi_action_stubs)
{
	struct vm_storage_fixture fixture = {
		.sqbc = wifi_actions_sqbc,
		.sqbc_len = sizeof(wifi_actions_sqbc),
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	static struct sq_vm_runtime runtime;

	memset(&runtime, 0, sizeof(runtime));
	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), 0);
	zassert_equal(runtime.output_count, 4);
	zassert_str_equal(runtime.outputs[0], "true null");
	zassert_str_equal(runtime.outputs[1], "unsupported");
	zassert_str_equal(runtime.outputs[2],
			  "false unsupported false unsupported false unsupported");
	zassert_str_equal(runtime.outputs[3], "idle false unsupported false unsupported idle");
}

ZTEST(squidscript_protocol, test_vm_runtime_dispatches_file_pick_file_unsupported_result)
{
	struct vm_storage_fixture fixture = {
		.sqbc = file_pick_file_sqbc,
		.sqbc_len = sizeof(file_pick_file_sqbc),
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	static struct sq_vm_runtime runtime;

	memset(&runtime, 0, sizeof(runtime));
	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), 0);
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "false unsupported null");
}

ZTEST(squidscript_protocol, test_vm_runtime_dispatches_file_read_unsupported_results)
{
	struct vm_storage_fixture fixture = {
		.sqbc = file_read_sqbc,
		.sqbc_len = sizeof(file_read_sqbc),
	};
	struct sq_vm_storage_backend backend = {
		.user_data = &fixture,
		.read_sqbc = fixture_read_sqbc,
		.load_state = fixture_load_state,
		.save_state = fixture_save_state,
		.reset_state = fixture_reset_state,
	};
	static struct sq_vm_runtime runtime;

	memset(&runtime, 0, sizeof(runtime));
	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), 0);
	zassert_equal(runtime.output_count, 2);
	zassert_str_equal(runtime.outputs[0], "false unsupported null");
	zassert_str_equal(runtime.outputs[1], "false unsupported <list>");
}

ZTEST(squidscript_protocol, test_vm_runtime_formats_wifi_bssid_without_heap)
{
	const uint8_t mac[] = {0x02, 0x34, 0xab, 0xcd, 0xef, 0x10};
	char bssid[SQ_VM_RUNTIME_WIFI_BSSID_LEN];

	zassert_equal(sq_vm_runtime_wifi_format_bssid(mac, sizeof(mac), bssid, sizeof(bssid)), 0);
	zassert_str_equal(bssid, "02:34:ab:cd:ef:10");
	zassert_equal(sq_vm_runtime_wifi_format_bssid(mac, 5, bssid, sizeof(bssid)), -EINVAL);
	zassert_equal(sq_vm_runtime_wifi_format_bssid(mac, sizeof(mac), bssid, 17), -ENOSPC);
}

ZTEST(squidscript_protocol, test_vm_runtime_tracks_wifi_ap_client_count)
{
	static struct sq_vm_runtime runtime;

	sq_vm_runtime_reset(&runtime);

	sq_vm_runtime_wifi_note_ap_sta_connected(&runtime);
	zassert_equal(runtime.wifi_ap_clients, 1);

	sq_vm_runtime_wifi_note_ap_sta_connected(&runtime);
	zassert_equal(runtime.wifi_ap_clients, 2);

	sq_vm_runtime_wifi_note_ap_sta_disconnected(&runtime);
	zassert_equal(runtime.wifi_ap_clients, 1);

	sq_vm_runtime_wifi_note_ap_sta_disconnected(&runtime);
	zassert_equal(runtime.wifi_ap_clients, 0);

	sq_vm_runtime_wifi_note_ap_sta_disconnected(&runtime);
	zassert_equal(runtime.wifi_ap_clients, 0);
}

ZTEST(squidscript_protocol, test_vm_runtime_tracks_wifi_service_state_transitions)
{
	static struct sq_vm_runtime runtime;

	sq_vm_runtime_reset(&runtime);
	zassert_equal(runtime.wifi_service_state, SQ_VM_RUNTIME_WIFI_SERVICE_IDLE);
	zassert_str_equal(sq_vm_runtime_wifi_service_state_text(runtime.wifi_service_state),
			  "idle");

	sq_vm_runtime_wifi_service_begin(&runtime, SQ_VM_RUNTIME_WIFI_OP_SCAN,
					 SQ_VM_RUNTIME_WIFI_SERVICE_SCANNING, 1000);
	zassert_equal(runtime.wifi_service_state, SQ_VM_RUNTIME_WIFI_SERVICE_SCANNING);
	zassert_equal(runtime.wifi_op_kind, SQ_VM_RUNTIME_WIFI_OP_SCAN);
	zassert_true(runtime.wifi_op_active);
	zassert_false(runtime.wifi_op_done);
	zassert_str_equal(sq_vm_runtime_wifi_service_state_text(runtime.wifi_service_state),
			  "scanning");

	sq_vm_runtime_wifi_service_finish(&runtime, SQ_VM_RUNTIME_WIFI_SERVICE_IDLE, true, NULL);
	zassert_equal(runtime.wifi_service_state, SQ_VM_RUNTIME_WIFI_SERVICE_IDLE);
	zassert_true(runtime.wifi_op_done);
	zassert_true(runtime.wifi_op_ok);

	sq_vm_runtime_wifi_service_begin(&runtime, SQ_VM_RUNTIME_WIFI_OP_CONNECT,
					 SQ_VM_RUNTIME_WIFI_SERVICE_CONNECTING, 1000);
	sq_vm_runtime_wifi_service_finish(&runtime, SQ_VM_RUNTIME_WIFI_SERVICE_ERROR, false,
					  "connect failed");
	zassert_equal(runtime.wifi_service_state, SQ_VM_RUNTIME_WIFI_SERVICE_ERROR);
	zassert_true(runtime.wifi_op_done);
	zassert_false(runtime.wifi_op_ok);
	zassert_str_equal(runtime.wifi_op_error, "connect failed");
	zassert_str_equal(sq_vm_runtime_wifi_service_state_text(runtime.wifi_service_state),
			  "error");
}

ZTEST(squidscript_protocol, test_vm_runtime_reset_runs_wifi_target_cleanup_before_clearing_state)
{
	static struct sq_vm_runtime runtime;

	sq_vm_runtime_reset(&runtime);
	reset_wifi_reset_platform_observer();

	sq_vm_runtime_wifi_service_begin(&runtime, SQ_VM_RUNTIME_WIFI_OP_DISCONNECT,
					 SQ_VM_RUNTIME_WIFI_SERVICE_DISCONNECTING, 1000);
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	runtime.wifi_ap_active = true;
#endif
	sq_vm_runtime_reset(&runtime);

	zassert_equal(test_wifi_reset_platform_calls, 1);
	zassert_equal(test_wifi_reset_platform_kind, SQ_VM_RUNTIME_WIFI_OP_DISCONNECT);
	zassert_equal(test_wifi_reset_platform_state, SQ_VM_RUNTIME_WIFI_SERVICE_DISCONNECTING);
#if SQ_VM_RUNTIME_HAS_WIFI_MGMT
	zassert_true(test_wifi_reset_platform_ap_active);
#endif
	zassert_equal(runtime.wifi_service_state, SQ_VM_RUNTIME_WIFI_SERVICE_IDLE);
	zassert_equal(runtime.wifi_op_kind, SQ_VM_RUNTIME_WIFI_OP_NONE);
	zassert_false(runtime.wifi_op_active);
	zassert_false(runtime.wifi_op_done);
}

ZTEST(squidscript_protocol, test_sqdc_ffi_parses_and_encodes_device_config)
{
	const uint8_t source[] =
		"SQDEVICE\n"
		"service string 17:indicator.default\n"
		"backend string 4:gpio\n"
		"activeLow bool false\n"
		"pin int 8\n";
	SqdcConfig config = {0};
	SqdcConfig decoded = {0};
	uint8_t encoded[256];
	size_t encoded_len = 0;

	zassert_equal(sqdc_parse_sqdevice(source, strlen((const char *)source), &config),
		      SQDC_STATUS_OK);
	zassert_equal(config.count, 4);
	zassert_equal(sqdc_config_set_string(&config, (const uint8_t *)"pinName",
					     strlen("pinName"), (const uint8_t *)"GPIO8",
					     strlen("GPIO8")),
		      SQDC_STATUS_OK);
	zassert_equal(sqdc_config_set_bool(&config, (const uint8_t *)"activeLow",
					   strlen("activeLow"), true),
		      SQDC_STATUS_OK);
	zassert_equal(sqdc_encode_sqdc(&config, encoded, sizeof(encoded), &encoded_len),
		      SQDC_STATUS_OK);
	zassert_mem_equal(encoded, "SQDC", 4);
	zassert_equal(sqdc_decode_sqdc(encoded, encoded_len, &decoded), SQDC_STATUS_OK);
	zassert_equal(decoded.count, config.count);
	zassert_equal(sqdc_is_safe_sqdevice_path((const uint8_t *)"device/indicator.sqdevice",
						 strlen("device/indicator.sqdevice")),
		      SQDC_STATUS_OK);
	zassert_equal(sqdc_is_safe_sqdevice_path((const uint8_t *)"../indicator.sqdevice",
						 strlen("../indicator.sqdevice")),
		      SQDC_STATUS_INVALID_ARGUMENT);
}

ZTEST(squidscript_protocol, test_sqdc_ffi_plans_device_binding_resources)
{
	SqdcDeviceBindingPlan plan = {0};
	SqdcConfig inline_config = {0};

	zassert_equal(sqdc_plan_device_binding((const uint8_t *)"indicator",
					       strlen("indicator"), (const uint8_t *)"default",
					       strlen("default"), (const uint8_t *)"gpio:GPIO8",
					       strlen("gpio:GPIO8"), &plan, &inline_config),
		      SQDC_STATUS_OK);
	zassert_equal(plan.kind, SQDC_DEVICE_BINDING_RESOURCE_INLINE_GPIO);
	zassert_equal(plan.alias_len, strlen("indicator.default"));
	zassert_mem_equal(plan.alias, "indicator.default", strlen("indicator.default"));
	zassert_equal(inline_config.count, 4);
	zassert_equal(inline_config.records[2].value.kind, SQDC_VALUE_STRING);
	zassert_mem_equal(inline_config.records[2].value.string, "GPIO8", strlen("GPIO8"));
	zassert_false(inline_config.records[3].value.bool_value);

	memset(&plan, 0, sizeof(plan));
	memset(&inline_config, 0, sizeof(inline_config));
	zassert_equal(sqdc_plan_device_binding((const uint8_t *)"indicator",
					       strlen("indicator"), (const uint8_t *)"default",
					       strlen("default"),
					       (const uint8_t *)"device/indicator.sqdevice",
					       strlen("device/indicator.sqdevice"), &plan,
					       &inline_config),
		      SQDC_STATUS_OK);
	zassert_equal(plan.kind, SQDC_DEVICE_BINDING_RESOURCE_PACKAGE_SQDEVICE);
	zassert_equal(plan.resource_len, strlen("device/indicator.sqdevice"));
	zassert_mem_equal(plan.resource, "device/indicator.sqdevice",
			  strlen("device/indicator.sqdevice"));
	zassert_equal(inline_config.count, 0);

	memset(&plan, 0, sizeof(plan));
	memset(&inline_config, 0, sizeof(inline_config));
	zassert_equal(sqdc_plan_device_binding((const uint8_t *)"display", strlen("display"),
					       (const uint8_t *)"status", strlen("status"),
					       (const uint8_t *)"device/display.sqdevice",
					       strlen("device/display.sqdevice"), &plan,
					       &inline_config),
		      SQDC_STATUS_OK);
	zassert_equal(plan.kind, SQDC_DEVICE_BINDING_RESOURCE_PACKAGE_SQDEVICE);
	zassert_equal(plan.alias_len, strlen("display.status"));
	zassert_mem_equal(plan.alias, "display.status", strlen("display.status"));
	zassert_equal(inline_config.count, 0);

	memset(&plan, 0, sizeof(plan));
	memset(&inline_config, 0, sizeof(inline_config));
	zassert_equal(sqdc_plan_device_binding((const uint8_t *)"input", strlen("input"),
					       (const uint8_t *)"default", strlen("default"),
					       (const uint8_t *)"gpio-button:GPIO9:key.SELECT:activeLow",
					       strlen("gpio-button:GPIO9:key.SELECT:activeLow"),
					       &plan, &inline_config),
		      SQDC_STATUS_OK);
	zassert_equal(plan.kind, SQDC_DEVICE_BINDING_RESOURCE_INLINE_GPIO_BUTTON);
	zassert_equal(plan.alias_len, strlen("input.default"));
	zassert_mem_equal(plan.alias, "input.default", strlen("input.default"));
	zassert_equal(inline_config.count, 5);
	zassert_equal(inline_config.records[2].value.kind, SQDC_VALUE_STRING);
	zassert_mem_equal(inline_config.records[2].value.string, "GPIO9", strlen("GPIO9"));
	zassert_equal(inline_config.records[3].value.kind, SQDC_VALUE_STRING);
	zassert_mem_equal(inline_config.records[3].value.string, "key.SELECT", strlen("key.SELECT"));
	zassert_true(inline_config.records[4].value.bool_value);
	zassert_true((SQ_TARGET_GPIO_CAPABLE_MASK & (1ULL << 9)) != 0ULL);

	memset(&plan, 0, sizeof(plan));
	memset(&inline_config, 0, sizeof(inline_config));
	zassert_equal(sqdc_plan_device_binding((const uint8_t *)"input", strlen("input"),
					       (const uint8_t *)"default", strlen("default"),
					       (const uint8_t *)"gpio-button:GPIO9:key.BOOT:activeLow",
					       strlen("gpio-button:GPIO9:key.BOOT:activeLow"),
					       &plan, &inline_config),
		      SQDC_STATUS_INVALID_ARGUMENT);

	zassert_equal(sqdc_plan_device_binding((const uint8_t *)"sensor", strlen("sensor"),
					       (const uint8_t *)"default", strlen("default"),
					       (const uint8_t *)"device/sensor.sqdevice",
					       strlen("device/sensor.sqdevice"), &plan,
					       &inline_config),
		      SQDC_STATUS_INVALID_ARGUMENT);
}

ZTEST(squidscript_protocol, test_vm_runtime_tracks_output_indicator_and_due_timers)
{
	struct sq_vm_runtime runtime = {0};
	char event[SQ_VM_RUNTIME_EVENT_LEN];

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_reset(&runtime);

	zassert_equal(sq_vm_runtime_record_output(&runtime, (const uint8_t *)"hello", 5), 0);
	zassert_equal(runtime.output_count, 1);
	zassert_str_equal(runtime.outputs[0], "hello");

	zassert_equal(sq_vm_runtime_indicator_write(&runtime, true), 0);
	bool value = false;
	zassert_equal(sq_vm_runtime_indicator_read(&runtime, &value), 0);
	zassert_true(value);
	zassert_equal(sq_vm_runtime_indicator_toggle(&runtime), 0);
	zassert_equal(sq_vm_runtime_indicator_read(&runtime, &value), 0);
	zassert_false(value);

	zassert_equal(sq_vm_runtime_indicator_breathe(&runtime), 0);
	zassert_equal(runtime.indicator_pattern, SQ_VM_RUNTIME_INDICATOR_BREATHE);
	uint8_t first_step = runtime.indicator_pattern_step;
	runtime.indicator_pattern_next_ms = k_uptime_get() - 1;
	zassert_equal(sq_vm_runtime_poll(&runtime), 0);
	zassert_equal(runtime.indicator_pattern, SQ_VM_RUNTIME_INDICATOR_BREATHE);
	zassert_not_equal(runtime.indicator_pattern_step, first_step);

	zassert_equal(sq_vm_runtime_indicator_blink(&runtime, 10, 20), 0);
	zassert_equal(runtime.indicator_pattern, SQ_VM_RUNTIME_INDICATOR_BLINK);
	zassert_true(runtime.indicator_pattern_on);
	zassert_true(runtime.indicator_state);
	zassert_equal(runtime.indicator_pattern_on_ms, 10);
	zassert_equal(runtime.indicator_pattern_off_ms, 20);
	runtime.indicator_pattern_next_ms = k_uptime_get() - 1;
	zassert_equal(sq_vm_runtime_poll(&runtime), 0);
	zassert_equal(runtime.indicator_pattern, SQ_VM_RUNTIME_INDICATOR_BLINK);
	zassert_false(runtime.indicator_pattern_on);
	zassert_false(runtime.indicator_state);
	runtime.indicator_pattern_next_ms = k_uptime_get() - 1;
	zassert_equal(sq_vm_runtime_poll(&runtime), 0);
	zassert_true(runtime.indicator_pattern_on);
	zassert_true(runtime.indicator_state);

	zassert_equal(sq_vm_runtime_indicator_write(&runtime, true), 0);
	zassert_equal(runtime.indicator_pattern, SQ_VM_RUNTIME_INDICATOR_STEADY);

	zassert_equal(sq_vm_runtime_indicator_breathe(&runtime), 0);
	zassert_equal(runtime.indicator_pattern, SQ_VM_RUNTIME_INDICATOR_BREATHE);
	zassert_equal(sq_vm_runtime_hardware_gpio_write(&runtime, (const uint8_t *)"GPIO8",
							strlen("GPIO8"), true),
		      0);
	zassert_equal(sq_vm_runtime_hardware_gpio_read(&runtime, (const uint8_t *)"GPIO8",
						       strlen("GPIO8"), &value),
		      0);
	zassert_true(value);
	zassert_equal(sq_vm_runtime_indicator_breathe(&runtime), 0);
	zassert_equal(runtime.indicator_pattern, SQ_VM_RUNTIME_INDICATOR_BREATHE);
	zassert_equal(sq_vm_runtime_hardware_gpio_toggle(&runtime, (const uint8_t *)"GPIO8",
							 strlen("GPIO8")),
		      0);
	zassert_equal(sq_vm_runtime_hardware_gpio_read(&runtime, (const uint8_t *)"GPIO8",
						       strlen("GPIO8"), &value),
		      0);
	zassert_false(value);

	zassert_equal(sq_vm_runtime_register_timer(&runtime, (const uint8_t *)"timer.debug",
						   strlen("timer.debug"), 1, true),
		      0);
	k_sleep(K_MSEC(2));
	zassert_equal(sq_vm_runtime_next_due_timer(&runtime, event, sizeof(event)), 0);
	zassert_str_equal(event, "timer.debug");
	zassert_not_equal(sq_vm_runtime_next_due_timer(&runtime, event, sizeof(event)), 0);
}

ZTEST(squidscript_protocol, test_indicator_pattern_state_machine_transitions)
{
	struct sq_vm_runtime runtime = {0};

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_reset(&runtime);
	zassert_equal(runtime.indicator_pattern, SQ_VM_RUNTIME_INDICATOR_STEADY);

	zassert_equal(sq_vm_runtime_indicator_breathe(&runtime), 0);
	zassert_equal(runtime.indicator_pattern, SQ_VM_RUNTIME_INDICATOR_BREATHE);
	zassert_equal(runtime.indicator_pattern_step, 0);

	zassert_equal(sq_vm_runtime_indicator_blink(&runtime, 10, 20), 0);
	zassert_equal(runtime.indicator_pattern, SQ_VM_RUNTIME_INDICATOR_BLINK);
	zassert_true(runtime.indicator_pattern_on);

	zassert_equal(sq_vm_runtime_indicator_write(&runtime, false), 0);
	zassert_equal(runtime.indicator_pattern, SQ_VM_RUNTIME_INDICATOR_STEADY);
	zassert_false(runtime.indicator_state);
}

ZTEST(squidscript_protocol, test_device_protocol_poll_advances_running_runtime_poll)
{
	struct sq_vm_runtime runtime = {0};
	struct sq_device_protocol_context context = {
		.runtime = &runtime,
	};

	sq_vm_runtime_init(&runtime);
	sq_vm_runtime_reset(&runtime);
	zassert_equal(sq_vm_runtime_indicator_breathe(&runtime), 0);
	zassert_equal(runtime.indicator_pattern, SQ_VM_RUNTIME_INDICATOR_BREATHE);
	uint8_t first_step = runtime.indicator_pattern_step;

	runtime.status = SQ_VM_RUNTIME_RUNNING;
	runtime.indicator_pattern_next_ms = k_uptime_get() - 1;
	zassert_equal(sq_device_protocol_poll(&context), 0);
	zassert_not_equal(runtime.indicator_pattern_step, first_step);

	runtime.status = SQ_VM_RUNTIME_IDLE;
	sq_vm_runtime_reset(&runtime);
}
