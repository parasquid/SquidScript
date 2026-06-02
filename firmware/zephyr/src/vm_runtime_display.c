#include "vm_runtime_internal.h"

void runtime_display_clear(void *user_data, const uint8_t *color, size_t color_len)
{
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];
	int written = snprintf(line, sizeof(line), "draw=clear color=%.*s", (int)color_len,
			       color == NULL ? (const uint8_t *)"" : color);

	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(user_data, line);
	}
}

void runtime_display_text(void *user_data, const uint8_t *text, size_t text_len,
				 const SqvmDisplayTextOptions *options)
{
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];

	if (options == NULL) {
		return;
	}
	int written = snprintf(line, sizeof(line), "draw=text text=\"%.*s\" x=%d y=%d",
			       (int)text_len, text == NULL ? (const uint8_t *)"" : text,
			       options->x, options->y);
	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(user_data, line);
	}
}

void runtime_display_rect(void *user_data, const SqvmDisplayRectOptions *options)
{
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];

	if (options == NULL) {
		return;
	}
	int written = snprintf(line, sizeof(line), "draw=rect x=%d y=%d w=%d h=%d", options->x,
			       options->y, options->w, options->h);
	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(user_data, line);
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

void runtime_display_draw(void *user_data, const uint8_t *drawable, size_t drawable_len,
				 const SqvmDisplayResourceOptions *options)
{
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];

	if (options == NULL) {
		return;
	}
	int written = snprintf(line, sizeof(line), "draw=resource drawable=\"%.*s\" x=%d y=%d",
			       (int)drawable_len,
			       drawable == NULL ? (const uint8_t *)"" : drawable, options->x,
			       options->y);
	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(user_data, line);
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
}

