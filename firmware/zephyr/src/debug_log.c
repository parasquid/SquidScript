#include "debug_log.h"
#include <stdarg.h>
#include <stdio.h>
#include <string.h>
#include <zephyr/kernel.h>

char sq_debug_log_buf[SQ_DEBUG_LOG_SIZE];
static size_t debug_log_pos;
static bool debug_log_wrapped;
static struct k_mutex debug_log_mutex;

void sq_debug_log_init(void)
{
	memset(sq_debug_log_buf, 0, sizeof(sq_debug_log_buf));
	debug_log_pos = 0;
	debug_log_wrapped = false;
	k_mutex_init(&debug_log_mutex);
}

void sq_debug_log_append(const char *fmt, ...)
{
	char line[SQ_DEBUG_LOG_ENTRY_LEN];
	va_list args;
	int len;

	va_start(args, fmt);
	len = vsnprintf(line, sizeof(line), fmt, args);
	va_end(args);

	if (len < 0 || (size_t)len >= sizeof(line)) {
		return;
	}

	k_mutex_lock(&debug_log_mutex, K_FOREVER);
	if (debug_log_pos + SQ_DEBUG_LOG_ENTRY_LEN > SQ_DEBUG_LOG_SIZE) {
		debug_log_pos = 0;
		debug_log_wrapped = true;
	}
	memset(sq_debug_log_buf + debug_log_pos, 0, SQ_DEBUG_LOG_ENTRY_LEN);
	memcpy(sq_debug_log_buf + debug_log_pos, line, len);
	debug_log_pos += SQ_DEBUG_LOG_ENTRY_LEN;
	k_mutex_unlock(&debug_log_mutex);
}

int sq_debug_log_read(char *out, size_t out_len)
{
	size_t total;

	if (out == NULL || out_len == 0) {
		return 0;
	}

	k_mutex_lock(&debug_log_mutex, K_FOREVER);
	if (!debug_log_wrapped) {
		total = debug_log_pos;
		if (total > out_len) {
			total = out_len;
		}
		memcpy(out, sq_debug_log_buf, total);
	} else {
		size_t first = SQ_DEBUG_LOG_SIZE - debug_log_pos;

		total = SQ_DEBUG_LOG_SIZE;
		if (total > out_len) {
			total = out_len;
		}
		if (first >= total) {
			memcpy(out, sq_debug_log_buf + debug_log_pos, total);
		} else {
			memcpy(out, sq_debug_log_buf + debug_log_pos, first);
			memcpy(out + first, sq_debug_log_buf, total - first);
		}
	}
	k_mutex_unlock(&debug_log_mutex);
	return total;
}

int sq_debug_log_line_count(void)
{
	int count;

	k_mutex_lock(&debug_log_mutex, K_FOREVER);
	if (!debug_log_wrapped) {
		count = debug_log_pos / SQ_DEBUG_LOG_ENTRY_LEN;
	} else {
		count = SQ_DEBUG_LOG_SIZE / SQ_DEBUG_LOG_ENTRY_LEN;
	}
	k_mutex_unlock(&debug_log_mutex);
	return count;
}
