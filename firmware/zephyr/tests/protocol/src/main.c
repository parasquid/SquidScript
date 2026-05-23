#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include <zephyr/fs/fs.h>
#include <zephyr/ztest.h>

#include "app_store.h"
#include "device_protocol.h"
#include "protocol.h"
#include "serial_transport.h"
#include "vm_runtime.h"
#include "vm_fs_storage.h"
#include "squidvm_ffi.h"
#include "vm_storage.h"

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

static const uint8_t headless_counter_sqbc[] = {
	0x53, 0x51, 0x42, 0x43, 0x6e, 0x00, 0x72, 0x01, 0x00, 0x00, 0x08, 0x00,
	0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x6e, 0x00, 0x00, 0x00, 0x1b, 0x00,
	0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x89, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x8b, 0x00, 0x00, 0x00, 0x59, 0x00,
	0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0xe4, 0x00, 0x00, 0x00, 0x1d, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x03, 0x01, 0x00, 0x00, 0x26, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x29, 0x01, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x35, 0x01, 0x00, 0x00, 0x3d, 0x00,
	0x00, 0x00, 0x10, 0x00, 0x68, 0x65, 0x61, 0x64, 0x6c, 0x65, 0x73, 0x73,
	0x2d, 0x63, 0x6f, 0x75, 0x6e, 0x74, 0x65, 0x72, 0x07, 0x00, 0x64, 0x65,
	0x66, 0x61, 0x75, 0x6c, 0x74, 0x00, 0x00, 0x08, 0x00, 0x10, 0x00, 0x68,
	0x65, 0x61, 0x64, 0x6c, 0x65, 0x73, 0x73, 0x2d, 0x63, 0x6f, 0x75, 0x6e,
	0x74, 0x65, 0x72, 0x0c, 0x00, 0x73, 0x74, 0x61, 0x74, 0x65, 0x56, 0x65,
	0x72, 0x73, 0x69, 0x6f, 0x6e, 0x05, 0x00, 0x63, 0x6f, 0x75, 0x6e, 0x74,
	0x07, 0x00, 0x73, 0x74, 0x61, 0x72, 0x74, 0x65, 0x64, 0x09, 0x00, 0x61,
	0x70, 0x70, 0x2e, 0x73, 0x74, 0x61, 0x72, 0x74, 0x0a, 0x00, 0x6b, 0x65,
	0x79, 0x2e, 0x53, 0x45, 0x4c, 0x45, 0x43, 0x54, 0x08, 0x00, 0x6b, 0x65,
	0x79, 0x2e, 0x42, 0x41, 0x43, 0x4b, 0x04, 0x00, 0x6e, 0x6f, 0x6f, 0x70,
	0x03, 0x00, 0x01, 0x00, 0x01, 0x00, 0x02, 0x01, 0x00, 0x00, 0x00, 0x02,
	0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x00,
	0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x04, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x00, 0x2a, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00,
	0x00, 0x2a, 0x00, 0x00, 0x00, 0x0f, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00,
	0x00, 0x39, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x07,
	0x00, 0x3c, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x32, 0x01, 0x0a,
	0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x17, 0x1f, 0x1f, 0x00, 0x00,
	0x00, 0x32, 0x0e, 0x01, 0x01, 0x00, 0x00, 0x00, 0x0b, 0x00, 0x00, 0x1e,
	0x1f, 0x00, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x0b, 0x02, 0x00,
	0x32, 0x02, 0x2a, 0x0a, 0x01, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x14,
	0x0b, 0x01, 0x00, 0x32, 0x02, 0x2a, 0x32, 0x03, 0x2a, 0x2a,
};

static const uint8_t lifecycle_sqbc[] = {
	0x53, 0x51, 0x42, 0x43, 0x6e, 0x00, 0x37, 0x01, 0x00, 0x00, 0x08, 0x00,
	0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x6e, 0x00, 0x00, 0x00, 0x14, 0x00,
	0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x82, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x84, 0x00, 0x00, 0x00, 0x5e, 0x00,
	0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0xe2, 0x00, 0x00, 0x00, 0x0b, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0xed, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xef, 0x00, 0x00, 0x00, 0x1a, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x09, 0x01, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x15, 0x01, 0x00, 0x00, 0x22, 0x00,
	0x00, 0x00, 0x09, 0x00, 0x6c, 0x69, 0x66, 0x65, 0x63, 0x79, 0x63, 0x6c,
	0x65, 0x07, 0x00, 0x64, 0x65, 0x66, 0x61, 0x75, 0x6c, 0x74, 0x00, 0x00,
	0x09, 0x00, 0x09, 0x00, 0x6c, 0x69, 0x66, 0x65, 0x63, 0x79, 0x63, 0x6c,
	0x65, 0x05, 0x00, 0x63, 0x6f, 0x75, 0x6e, 0x74, 0x09, 0x00, 0x61, 0x70,
	0x70, 0x2e, 0x73, 0x74, 0x61, 0x72, 0x74, 0x04, 0x00, 0x72, 0x65, 0x70,
	0x6c, 0x0e, 0x00, 0x62, 0x72, 0x65, 0x61, 0x6b, 0x2d, 0x72, 0x65, 0x6d,
	0x69, 0x6e, 0x64, 0x65, 0x72, 0x06, 0x00, 0x72, 0x65, 0x61, 0x64, 0x65,
	0x72, 0x0b, 0x00, 0x74, 0x69, 0x6d, 0x65, 0x72, 0x2e, 0x62, 0x72, 0x65,
	0x61, 0x6b, 0x0c, 0x00, 0x6c, 0x69, 0x66, 0x65, 0x63, 0x79, 0x63, 0x6c,
	0x65, 0x20, 0x6f, 0x6b, 0x04, 0x00, 0x6d, 0x61, 0x69, 0x6e, 0x01, 0x00,
	0x01, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
	0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
	0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00,
	0x00, 0x01, 0x00, 0x08, 0x00, 0x21, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
	0x00, 0x2a, 0x03, 0x04, 0x00, 0x32, 0x10, 0x03, 0x05, 0x00, 0x32, 0x0d,
	0x03, 0x04, 0x00, 0x32, 0x11, 0x03, 0x06, 0x00, 0x01, 0xfa, 0x00, 0x00,
	0x00, 0x32, 0x13, 0x03, 0x07, 0x00, 0x32, 0x04, 0x01, 0x2a, 0x2a,
};

static const uint8_t reader_exit_sqbc[] = {
	0x53, 0x51, 0x42, 0x43, 0x6e, 0x00, 0xe5, 0x00, 0x00, 0x00, 0x08, 0x00,
	0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x6e, 0x00, 0x00, 0x00, 0x11, 0x00,
	0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x7f, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x81, 0x00, 0x00, 0x00, 0x2f, 0x00,
	0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0xb0, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0xb2, 0x00, 0x00, 0x00, 0x02, 0x00,
	0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xb4, 0x00, 0x00, 0x00, 0x1a, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0xce, 0x00, 0x00, 0x00, 0x0c, 0x00,
	0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0xda, 0x00, 0x00, 0x00, 0x0b, 0x00,
	0x00, 0x00, 0x06, 0x00, 0x72, 0x65, 0x61, 0x64, 0x65, 0x72, 0x07, 0x00,
	0x64, 0x65, 0x66, 0x61, 0x75, 0x6c, 0x74, 0x00, 0x00, 0x05, 0x00, 0x06,
	0x00, 0x72, 0x65, 0x61, 0x64, 0x65, 0x72, 0x09, 0x00, 0x61, 0x70, 0x70,
	0x2e, 0x73, 0x74, 0x61, 0x72, 0x74, 0x0c, 0x00, 0x72, 0x65, 0x61, 0x64,
	0x65, 0x72, 0x20, 0x73, 0x74, 0x61, 0x72, 0x74, 0x04, 0x00, 0x72, 0x65,
	0x70, 0x6c, 0x04, 0x00, 0x6d, 0x61, 0x69, 0x6e, 0x00, 0x00, 0x00, 0x00,
	0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x00,
	0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x03, 0x00,
	0x00, 0x00, 0x01, 0x00, 0x04, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x01, 0x00,
	0x00, 0x00, 0x03, 0x02, 0x00, 0x32, 0x04, 0x01, 0x2a, 0x32, 0x03, 0x2a,
	0x2a,
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

static void wait_runtime_done(struct sq_vm_runtime *runtime)
{
	for (int i = 0; i < 100 && runtime->status == SQ_VM_RUNTIME_RUNNING; i++) {
		k_sleep(K_MSEC(1));
	}
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

static int mount_test_fs(void)
{
	int result = fs_mount(&test_fs_mount);

	return result == -EALREADY ? 0 : result;
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
	uint8_t response[128];
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
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.registry = &registry,
		.mutable_registry = &registry,
		.install_session = &install_session,
		.temp_session = &temp_session,
		.resource_session = &resource_session,
		.runtime = &runtime,
		.store_mount_point = test_fs_mount.mnt_point,
		.launch_storage = &launch_storage,
	};
	int handle_result;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "main", sqbc, sizeof(sqbc)),
		      0);
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
	zassert_equal(frame.status, SQ_STATUS_OK);
	zassert_equal(registry.count, 0);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_IDLE);
	zassert_false(install_session.active);
	zassert_false(temp_session.active);
	zassert_false(resource_session.active);
	zassert_equal(launch_storage.sqbc_path[0], '\0');
	zassert_equal(fs_stat("/sqtest/apps/main/main.sqbc", &entry), -ENOENT);
	zassert_equal(fs_stat("/sqtest/apps", &entry), 0);
	zassert_equal(fs_stat("/sqtest/state", &entry), 0);
	zassert_equal(fs_stat("/sqtest/tmp", &entry), 0);
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
	zassert_equal(sq_app_store_prepare_filesystem(test_fs_mount.mnt_point), 0);
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
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "launch-app",
					       headless_counter_sqbc,
					       sizeof(headless_counter_sqbc)),
		      0);
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
	zassert_equal(runtime.status, SQ_VM_RUNTIME_RUNNING);
	wait_runtime_done(&runtime);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);
	zassert_equal(runtime.result_code, 0);
	zassert_equal(runtime.trace_count, 3);
	zassert_str_equal(runtime.traces[0], "app.start");
	zassert_str_equal(runtime.traces[1], "state.load");
	zassert_str_equal(runtime.traces[2], "state.save");

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
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "lifecycle", lifecycle_sqbc,
					       sizeof(lifecycle_sqbc)),
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
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);
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
	zassert_mem_equal(field.value, "app.arm break-reminder",
			  strlen("app.arm break-reminder"));
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_mem_equal(field.value, "app.launch reader", strlen("app.launch reader"));
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_mem_equal(field.value, "app.disarm break-reminder",
			  strlen("app.disarm break-reminder"));

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
	bool saw_arm = false;
	bool saw_launch = false;
	bool saw_disarm = false;
	while (sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field) ==
	       SQ_PROTOCOL_OK) {
		saw_arm = saw_arm || field_string_equals(&field, "app.arm break-reminder");
		saw_launch = saw_launch || field_string_equals(&field, "app.launch reader");
		saw_disarm = saw_disarm || field_string_equals(&field, "app.disarm break-reminder");
	}
	zassert_true(saw_arm);
	zassert_true(saw_launch);
	zassert_true(saw_disarm);

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
	struct sq_app_store_vm_storage launch_storage = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.store_mount_point = test_fs_mount.mnt_point,
		.runtime = &runtime,
		.launch_storage = &launch_storage,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "lifecycle", lifecycle_sqbc,
					       sizeof(lifecycle_sqbc)),
		      0);
	zassert_equal(sq_app_store_install_app(test_fs_mount.mnt_point, "reader", reader_exit_sqbc,
					       sizeof(reader_exit_sqbc)),
		      0);

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
	zassert_str_equal(runtime.current_app, "lifecycle");
	zassert_equal(runtime.return_stack_count, 0);

	payload_len = 0;
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "lifecycle"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 2,
						      "repl"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_EVENT_DISPATCH,
						      SQ_STATUS_OK, 45, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	wait_runtime_done(&runtime);
	for (int i = 0; i < 20; i++) {
		zassert_equal(sq_device_protocol_poll(&context), 0);
		if (runtime.status != SQ_VM_RUNTIME_RUNNING && strcmp(runtime.current_app, "reader") == 0) {
			break;
		}
		k_sleep(K_MSEC(1));
	}
	zassert_str_equal(runtime.current_app, "reader");
	zassert_equal(runtime.return_stack_count, 1);
	zassert_str_equal(runtime.return_stack[0], "lifecycle");
	zassert_equal(runtime.output_count, 2);
	zassert_str_equal(runtime.outputs[1], "reader start");

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
	zassert_true(field_string_equals(&field, "process_stack[0]=lifecycle"));
	zassert_equal(sq_protocol_next_field(frame.payload, frame.payload_len, &offset, &field),
		      SQ_PROTOCOL_OK);
	zassert_true(field_string_equals(&field, "armed_stack="));

	payload_len = 0;
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 1,
						      "reader"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_string_field(payload, sizeof(payload), &payload_len, 2,
						      "repl"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_EVENT_DISPATCH,
						      SQ_STATUS_OK, 46, payload, payload_len,
						      request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);
	wait_runtime_done(&runtime);
	for (int i = 0; i < 20; i++) {
		zassert_equal(sq_device_protocol_poll(&context), 0);
		if (runtime.status != SQ_VM_RUNTIME_RUNNING &&
		    strcmp(runtime.current_app, "lifecycle") == 0) {
			break;
		}
		k_sleep(K_MSEC(1));
	}
	zassert_str_equal(runtime.current_app, "lifecycle");
	zassert_equal(runtime.return_stack_count, 0);

	zassert_equal(fs_unmount(&test_fs_mount), 0, "unmount failed");
}

ZTEST(squidscript_protocol, test_handles_temp_run_commit_dispatches_file_staged_app_start)
{
	uint8_t begin_payload[64];
	uint8_t chunk_payload[512];
	uint8_t request[768];
	uint8_t response[128];
	size_t payload_len = 0;
	size_t response_len = 0;
	struct sq_device_identity identity = {
		.target = "esp32c3-supermini",
		.firmware = "squidscript-zephyr",
		.diagnostic = true,
	};
	struct sq_device_temp_session temp_session = {0};
	struct sq_vm_runtime runtime = {0};
	struct sq_device_protocol_context context = {
		.identity = &identity,
		.temp_session = &temp_session,
		.runtime = &runtime,
		.store_mount_point = test_fs_mount.mnt_point,
	};

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_prepare_filesystem(test_fs_mount.mnt_point), 0);
	zassert_true(sizeof(temp_session) < 512,
		     "temp-run session must not reserve full SQBC payload RAM");

	zassert_equal(sq_protocol_append_string_field(begin_payload, sizeof(begin_payload),
						     &payload_len, 1, "temp-app"),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_u64_field(begin_payload, sizeof(begin_payload),
						  &payload_len, 2,
						  sizeof(headless_counter_sqbc)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_append_u64_field(begin_payload, sizeof(begin_payload),
						  &payload_len, 3,
						  sq_protocol_crc32(headless_counter_sqbc,
								    sizeof(headless_counter_sqbc))),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_TEMP_RUN_BEGIN,
						      SQ_STATUS_OK, 50, begin_payload,
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
						    &payload_len, 2, headless_counter_sqbc,
						    sizeof(headless_counter_sqbc)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_TEMP_RUN_CHUNK,
						      SQ_STATUS_OK, 51, chunk_payload,
						      payload_len, request, sizeof(request)),
		      SQ_PROTOCOL_OK);
	memcpy(&request[SQ_PROTOCOL_HEADER_LEN], chunk_payload, payload_len);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN + payload_len,
						      &context, response, sizeof(response),
						      &response_len),
		      SQ_PROTOCOL_OK);

	zassert_equal(sq_protocol_encode_frame_header(SQ_FRAME_REQUEST, SQ_OPCODE_TEMP_RUN_COMMIT,
						      SQ_STATUS_OK, 52, NULL, 0, request,
						      sizeof(request)),
		      SQ_PROTOCOL_OK);
	zassert_equal(sq_device_protocol_handle_frame(request, SQ_PROTOCOL_HEADER_LEN, &context,
						      response, sizeof(response), &response_len),
		      SQ_PROTOCOL_OK);
	zassert_true(response_len >= SQ_PROTOCOL_HEADER_LEN);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_RUNNING);
	wait_runtime_done(&runtime);
	zassert_equal(runtime.status, SQ_VM_RUNTIME_COMPLETE);
	zassert_equal(runtime.result_code, 0);
	zassert_equal(runtime.trace_count, 3);
	zassert_str_equal(runtime.traces[0], "app.start");

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

ZTEST(squidscript_protocol, test_links_squidvm_ffi_context_metadata)
{
	zassert_true(sqvm_context_size() > 0);
	zassert_true(sqvm_context_align() > 0);
}

ZTEST(squidscript_protocol, test_exposes_resumable_squidvm_ffi_abi)
{
	SqvmCallbacks callbacks = {0};
	SqvmDispatchResult result = {0};
	SqvmStorageCompletion completion = {0};

	zassert_equal(sqvm_storage_transfer_capacity(), SQVM_STORAGE_TRANSFER_CAPACITY);
	zassert_equal(sizeof(result.storage.bytes), SQVM_STORAGE_TRANSFER_CAPACITY);
	zassert_equal(sizeof(completion.bytes), SQVM_STORAGE_TRANSFER_CAPACITY);

	zassert_equal(sqvm_dispatch_start_resumable(NULL, callbacks, (const uint8_t *)"app.start",
						    9, &result),
		      SQVM_STATUS_INVALID_ARGUMENT);
	zassert_equal(sqvm_dispatch_resume_storage(NULL, callbacks, &completion, &result),
		      SQVM_STATUS_INVALID_ARGUMENT);
}

struct vm_storage_fixture {
	const uint8_t *sqbc;
	size_t sqbc_len;
	uint8_t state[SQVM_STORAGE_TRANSFER_CAPACITY];
	size_t state_len;
	bool state_present;
	bool reset_called;
};

struct ffi_vm_fixture {
	struct vm_storage_fixture storage;
	char traces[4][16];
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

static int fixture_read_sqbc(void *user_data, size_t offset, uint8_t *out, size_t len)
{
	struct vm_storage_fixture *fixture = user_data;

	if (offset > fixture->sqbc_len || len > fixture->sqbc_len - offset) {
		return -EINVAL;
	}
	memcpy(out, fixture->sqbc + offset, len);
	return 0;
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
	zassert_equal(sq_app_store_prepare_filesystem(test_fs_mount.mnt_point), 0);

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

ZTEST(squidscript_protocol, test_app_store_installs_app_and_rebuilds_registry)
{
	const uint8_t sqbc_a[] = {0x53, 0x51, 0x42, 0x43, 0x01};
	const uint8_t sqbc_b[] = {0x53, 0x51, 0x42, 0x43, 0x02, 0x03};
	struct sq_app_registry registry = {0};
	struct fs_dirent entry;
	const struct sq_app_registry_entry *installed;

	zassert_equal(mount_test_fs(), 0, "mount failed");
	zassert_equal(sq_app_store_prepare_filesystem(test_fs_mount.mnt_point), 0);

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
		.user_data = &fixture,
		.trace = ffi_trace,
		.read_exact_at = ffi_read_exact_at,
	};
	SqvmDispatchResult result = {0};
	SqvmStorageCompletion completion = {0};
	zassert_true(sqvm_context_size() <= sizeof(ffi_context_storage));
	zassert_equal(sqvm_context_prepare(ffi_context_storage, sizeof(ffi_context_storage)),
		      SQVM_STATUS_OK);
	zassert_equal(sqvm_context_init_in_place(ffi_context_storage, callbacks, ffi_scratch,
						 sizeof(ffi_scratch)),
		      SQVM_STATUS_OK);

	zassert_equal(sqvm_dispatch_start_resumable(ffi_context_storage, callbacks,
						    (const uint8_t *)"app.start", 9, &result),
		      SQVM_STATUS_OK);

	while (result.outcome == SQVM_DISPATCH_PENDING_STORAGE) {
		zassert_equal(sq_vm_storage_complete_request(&backend, &result.storage, &completion),
			      0);
		zassert_equal(sqvm_dispatch_resume_storage(ffi_context_storage, callbacks,
							   &completion, &result),
			      SQVM_STATUS_OK);
	}

	zassert_equal(result.outcome, SQVM_DISPATCH_COMPLETE);
	zassert_equal(fixture.trace_count, 3);
	zassert_str_equal(fixture.traces[0], "app.start");
	zassert_str_equal(fixture.traces[1], "state.load");
	zassert_str_equal(fixture.traces[2], "state.save");
	zassert_true(fixture.storage.state_present);
	zassert_true(fixture.storage.state_len > 0);
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

	zassert_equal(sq_vm_runtime_dispatch(&runtime, &backend, "app.start"), 0);
	zassert_equal(runtime.trace_count, 3);
	zassert_str_equal(runtime.traces[0], "app.start");
	zassert_str_equal(runtime.traces[1], "state.load");
	zassert_str_equal(runtime.traces[2], "state.save");
}

ZTEST(squidscript_protocol, test_vm_runtime_tracks_output_indicator_and_due_timers)
{
	struct sq_vm_runtime runtime = {0};
	char event[SQ_VM_RUNTIME_EVENT_LEN];

	sq_vm_runtime_init(&runtime);

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
	zassert_true(runtime.indicator_breathe_active);
	uint8_t first_step = runtime.indicator_breathe_step;
	runtime.indicator_breathe_next_ms = k_uptime_get() - 1;
	zassert_equal(sq_vm_runtime_poll(&runtime), 0);
	zassert_true(runtime.indicator_breathe_active);
	zassert_not_equal(runtime.indicator_breathe_step, first_step);

	zassert_equal(sq_vm_runtime_indicator_write(&runtime, true), 0);
	zassert_false(runtime.indicator_breathe_active);

	zassert_equal(sq_vm_runtime_indicator_breathe(&runtime), 0);
	zassert_true(runtime.indicator_breathe_active);
	zassert_equal(sq_vm_runtime_hardware_gpio_write(&runtime, (const uint8_t *)"GPIO8",
							strlen("GPIO8"), true),
		      0);
	zassert_equal(sq_vm_runtime_hardware_gpio_read(&runtime, (const uint8_t *)"GPIO8",
						       strlen("GPIO8"), &value),
		      0);
	zassert_true(value);
	zassert_equal(sq_vm_runtime_indicator_breathe(&runtime), 0);
	zassert_true(runtime.indicator_breathe_active);
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
