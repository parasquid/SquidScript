#ifndef SQUIDSCRIPT_HTTP_UPLOAD_H
#define SQUIDSCRIPT_HTTP_UPLOAD_H

#include <stddef.h>
#include <stdbool.h>

#include "app_store.h"
#include "squidvm_ffi.h"

struct SqvmEventPayloadField;
struct sq_app_registry;

void sq_http_upload_set_registry(const struct sq_app_registry *registry);
void sq_http_upload_set_fallback_app_id(const char *app_id);
void sq_http_upload_set_error_sink(void (*sink)(void *user_data, const char *line),
				   void *user_data);

int sq_http_upload_start_profile(const char *app_id, const char *id,
				 const char accept[][SQVM_HTTP_PROFILE_TEXT_CAP],
				 size_t accept_count, const SqvmHttpProfileEventRoute *events,
				 size_t event_count);
int sq_http_upload_stop_app(const char *app_id);
void sq_http_upload_abort(void);

bool sq_http_upload_pending_is_complete(void);
int sq_http_upload_drain_pending_event(char *app_id_out, size_t app_id_cap, char *event_out,
				       size_t event_cap);
const char *sq_http_upload_pending_staging_path(void);
const char *sq_http_upload_pending_profile_id(void);
const char *sq_http_upload_pending_name(void);
size_t sq_http_upload_pending_bytes_received(void);
size_t sq_http_upload_pending_total_bytes(void);
void sq_http_upload_cleanup_staging(void);

#ifdef CONFIG_ZTEST
int sq_http_upload_test_complete(const char *name, const char *staging_path,
				 size_t bytes_received, size_t total_bytes);
int sq_http_upload_test_begin(const char *name, size_t offset, size_t content_len,
			      size_t total_bytes);
int sq_http_upload_test_write(const void *data, size_t len);
int sq_http_upload_test_finish(void);
int sq_http_upload_test_preserve_partial(void);
int sq_http_upload_test_offset(const char *name, size_t *out_offset, size_t *out_total);
#endif

#endif
