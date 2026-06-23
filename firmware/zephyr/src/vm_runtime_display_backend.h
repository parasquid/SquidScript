#ifndef SQUIDSCRIPT_VM_RUNTIME_DISPLAY_BACKEND_H
#define SQUIDSCRIPT_VM_RUNTIME_DISPLAY_BACKEND_H

#include "vm_runtime.h"

void sq_display_backend_rasterize_clear(sq_display_color_t color);
void sq_display_backend_rasterize_text(const char *text, int32_t x, int32_t y,
				       int32_t font_height, sq_display_color_t color);
void sq_display_backend_rasterize_rect(int32_t x, int32_t y, int32_t w, int32_t h,
				       sq_display_color_t fill, sq_display_color_t stroke);
void sq_display_backend_rasterize_binbook(const struct sq_vm_runtime_binbook_page *page);
int sq_display_backend_flush_framebuffer(enum sq_vm_runtime_display_refresh_mode mode);

const uint8_t *sq_display_backend_framebuffer(void);
size_t sq_display_backend_framebuffer_size(void);

int sq_display_backend_window_probe(const char *pattern);
void sq_display_backend_reset(void);

#endif
