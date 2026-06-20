#include "vm_runtime_internal.h"

#ifndef CONFIG_SQ_TARGET_DISPLAY_LOGICAL_WIDTH
#define CONFIG_SQ_TARGET_DISPLAY_LOGICAL_WIDTH 0
#endif
#ifndef CONFIG_SQ_TARGET_DISPLAY_LOGICAL_HEIGHT
#define CONFIG_SQ_TARGET_DISPLAY_LOGICAL_HEIGHT 0
#endif
#ifndef CONFIG_SQ_TARGET_DISPLAY_PHYSICAL_WIDTH
#define CONFIG_SQ_TARGET_DISPLAY_PHYSICAL_WIDTH 0
#endif
#ifndef CONFIG_SQ_TARGET_DISPLAY_PHYSICAL_HEIGHT
#define CONFIG_SQ_TARGET_DISPLAY_PHYSICAL_HEIGHT 0
#endif
#ifndef CONFIG_SQ_TARGET_DISPLAY_ROTATION
#define CONFIG_SQ_TARGET_DISPLAY_ROTATION 0
#endif

static void runtime_display_copy_text(char *out, size_t out_cap, const uint8_t *text,
				      size_t text_len)
{
	if (out == NULL || out_cap == 0) {
		return;
	}
	size_t len = text_len;
	if (text == NULL) {
		len = 0;
	} else if (len >= out_cap) {
		len = out_cap - 1;
	}
	if (len > 0) {
		memcpy(out, text, len);
	}
	out[len] = '\0';
}

static struct sq_vm_runtime_display_op *runtime_display_append_op(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL) {
		return NULL;
	}
	size_t slot = runtime->display_op_count;
	if (slot >= SQ_VM_RUNTIME_DISPLAY_OP_MAX) {
		memmove(&runtime->display_ops[0], &runtime->display_ops[1],
			(SQ_VM_RUNTIME_DISPLAY_OP_MAX - 1) * sizeof(runtime->display_ops[0]));
		slot = SQ_VM_RUNTIME_DISPLAY_OP_MAX - 1;
		runtime->display_op_count = SQ_VM_RUNTIME_DISPLAY_OP_MAX - 1;
	}
	memset(&runtime->display_ops[slot], 0, sizeof(runtime->display_ops[slot]));
	runtime->display_op_count++;
	runtime->display_dirty = true;
	return &runtime->display_ops[slot];
}

void runtime_display_clear(void *user_data, const uint8_t *color, size_t color_len)
{
	struct sq_vm_runtime *runtime = user_data;
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];
	int written = snprintf(line, sizeof(line), "draw=clear color=%.*s", (int)color_len,
			       color == NULL ? (const uint8_t *)"" : color);

	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(runtime, line);
	}
	struct sq_vm_runtime_display_op *op = runtime_display_append_op(runtime);
	if (op != NULL) {
		op->kind = SQ_VM_RUNTIME_DISPLAY_OP_CLEAR;
		runtime_display_copy_text(op->text, sizeof(op->text), color, color_len);
	}
}

void runtime_display_text(void *user_data, const uint8_t *text, size_t text_len,
				 const SqvmDisplayTextOptions *options)
{
	struct sq_vm_runtime *runtime = user_data;
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];

	if (options == NULL) {
		return;
	}
	int written = snprintf(line, sizeof(line), "draw=text text=\"%.*s\" x=%d y=%d",
			       (int)text_len, text == NULL ? (const uint8_t *)"" : text,
			       options->x, options->y);
	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(runtime, line);
	}
	struct sq_vm_runtime_display_op *op = runtime_display_append_op(runtime);
	if (op != NULL) {
		op->kind = SQ_VM_RUNTIME_DISPLAY_OP_TEXT;
		runtime_display_copy_text(op->text, sizeof(op->text), text, text_len);
		op->x = options->x;
		op->y = options->y;
		op->font_height = options->font_height;
	}
}

void runtime_display_rect(void *user_data, const SqvmDisplayRectOptions *options)
{
	struct sq_vm_runtime *runtime = user_data;
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];

	if (options == NULL) {
		return;
	}
	int written = snprintf(line, sizeof(line), "draw=rect x=%d y=%d w=%d h=%d", options->x,
			       options->y, options->w, options->h);
	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(runtime, line);
	}
	struct sq_vm_runtime_display_op *op = runtime_display_append_op(runtime);
	if (op != NULL) {
		op->kind = SQ_VM_RUNTIME_DISPLAY_OP_RECT;
		op->x = options->x;
		op->y = options->y;
		op->w = options->w;
		op->h = options->h;
		runtime_display_copy_text(op->fill_color, sizeof(op->fill_color),
					  options->fill_color, options->fill_color_len);
		runtime_display_copy_text(op->stroke_color, sizeof(op->stroke_color),
					  options->stroke_color, options->stroke_color_len);
	}
}

void runtime_display_line(void *user_data, const SqvmDisplayLineOptions *options)
{
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];

	if (options == NULL) {
		return;
	}
	int written = snprintf(line, sizeof(line), "draw=line x1=%d y1=%d x2=%d y2=%d",
			       options->x1, options->y1, options->x2, options->y2);
	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(user_data, line);
	}
}

int32_t runtime_display_select(void *user_data, const uint8_t *name, size_t name_len)
{
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];
	int written = snprintf(line, sizeof(line), "draw=select name=%.*s", (int)name_len,
			       name == NULL ? (const uint8_t *)"" : name);

	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(user_data, line);
	}
	return 0;
}

void runtime_display_image(void *user_data, const uint8_t *path, size_t path_len,
				  const SqvmDisplayResourceOptions *options)
{
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];

	if (options == NULL) {
		return;
	}
	int written = snprintf(line, sizeof(line), "draw=image path=\"%.*s\" x=%d y=%d",
			       (int)path_len, path == NULL ? (const uint8_t *)"" : path,
			       options->x, options->y);
	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(user_data, line);
	}
}

void runtime_display_draw(void *user_data, SqvmHandle drawable,
			  const SqvmDisplayResourceOptions *options)
{
	struct sq_vm_runtime *runtime = user_data;
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];

	if (options == NULL) {
		return;
	}
	if (runtime == NULL || drawable.kind != SQVM_HANDLE_DRAWABLE || drawable.id != 1 ||
	    !runtime->drawable.active) {
		return;
	}
	int written = snprintf(line, sizeof(line), "draw=binbook id=%u x=%d y=%d",
			       (unsigned int)drawable.id, options->x, options->y);
	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(runtime, line);
	}
	struct sq_vm_runtime_display_op *op = runtime_display_append_op(runtime);
	if (op != NULL) {
		op->kind = SQ_VM_RUNTIME_DISPLAY_OP_BINBOOK_DRAWABLE;
		op->x = options->x;
		op->y = options->y;
		op->binbook_page = runtime->drawable.page;
	}
}

void runtime_display_refresh_mode(void *user_data, const uint8_t *mode, size_t mode_len)
{
	struct sq_vm_runtime *runtime = user_data;
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];
	enum sq_vm_runtime_display_refresh_mode refresh_mode = SQ_VM_RUNTIME_DISPLAY_REFRESH_AUTO;

	if (runtime == NULL || mode == NULL) {
		return;
	}
	if (mode_len == strlen("fast1bpp") && memcmp(mode, "fast1bpp", mode_len) == 0) {
		refresh_mode = SQ_VM_RUNTIME_DISPLAY_REFRESH_FAST_1BPP;
	} else if (mode_len == strlen("full") && memcmp(mode, "full", mode_len) == 0) {
		refresh_mode = SQ_VM_RUNTIME_DISPLAY_REFRESH_FULL;
	} else if (mode_len == strlen("auto") && memcmp(mode, "auto", mode_len) == 0) {
		refresh_mode = SQ_VM_RUNTIME_DISPLAY_REFRESH_AUTO;
	} else {
		int written = snprintf(line, sizeof(line), "display.refreshMode=unknown mode=%.*s",
				       (int)mode_len, mode);
		if (written > 0 && (size_t)written < sizeof(line)) {
			(void)sq_vm_runtime_record_device_error(runtime, line);
		}
		return;
	}
	runtime->display_refresh_mode = refresh_mode;
	int written = snprintf(line, sizeof(line), "draw=refresh mode=%.*s", (int)mode_len, mode);
	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(runtime, line);
	}
}

static void runtime_display_info_text(const char *text, const uint8_t **out, size_t *out_len)
{
	*out = (const uint8_t *)text;
	*out_len = strlen(text);
}

int32_t runtime_display_info(void *user_data, SqvmDisplayInfo *out)
{
	ARG_UNUSED(user_data);
	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
	out->ok = true;
	out->available = false;
#if IS_ENABLED(CONFIG_SQUIDSCRIPT_TARGET_DISPLAY_SSD1677_EXPECTED)
	out->available = true;
	runtime_display_info_text("ready", &out->status, &out->status_len);
	runtime_display_info_text("display.default", &out->binding, &out->binding_len);
	runtime_display_info_text("ssd1677", &out->driver, &out->driver_len);
	runtime_display_info_text("spi", &out->transport, &out->transport_len);
	out->width = CONFIG_SQ_TARGET_DISPLAY_LOGICAL_WIDTH;
	out->height = CONFIG_SQ_TARGET_DISPLAY_LOGICAL_HEIGHT;
	out->physical_width = CONFIG_SQ_TARGET_DISPLAY_PHYSICAL_WIDTH;
	out->physical_height = CONFIG_SQ_TARGET_DISPLAY_PHYSICAL_HEIGHT;
	out->rotation = CONFIG_SQ_TARGET_DISPLAY_ROTATION;
	runtime_display_info_text("grayscale", &out->color_model, &out->color_model_len);
	out->logical_gray_levels = 16;
	out->native_bpp = 1;
	runtime_display_info_text("GRAY1_PACKED", &out->native_pixel_format,
				  &out->native_pixel_format_len);
	out->default_font_height = 20;
	out->supports_partial_refresh = true;
	out->supports_fast_refresh = true;
	return 0;
#else
	runtime_display_info_text("drawlog", &out->status, &out->status_len);
	runtime_display_info_text("display.default", &out->binding, &out->binding_len);
	runtime_display_info_text("drawlog", &out->driver, &out->driver_len);
	runtime_display_info_text("memory", &out->transport, &out->transport_len);
	out->width = 0;
	out->height = 0;
	out->physical_width = 0;
	out->physical_height = 0;
	out->rotation = 0;
	runtime_display_info_text("grayscale", &out->color_model, &out->color_model_len);
	out->logical_gray_levels = 16;
	out->native_bpp = 0;
	runtime_display_info_text("DRAWLOG", &out->native_pixel_format,
				  &out->native_pixel_format_len);
	out->default_font_height = 0;
	out->supports_partial_refresh = false;
	out->supports_fast_refresh = false;
	return 0;
#endif
}
