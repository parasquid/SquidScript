#include "http_upload.h"

#include "sq_errno.h"
#include "vm_runtime.h"

#include <errno.h>
#include <limits.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>

#include <zephyr/fs/fs.h>
#include <zephyr/kernel.h>
#if IS_ENABLED(CONFIG_NET_SOCKETS)
#include <zephyr/net/socket.h>
#endif
#include <zephyr/sys/util.h>

#define SQ_HTTP_UPLOAD_DIR "/SD:/sq/tmp"
#define SQ_HTTP_UPLOAD_ROUTE_PREFIX "/upload/"
#define SQ_HTTP_UPLOAD_MAX_NAME_LEN 80
#define SQ_HTTP_UPLOAD_HEADER_MAX 1024
#define SQ_HTTP_UPLOAD_CHUNK_MAX 2048
#define SQ_HTTP_UPLOAD_POLL_MS 500
#define SQ_HTTP_UPLOAD_RETRY_MS 1000
#define SQ_HTTP_UPLOAD_THREAD_STACK 2048
#define SQ_HTTP_UPLOAD_THREAD_PRIORITY 8
#define SQ_HTTP_UPLOAD_RESPONSE_OK "ok\n"
#define SQ_HTTP_UPLOAD_RESPONSE_INACTIVE "inactive\n"
#define SQ_HTTP_UPLOAD_RESPONSE_BAD_NAME "bad name\n"
#define SQ_HTTP_UPLOAD_RESPONSE_BUSY "busy\n"
#define SQ_HTTP_UPLOAD_RESPONSE_IO "io error\n"
#define SQ_HTTP_UPLOAD_RESPONSE_RANGE "range\n"

struct sq_http_upload_route {
	bool active;
	char app_id[SQ_APP_STORE_APP_ID_MAX];
	char profile_id[SQVM_HTTP_PROFILE_TEXT_CAP];
	char complete_event[SQ_VM_RUNTIME_EVENT_LEN];
	char accept[SQVM_HTTP_PROFILE_ACCEPT_MAX][SQVM_HTTP_PROFILE_TEXT_CAP];
	size_t accept_count;
};

struct sq_http_upload_session {
	bool active;
	void *client;
	struct fs_file_t file;
	char app_id[SQ_APP_STORE_APP_ID_MAX];
	char profile_id[SQVM_HTTP_PROFILE_TEXT_CAP];
	char complete_event[SQ_VM_RUNTIME_EVENT_LEN];
	char name[SQ_HTTP_UPLOAD_MAX_NAME_LEN];
	char staging_path[SQ_APP_STORE_PATH_MAX];
	size_t bytes_received;
	size_t total_bytes;
};

struct sq_http_upload_pending {
	bool active;
	char app_id[SQ_APP_STORE_APP_ID_MAX];
	char profile_id[SQVM_HTTP_PROFILE_TEXT_CAP];
	char event[SQ_VM_RUNTIME_EVENT_LEN];
	char name[SQ_HTTP_UPLOAD_MAX_NAME_LEN];
	char staging_path[SQ_APP_STORE_PATH_MAX];
	size_t bytes_received;
	size_t total_bytes;
};

struct sq_http_upload_partial {
	bool active;
	char name[SQ_HTTP_UPLOAD_MAX_NAME_LEN];
	char staging_path[SQ_APP_STORE_PATH_MAX];
	size_t bytes_received;
	size_t total_bytes;
};

static struct sq_http_upload_route sq_http_route;
static struct sq_http_upload_session sq_http_session;
static struct sq_http_upload_pending sq_http_pending;
static struct sq_http_upload_partial sq_http_partial;
#if IS_ENABLED(CONFIG_NET_SOCKETS)
static char sq_http_header_buffer[SQ_HTTP_UPLOAD_HEADER_MAX];
static uint8_t sq_http_chunk_buffer[SQ_HTTP_UPLOAD_CHUNK_MAX];
#endif
static void (*sq_http_error_sink)(void *user_data, const char *line);
static void *sq_http_error_user_data;
#if IS_ENABLED(CONFIG_NET_SOCKETS)
static K_SEM_DEFINE(sq_http_start_sem, 0, 1);
#endif

void sq_http_upload_set_registry(const struct sq_app_registry *registry)
{
	ARG_UNUSED(registry);
}

void sq_http_upload_set_fallback_app_id(const char *app_id)
{
	ARG_UNUSED(app_id);
}

void sq_http_upload_set_error_sink(void (*sink)(void *user_data, const char *line),
				   void *user_data)
{
	sq_http_error_sink = sink;
	sq_http_error_user_data = user_data;
}

static bool sq_http_name_safe(const char *name)
{
	size_t len;

	if (name == NULL || name[0] == '\0' || name[0] == '.') {
		return false;
	}
	len = strlen(name);
	if (len >= SQ_HTTP_UPLOAD_MAX_NAME_LEN) {
		return false;
	}
	for (size_t i = 0; i < len; ++i) {
		if (name[i] == '/' || name[i] == '\\') {
			return false;
		}
	}
	return true;
}

static const char *sq_http_extension(const char *name)
{
	const char *dot = strrchr(name, '.');

	return dot == NULL ? "" : dot;
}

static bool sq_http_extension_accepted(const char *name)
{
	const char *extension = sq_http_extension(name);

	for (size_t i = 0; i < sq_http_route.accept_count; ++i) {
		if (strcmp(extension, sq_http_route.accept[i]) == 0) {
			return true;
		}
	}
	return false;
}

static int sq_http_route_event(const SqvmHttpProfileEventRoute *events, size_t event_count,
			       const char *kind, char *out, size_t out_len)
{
	for (size_t i = 0; i < event_count; ++i) {
		if (strncmp((const char *)events[i].kind, kind, SQVM_HTTP_PROFILE_TEXT_CAP) == 0) {
			strncpy(out, (const char *)events[i].event, out_len - 1);
			out[out_len - 1] = '\0';
			return out[0] == '\0' ? -EINVAL : 0;
		}
	}
	return -ENOENT;
}

int sq_http_upload_start_profile(const char *app_id, const char *id,
				 const char accept[][SQVM_HTTP_PROFILE_TEXT_CAP],
				 size_t accept_count, const SqvmHttpProfileEventRoute *events,
				 size_t event_count)
{
	if (app_id == NULL || app_id[0] == '\0' || id == NULL || id[0] == '\0' ||
	    accept == NULL || accept_count == 0 || accept_count > SQVM_HTTP_PROFILE_ACCEPT_MAX ||
	    events == NULL) {
		return -EINVAL;
	}
	memset(&sq_http_route, 0, sizeof(sq_http_route));
	strncpy(sq_http_route.app_id, app_id, sizeof(sq_http_route.app_id) - 1);
	strncpy(sq_http_route.profile_id, id, sizeof(sq_http_route.profile_id) - 1);
	sq_http_route.accept_count = accept_count;
	for (size_t i = 0; i < accept_count; ++i) {
		strncpy(sq_http_route.accept[i], accept[i], sizeof(sq_http_route.accept[i]) - 1);
	}
	int result = sq_http_route_event(events, event_count, "complete",
					 sq_http_route.complete_event,
					 sizeof(sq_http_route.complete_event));
	if (result != 0) {
		memset(&sq_http_route, 0, sizeof(sq_http_route));
		return result;
	}
	sq_http_route.active = true;
#if IS_ENABLED(CONFIG_NET_SOCKETS)
	k_sem_give(&sq_http_start_sem);
#endif
	return 0;
}

int sq_http_upload_stop_app(const char *app_id)
{
	if (app_id == NULL || !sq_http_route.active) {
		return 0;
	}
	if (strcmp(sq_http_route.app_id, app_id) == 0) {
		memset(&sq_http_route, 0, sizeof(sq_http_route));
		sq_http_upload_abort();
	}
	return 0;
}

void sq_http_upload_abort(void)
{
	if (sq_http_session.active) {
		(void)fs_close(&sq_http_session.file);
		if (sq_http_session.staging_path[0] != '\0') {
			(void)fs_unlink(sq_http_session.staging_path);
		}
	}
	memset(&sq_http_session, 0, sizeof(sq_http_session));
	if (sq_http_partial.staging_path[0] != '\0') {
		(void)fs_unlink(sq_http_partial.staging_path);
	}
	memset(&sq_http_partial, 0, sizeof(sq_http_partial));
}

static bool sq_http_route_accepts_name(const char *name)
{
	return sq_http_name_safe(name) && sq_http_extension_accepted(name);
}

#if IS_ENABLED(CONFIG_NET_SOCKETS)
static void sq_http_record_error(const char *name, int code)
{
	char line[SQ_VM_RUNTIME_DEVICE_ERROR_LEN];

	if (sq_http_error_sink == NULL || name == NULL) {
		return;
	}
	snprintk(line, sizeof(line), "invariant.http.%s code=%d (%s)", name, code,
		 sq_errno_name(code));
	sq_http_error_sink(sq_http_error_user_data, line);
}
#endif

static int sq_http_prepare_dirs(void)
{
	int result = fs_mkdir("/SD:/sq");

	if (result != 0 && result != -EEXIST) {
		return result;
	}
	result = fs_mkdir(SQ_HTTP_UPLOAD_DIR);
	return result == -EEXIST ? 0 : result;
}

static int sq_http_staging_path(char *out, size_t out_cap)
{
	int written;

	if (out == NULL || out_cap == 0 || !sq_http_route.active) {
		return -EINVAL;
	}
	written = snprintf(out, out_cap, SQ_HTTP_UPLOAD_DIR "/%s.upload",
			   sq_http_route.profile_id);
	return written > 0 && (size_t)written < out_cap ? 0 : -ENAMETOOLONG;
}

static int sq_http_partial_offset(const char *name, size_t *out_offset, size_t *out_total)
{
	struct fs_dirent entry;

	if (out_offset == NULL || out_total == NULL || name == NULL) {
		return -EINVAL;
	}
	*out_offset = 0;
	*out_total = 0;
	if (!sq_http_route_accepts_name(name)) {
		return -EINVAL;
	}
	if (!sq_http_partial.active || strcmp(sq_http_partial.name, name) != 0 ||
	    sq_http_partial.staging_path[0] == '\0') {
		return 0;
	}
	if (fs_stat(sq_http_partial.staging_path, &entry) != 0 ||
	    entry.type != FS_DIR_ENTRY_FILE) {
		memset(&sq_http_partial, 0, sizeof(sq_http_partial));
		return 0;
	}
	*out_offset = entry.size;
	*out_total = sq_http_partial.total_bytes;
	return 0;
}

static int sq_http_begin_session(void *client, const char *url, size_t offset,
				 size_t content_len, size_t total_bytes)
{
	const char *name;
	char staging_path[SQ_APP_STORE_PATH_MAX];
	int result;

	if (!sq_http_route.active) {
		return -ENOENT;
	}
	if (sq_http_session.active) {
		return -EBUSY;
	}
	if (strncmp(url, SQ_HTTP_UPLOAD_ROUTE_PREFIX, sizeof(SQ_HTTP_UPLOAD_ROUTE_PREFIX) - 1) != 0) {
		return -EINVAL;
	}
	name = url + sizeof(SQ_HTTP_UPLOAD_ROUTE_PREFIX) - 1;
	if (!sq_http_route_accepts_name(name)) {
		return -EINVAL;
	}
	if (offset > total_bytes || content_len > total_bytes - offset) {
		return -ERANGE;
	}
	result = sq_http_prepare_dirs();
	if (result != 0) {
		return result;
	}
	result = sq_http_staging_path(staging_path, sizeof(staging_path));
	if (result != 0) {
		return result;
	}
	memset(&sq_http_session, 0, sizeof(sq_http_session));
	sq_http_session.active = true;
	sq_http_session.client = client;
	strncpy(sq_http_session.app_id, sq_http_route.app_id, sizeof(sq_http_session.app_id) - 1);
	strncpy(sq_http_session.profile_id, sq_http_route.profile_id,
		sizeof(sq_http_session.profile_id) - 1);
	strncpy(sq_http_session.complete_event, sq_http_route.complete_event,
		sizeof(sq_http_session.complete_event) - 1);
	strncpy(sq_http_session.name, name, sizeof(sq_http_session.name) - 1);
	sq_http_session.bytes_received = offset;
	sq_http_session.total_bytes = total_bytes;
	strncpy(sq_http_session.staging_path, staging_path,
		sizeof(sq_http_session.staging_path) - 1);
	fs_file_t_init(&sq_http_session.file);
	if (offset == 0) {
		(void)fs_unlink(sq_http_session.staging_path);
		memset(&sq_http_partial, 0, sizeof(sq_http_partial));
		result = fs_open(&sq_http_session.file, sq_http_session.staging_path,
				 FS_O_CREATE | FS_O_TRUNC | FS_O_WRITE);
	} else {
		size_t partial_offset = 0;
		size_t partial_total = 0;

		result = sq_http_partial_offset(name, &partial_offset, &partial_total);
		if (result != 0) {
			memset(&sq_http_session, 0, sizeof(sq_http_session));
			return result;
		}
		if (partial_offset != offset || partial_total != total_bytes) {
			memset(&sq_http_session, 0, sizeof(sq_http_session));
			return -ERANGE;
		}
		result = fs_open(&sq_http_session.file, sq_http_session.staging_path, FS_O_WRITE);
		if (result == 0) {
			result = fs_seek(&sq_http_session.file, (off_t)offset, FS_SEEK_SET);
		}
	}
	if (result != 0) {
		memset(&sq_http_session, 0, sizeof(sq_http_session));
		return result;
	}
	return 0;
}

static int sq_http_write_chunk(const uint8_t *data, size_t len)
{
	if (!sq_http_session.active || data == NULL || len == 0) {
		return 0;
	}
	ssize_t written = fs_write(&sq_http_session.file, data, len);

	if (written < 0) {
		return (int)written;
	}
	if ((size_t)written != len || sq_http_session.bytes_received > SIZE_MAX - len) {
		return -EIO;
	}
	sq_http_session.bytes_received += len;
	return 0;
}

static int sq_http_complete_session(void)
{
	int result = fs_sync(&sq_http_session.file);
	int close_result = fs_close(&sq_http_session.file);

	if (result == 0) {
		result = close_result;
	}
	if (result != 0) {
		sq_http_upload_abort();
		return result;
	}
	memset(&sq_http_pending, 0, sizeof(sq_http_pending));
	strncpy(sq_http_pending.app_id, sq_http_session.app_id, sizeof(sq_http_pending.app_id) - 1);
	strncpy(sq_http_pending.profile_id, sq_http_session.profile_id,
		sizeof(sq_http_pending.profile_id) - 1);
	strncpy(sq_http_pending.event, sq_http_session.complete_event,
		sizeof(sq_http_pending.event) - 1);
	strncpy(sq_http_pending.name, sq_http_session.name, sizeof(sq_http_pending.name) - 1);
	strncpy(sq_http_pending.staging_path, sq_http_session.staging_path,
		sizeof(sq_http_pending.staging_path) - 1);
	sq_http_pending.bytes_received = sq_http_session.bytes_received;
	sq_http_pending.total_bytes = sq_http_session.total_bytes;
	sq_http_pending.active = true;
	sq_http_session.staging_path[0] = '\0';
	memset(&sq_http_session, 0, sizeof(sq_http_session));
	memset(&sq_http_partial, 0, sizeof(sq_http_partial));
	return 0;
}

static int sq_http_preserve_partial(void)
{
	int result;

	if (!sq_http_session.active) {
		return 0;
	}
	result = fs_sync(&sq_http_session.file);
	if (fs_close(&sq_http_session.file) != 0 && result == 0) {
		result = -EIO;
	}
	if (sq_http_session.staging_path[0] != '\0' && sq_http_session.bytes_received > 0) {
		memset(&sq_http_partial, 0, sizeof(sq_http_partial));
		strncpy(sq_http_partial.name, sq_http_session.name,
			sizeof(sq_http_partial.name) - 1);
		strncpy(sq_http_partial.staging_path, sq_http_session.staging_path,
			sizeof(sq_http_partial.staging_path) - 1);
		sq_http_partial.bytes_received = sq_http_session.bytes_received;
		sq_http_partial.total_bytes = sq_http_session.total_bytes;
		sq_http_partial.active = true;
	}
	memset(&sq_http_session, 0, sizeof(sq_http_session));
	return result;
}

#if IS_ENABLED(CONFIG_NET_SOCKETS)
static int sq_http_send_all(int client, const char *data, size_t len)
{
	const char *cursor = data;
	size_t remaining = len;

	while (remaining > 0) {
		ssize_t sent = zsock_send(client, cursor, remaining, 0);

		if (sent < 0) {
			return -errno;
		}
		if (sent == 0) {
			return -EIO;
		}
		cursor += sent;
		remaining -= (size_t)sent;
	}
	return 0;
}

static void sq_http_send_response(int client, int status, const char *reason, const char *body)
{
	char header[128];
	size_t body_len = strlen(body);
	int written = snprintk(header, sizeof(header),
			       "HTTP/1.1 %d %s\r\nContent-Type: text/plain\r\n"
			       "Content-Length: %zu\r\nConnection: close\r\n\r\n",
			       status, reason, body_len);

	if (written > 0 && (size_t)written < sizeof(header)) {
		(void)sq_http_send_all(client, header, (size_t)written);
	}
	(void)sq_http_send_all(client, body, body_len);
}

static void sq_http_send_head_response(int client, size_t offset, size_t total)
{
	char header[192];
	int written = snprintk(header, sizeof(header),
			       "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n"
			       "X-Squid-Upload-Offset: %zu\r\n"
			       "X-Squid-Upload-Total: %zu\r\nConnection: close\r\n\r\n",
			       offset, total);

	if (written > 0 && (size_t)written < sizeof(header)) {
		(void)sq_http_send_all(client, header, (size_t)written);
	}
}

static bool sq_http_is_client_abort_error(int code)
{
	return code == -ECONNRESET || code == -EAGAIN || code == -ETIMEDOUT ||
	       code == -EPIPE;
}

static char *sq_http_header_end(char *buffer, size_t len)
{
	for (size_t i = 3; i < len; ++i) {
		if (buffer[i - 3] == '\r' && buffer[i - 2] == '\n' && buffer[i - 1] == '\r' &&
		    buffer[i] == '\n') {
			return &buffer[i + 1];
		}
	}
	return NULL;
}

static int sq_http_content_length(const char *headers, size_t *out)
{
	const char *cursor = headers;

	while (cursor != NULL && *cursor != '\0') {
		const char *next = strstr(cursor, "\r\n");
		size_t line_len = next == NULL ? strlen(cursor) : (size_t)(next - cursor);

		if (line_len == 0) {
			break;
		}
		if (line_len > sizeof("Content-Length:") - 1 &&
		    strncasecmp(cursor, "Content-Length:", sizeof("Content-Length:") - 1) == 0) {
			const char *value = cursor + sizeof("Content-Length:") - 1;
			char *end = NULL;
			unsigned long parsed;

			while (*value == ' ' || *value == '\t') {
				value++;
			}
			errno = 0;
			parsed = strtoul(value, &end, 10);
			if (errno != 0 || end == value || parsed > SIZE_MAX) {
				return -EINVAL;
			}
			*out = (size_t)parsed;
			return 0;
		}
		cursor = next == NULL ? NULL : next + 2;
	}
	return -EINVAL;
}

static int sq_http_content_range(const char *headers, size_t *offset_out, size_t *len_out,
				 size_t *total_out, bool *has_range_out)
{
	const char *cursor = headers;

	if (offset_out == NULL || len_out == NULL || total_out == NULL || has_range_out == NULL) {
		return -EINVAL;
	}
	*offset_out = 0;
	*len_out = 0;
	*total_out = 0;
	*has_range_out = false;
	while (cursor != NULL && *cursor != '\0') {
		const char *next = strstr(cursor, "\r\n");
		size_t line_len = next == NULL ? strlen(cursor) : (size_t)(next - cursor);

		if (line_len == 0) {
			break;
		}
		if (line_len > sizeof("Content-Range:") - 1 &&
		    strncasecmp(cursor, "Content-Range:", sizeof("Content-Range:") - 1) == 0) {
			const char *value = cursor + sizeof("Content-Range:") - 1;
			char *end = NULL;
			unsigned long start;
			unsigned long finish;
			unsigned long total;

			while (*value == ' ' || *value == '\t') {
				value++;
			}
			if (strncmp(value, "bytes ", sizeof("bytes ") - 1) != 0) {
				return -EINVAL;
			}
			value += sizeof("bytes ") - 1;
			errno = 0;
			start = strtoul(value, &end, 10);
			if (errno != 0 || end == value || *end != '-') {
				return -EINVAL;
			}
			value = end + 1;
			errno = 0;
			finish = strtoul(value, &end, 10);
			if (errno != 0 || end == value || *end != '/' || finish < start) {
				return -EINVAL;
			}
			value = end + 1;
			errno = 0;
			total = strtoul(value, &end, 10);
			if (errno != 0 || end == value || total == 0 || finish >= total ||
			    start > SIZE_MAX || finish > SIZE_MAX || total > SIZE_MAX) {
				return -EINVAL;
			}
			*offset_out = (size_t)start;
			*len_out = (size_t)(finish - start + 1);
			*total_out = (size_t)total;
			*has_range_out = true;
			return 0;
		}
		cursor = next == NULL ? NULL : next + 2;
	}
	return 0;
}

static bool sq_http_has_expect_continue(const char *headers)
{
	const char *cursor = headers;

	while (cursor != NULL && *cursor != '\0') {
		const char *next = strstr(cursor, "\r\n");
		size_t line_len = next == NULL ? strlen(cursor) : (size_t)(next - cursor);

		if (line_len == 0) {
			break;
		}
		if (line_len >= sizeof("Expect: 100-continue") - 1 &&
		    strncasecmp(cursor, "Expect:", sizeof("Expect:") - 1) == 0) {
			const char *expect = strstr(cursor, "100-continue");

			if (expect != NULL && expect < cursor + line_len) {
				return true;
			}
		}
		cursor = next == NULL ? NULL : next + 2;
	}
	return false;
}

static int sq_http_read_headers(int client, char *buffer, size_t cap, size_t *header_len,
				size_t *total_len)
{
	size_t used = 0;

	while (used < cap - 1) {
		ssize_t received = zsock_recv(client, buffer + used, cap - 1 - used, 0);
		char *end;

		if (received < 0) {
			return -errno;
		}
		if (received == 0) {
			return -EIO;
		}
		used += (size_t)received;
		buffer[used] = '\0';
		end = sq_http_header_end(buffer, used);
		if (end != NULL) {
			*header_len = (size_t)(end - buffer);
			*total_len = used;
			return 0;
		}
	}
	return -ENOBUFS;
}

static int sq_http_parse_request(char *headers, char **method_out, char **path_out,
				 size_t *content_len_out, size_t *range_offset_out,
				 size_t *range_len_out, size_t *range_total_out,
				 bool *has_range_out)
{
	char *line_end = strstr(headers, "\r\n");
	char *method;
	char *path;
	char *version;

	if (line_end == NULL) {
		return -EINVAL;
	}
	*line_end = '\0';
	method = headers;
	path = strchr(method, ' ');
	if (path == NULL) {
		return -EINVAL;
	}
	*path++ = '\0';
	version = strchr(path, ' ');
	if (version == NULL) {
		return -EINVAL;
	}
	*version++ = '\0';
	if ((strcmp(method, "PUT") != 0 && strcmp(method, "HEAD") != 0) ||
	    strncmp(version, "HTTP/", 5) != 0) {
		return -EINVAL;
	}
	*method_out = method;
	*path_out = path;
	if (strcmp(method, "HEAD") == 0) {
		*content_len_out = 0;
		*range_offset_out = 0;
		*range_len_out = 0;
		*range_total_out = 0;
		*has_range_out = false;
		return 0;
	}
	int result = sq_http_content_length(line_end + 2, content_len_out);

	if (result != 0) {
		return result;
	}
	return sq_http_content_range(line_end + 2, range_offset_out, range_len_out,
				     range_total_out, has_range_out);
}

static int sq_http_handle_client(int client)
{
	size_t header_len = 0;
	size_t total_len = 0;
	size_t content_len = 0;
	size_t range_offset = 0;
	size_t range_len = 0;
	size_t range_total = 0;
	size_t received_body = 0;
	bool has_range = false;
	char *method = NULL;
	char *path = NULL;
	char *headers = sq_http_header_buffer;
	int result = sq_http_read_headers(client, headers, SQ_HTTP_UPLOAD_HEADER_MAX, &header_len,
					  &total_len);
	bool expect_continue = result == 0 && sq_http_has_expect_continue(headers);

	if (result == 0) {
		result = sq_http_parse_request(headers, &method, &path, &content_len, &range_offset,
					       &range_len, &range_total, &has_range);
	}
	if (result != 0) {
		sq_http_send_response(client, 400, "Bad Request", SQ_HTTP_UPLOAD_RESPONSE_BAD_NAME);
		return result;
	}
	if (strcmp(method, "HEAD") == 0) {
		size_t offset = 0;
		size_t total = 0;

		if (strncmp(path, SQ_HTTP_UPLOAD_ROUTE_PREFIX,
			    sizeof(SQ_HTTP_UPLOAD_ROUTE_PREFIX) - 1) != 0) {
			sq_http_send_response(client, 400, "Bad Request",
					      SQ_HTTP_UPLOAD_RESPONSE_BAD_NAME);
			return -EINVAL;
		}
		result = sq_http_partial_offset(path + sizeof(SQ_HTTP_UPLOAD_ROUTE_PREFIX) - 1,
						&offset, &total);
		if (result != 0) {
			sq_http_send_response(client, 400, "Bad Request",
					      SQ_HTTP_UPLOAD_RESPONSE_BAD_NAME);
			return result;
		}
		sq_http_send_head_response(client, offset, total);
		return 0;
	}
	if (has_range && range_len != content_len) {
		sq_http_send_response(client, 416, "Range Not Satisfiable",
				      SQ_HTTP_UPLOAD_RESPONSE_RANGE);
		return -ERANGE;
	}
	result = sq_http_begin_session((void *)(uintptr_t)client, path,
				       has_range ? range_offset : 0, content_len,
				       has_range ? range_total : content_len);
	if (result != 0) {
		if (result == -ENOENT) {
			sq_http_send_response(client, 404, "Not Found",
					      SQ_HTTP_UPLOAD_RESPONSE_INACTIVE);
		} else if (result == -EBUSY) {
			sq_http_send_response(client, 409, "Conflict", SQ_HTTP_UPLOAD_RESPONSE_BUSY);
		} else if (result == -ERANGE) {
			sq_http_send_response(client, 416, "Range Not Satisfiable",
					      SQ_HTTP_UPLOAD_RESPONSE_RANGE);
		} else {
			sq_http_send_response(client, 400, "Bad Request",
					      SQ_HTTP_UPLOAD_RESPONSE_BAD_NAME);
		}
		return result;
	}
	if (expect_continue) {
		(void)sq_http_send_all(client, "HTTP/1.1 100 Continue\r\n\r\n",
				       sizeof("HTTP/1.1 100 Continue\r\n\r\n") - 1);
	}
	if (total_len > header_len) {
		size_t body_in_header = total_len - header_len;

		if (body_in_header > content_len) {
			body_in_header = content_len;
		}
		result = sq_http_write_chunk((const uint8_t *)headers + header_len, body_in_header);
		if (result != 0) {
			goto io_error;
		}
		received_body += body_in_header;
	}
	while (received_body < content_len) {
		size_t want = MIN(content_len - received_body, sizeof(sq_http_chunk_buffer));
		ssize_t received = zsock_recv(client, sq_http_chunk_buffer, want, 0);

		if (received < 0) {
			result = -errno;
			goto io_error;
		}
		if (received == 0) {
			result = -ECONNRESET;
			goto io_error;
		}
		result = sq_http_write_chunk(sq_http_chunk_buffer, (size_t)received);
		if (result != 0) {
			goto io_error;
		}
		received_body += (size_t)received;
	}
	result = sq_http_complete_session();
	if (result != 0) {
		goto io_error;
	}
	sq_http_send_response(client, 201, "Created", SQ_HTTP_UPLOAD_RESPONSE_OK);
	return 0;

io_error:
	if (!sq_http_is_client_abort_error(result)) {
		sq_http_record_error("upload_io", result);
	}
	(void)sq_http_preserve_partial();
	sq_http_send_response(client, 500, "Internal Server Error", SQ_HTTP_UPLOAD_RESPONSE_IO);
	return result;
}

static int sq_http_open_listener(void)
{
	struct sockaddr_in addr = {
		.sin_family = AF_INET,
		.sin_port = htons(80),
		.sin_addr = {
			.s_addr = htonl(INADDR_ANY),
		},
	};
	int opt = 1;
	int fd = zsock_socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
	int result;

	if (fd < 0) {
		return -errno;
	}
	(void)zsock_setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &opt, sizeof(opt));
	result = zsock_bind(fd, (struct sockaddr *)&addr, sizeof(addr));
	if (result < 0) {
		result = -errno;
		(void)zsock_close(fd);
		return result;
	}
	result = zsock_listen(fd, 1);
	if (result < 0) {
		result = -errno;
		(void)zsock_close(fd);
		return result;
	}
	return fd;
}

static void sq_http_configure_client_socket(int client)
{
	struct timeval timeout = {
		.tv_sec = 5,
		.tv_usec = 0,
	};

	(void)zsock_setsockopt(client, ZSOCK_SOL_SOCKET, ZSOCK_SO_RCVTIMEO, &timeout,
			       sizeof(timeout));
	(void)zsock_setsockopt(client, ZSOCK_SOL_SOCKET, ZSOCK_SO_SNDTIMEO, &timeout,
			       sizeof(timeout));
}

static void sq_http_upload_thread(void *p1, void *p2, void *p3)
{
	int listener = -1;

	ARG_UNUSED(p1);
	ARG_UNUSED(p2);
	ARG_UNUSED(p3);
	while (true) {
		if (!sq_http_route.active) {
			if (listener >= 0) {
				(void)zsock_close(listener);
				listener = -1;
			}
			k_sem_take(&sq_http_start_sem, K_FOREVER);
			continue;
		}
		if (listener < 0) {
			listener = sq_http_open_listener();
			if (listener < 0) {
				sq_http_record_error("listen", listener);
				k_sleep(K_MSEC(SQ_HTTP_UPLOAD_RETRY_MS));
				continue;
			}
		}
		struct zsock_pollfd pollfd = {
			.fd = listener,
			.events = ZSOCK_POLLIN,
		};
		int polled = zsock_poll(&pollfd, 1, SQ_HTTP_UPLOAD_POLL_MS);

		if (polled < 0) {
			sq_http_record_error("poll", -errno);
			(void)zsock_close(listener);
			listener = -1;
			continue;
		}
		if (polled == 0 || !(pollfd.revents & ZSOCK_POLLIN)) {
			continue;
		}
		int client = zsock_accept(listener, NULL, NULL);

		if (client < 0) {
			sq_http_record_error("accept", -errno);
			continue;
		}
		sq_http_configure_client_socket(client);
		(void)sq_http_handle_client(client);
		(void)zsock_close(client);
	}
}

K_THREAD_DEFINE(sq_http_upload_tid, SQ_HTTP_UPLOAD_THREAD_STACK, sq_http_upload_thread, NULL,
		NULL, NULL, SQ_HTTP_UPLOAD_THREAD_PRIORITY, 0, 0);
#endif

bool sq_http_upload_pending_is_complete(void)
{
	return sq_http_pending.active;
}

int sq_http_upload_drain_pending_event(char *app_id_out, size_t app_id_cap, char *event_out,
				       size_t event_cap)
{
	if (!sq_http_pending.active || app_id_out == NULL || event_out == NULL ||
	    app_id_cap == 0 || event_cap == 0) {
		return -EINVAL;
	}
	strncpy(app_id_out, sq_http_pending.app_id, app_id_cap - 1);
	app_id_out[app_id_cap - 1] = '\0';
	strncpy(event_out, sq_http_pending.event, event_cap - 1);
	event_out[event_cap - 1] = '\0';
	sq_http_pending.active = false;
	return 0;
}

const char *sq_http_upload_pending_staging_path(void)
{
	return sq_http_pending.staging_path;
}

const char *sq_http_upload_pending_profile_id(void)
{
	return sq_http_pending.profile_id;
}

const char *sq_http_upload_pending_name(void)
{
	return sq_http_pending.name;
}

size_t sq_http_upload_pending_bytes_received(void)
{
	return sq_http_pending.bytes_received;
}

size_t sq_http_upload_pending_total_bytes(void)
{
	return sq_http_pending.total_bytes;
}

void sq_http_upload_cleanup_staging(void)
{
	if (sq_http_pending.staging_path[0] != '\0') {
		(void)fs_unlink(sq_http_pending.staging_path);
	}
	memset(&sq_http_pending, 0, sizeof(sq_http_pending));
}

#ifdef CONFIG_ZTEST
int sq_http_upload_test_complete(const char *name, const char *staging_path,
				 size_t bytes_received, size_t total_bytes)
{
	if (!sq_http_route.active || name == NULL || staging_path == NULL ||
	    !sq_http_route_accepts_name(name)) {
		return -EINVAL;
	}
	memset(&sq_http_pending, 0, sizeof(sq_http_pending));
	strncpy(sq_http_pending.app_id, sq_http_route.app_id, sizeof(sq_http_pending.app_id) - 1);
	strncpy(sq_http_pending.profile_id, sq_http_route.profile_id,
		sizeof(sq_http_pending.profile_id) - 1);
	strncpy(sq_http_pending.event, sq_http_route.complete_event,
		sizeof(sq_http_pending.event) - 1);
	strncpy(sq_http_pending.name, name, sizeof(sq_http_pending.name) - 1);
	strncpy(sq_http_pending.staging_path, staging_path,
		sizeof(sq_http_pending.staging_path) - 1);
	sq_http_pending.bytes_received = bytes_received;
	sq_http_pending.total_bytes = total_bytes;
	sq_http_pending.active = true;
	return 0;
}

int sq_http_upload_test_begin(const char *name, size_t offset, size_t content_len,
			      size_t total_bytes)
{
	char url[SQ_HTTP_UPLOAD_MAX_NAME_LEN + sizeof(SQ_HTTP_UPLOAD_ROUTE_PREFIX)];
	int written;

	written = snprintf(url, sizeof(url), SQ_HTTP_UPLOAD_ROUTE_PREFIX "%s", name);
	if (written <= 0 || (size_t)written >= sizeof(url)) {
		return -ENAMETOOLONG;
	}
	return sq_http_begin_session(NULL, url, offset, content_len, total_bytes);
}

int sq_http_upload_test_write(const void *data, size_t len)
{
	return sq_http_write_chunk(data, len);
}

int sq_http_upload_test_finish(void)
{
	return sq_http_complete_session();
}

int sq_http_upload_test_preserve_partial(void)
{
	return sq_http_preserve_partial();
}

int sq_http_upload_test_offset(const char *name, size_t *out_offset, size_t *out_total)
{
	return sq_http_partial_offset(name, out_offset, out_total);
}
#endif
