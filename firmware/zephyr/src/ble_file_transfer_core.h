#ifndef SQUIDSCRIPT_BLE_FILE_TRANSFER_CORE_H
#define SQUIDSCRIPT_BLE_FILE_TRANSFER_CORE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <sys/types.h>

/*
 * Transport-neutral BLE file-transfer core: the single in-flight staging
 * session, the file-name router/parser, and the completed-transfer pending
 * event handed off to the runtime poll loop. The BLE transport front-end
 * (ble_file_transfer.c) drives this core; it lives in its own translation unit
 * so additional transport front-ends can sit on top without depending on a
 * specific one.
 */

/* Reject codes the file-name parser / create returns when the request is
 * malformed (INV_PARAM) or a transfer is already in progress (BUSY). A
 * transport front-end maps them to its own wire-level error (e.g. an ATT error
 * for the GATT transport).
 */
#define SQ_BLE_FILE_TRANSFER_RES_INV_PARAM  0x03
#define SQ_BLE_FILE_TRANSFER_RES_BUSY 0x0a
#define SQ_BLE_FILE_TRANSFER_RES_ROUTE_AMBIGUOUS 0x11

#ifdef __cplusplus
extern "C" {
#endif

struct sq_app_registry;
typedef void (*sq_ble_file_transfer_error_sink)(void *user_data, const char *line);

void sq_ble_file_transfer_set_registry(const struct sq_app_registry *registry);

void sq_ble_file_transfer_set_fallback_app_id(const char *app_id);

void sq_ble_file_transfer_set_error_sink(sq_ble_file_transfer_error_sink sink, void *user_data);

int sq_ble_file_transfer_parse_file_name(const char *name, char *extension_out,
					 size_t extension_cap);

/* Discard an in-flight transfer (client ABORT, disconnect, or error). */
void sq_ble_file_transfer_abort(void);

/* Framed transfer for transports whose individual writes must stay small (e.g.
 * GATT at the default ATT MTU). begin_framed declares the content size and the
 * file-name length; feed_name appends name bytes (across one or more writes)
 * until name_len is reached, at which point the name is parsed and the staging
 * file opened; feed_content then appends content bytes, and completion (content
 * fully received) publishes the same pending event as the non-framed path.
 * Each returns 0 on success or a reject/-errno code.
 */
int sq_ble_file_transfer_begin_framed(size_t total_size, size_t name_len);

int sq_ble_file_transfer_feed_name(const void *data, size_t len);

int sq_ble_file_transfer_feed_content(const void *data, size_t len);

void sq_ble_file_transfer_reset_session(void);

int sq_ble_file_transfer_drain_pending_event(char *app_id_out, size_t app_id_cap, char *event_out,
				   size_t event_cap);

const char *sq_ble_file_transfer_pending_staging_path(void);

const char *sq_ble_file_transfer_pending_profile_id(void);

size_t sq_ble_file_transfer_pending_bytes_received(void);

size_t sq_ble_file_transfer_pending_total_bytes(void);

void sq_ble_file_transfer_cleanup_staging(void);

bool sq_ble_file_transfer_pending_is_complete(void);

const char *sq_ble_file_transfer_pending_app_id(void);

const char *sq_ble_file_transfer_pending_event_name(void);

int sq_ble_file_transfer_test_invoke_begin_with_name(const char *name, size_t alloc_size,
						 char *staging_path_out,
						 size_t staging_path_out_len);

int sq_ble_file_transfer_test_invoke_write_with_path(const char *staging_path, const void *data,
					       size_t len, off_t offset, size_t rem);

void sq_ble_file_transfer_test_invoke_abort(void);

#ifdef __cplusplus
}
#endif

#endif /* SQUIDSCRIPT_BLE_FILE_TRANSFER_CORE_H */
