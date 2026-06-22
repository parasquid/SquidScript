#ifndef SQUIDSCRIPT_DEBUG_LOG_H
#define SQUIDSCRIPT_DEBUG_LOG_H

#include <stddef.h>
#include <stdint.h>

#define SQ_DEBUG_LOG_SIZE 8192
#define SQ_DEBUG_LOG_ENTRY_LEN 64
#define SQ_DEBUG_LOG_MAX_RESPONSE_ENTRIES 64

void sq_debug_log_init(void);
void sq_debug_log_append(const char *fmt, ...);
int sq_debug_log_read(char *out, size_t out_len);
int sq_debug_log_line_count(void);

extern char sq_debug_log_buf[SQ_DEBUG_LOG_SIZE];

#endif
