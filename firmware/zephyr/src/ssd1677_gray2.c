#include "ssd1677_gray2.h"

#include <stddef.h>
#include <string.h>

static uint8_t gray2_to_xth(uint8_t gray)
{
	static const uint8_t map[4] = {0U, 1U, 2U, 3U};

	return map[gray & 0x03U];
}

static uint8_t gray2_plane_active_mask(uint8_t packed, uint8_t plane_bit)
{
	uint8_t mask = 0U;

	for (uint8_t pixel = 0; pixel < 4U; ++pixel) {
		uint8_t gray = (uint8_t)((packed >> (6U - pixel * 2U)) & 0x03U);
		uint8_t xth = gray2_to_xth(gray);

		if ((xth & plane_bit) != 0U) {
			mask |= (uint8_t)(0x80U >> pixel);
		}
	}
	return mask;
}

uint8_t sq_ssd1677_gray2_lsb_active_mask(uint8_t packed)
{
	return gray2_plane_active_mask(packed, 0x01U);
}

uint8_t sq_ssd1677_gray2_msb_active_mask(uint8_t packed)
{
	return gray2_plane_active_mask(packed, 0x02U);
}

uint8_t sq_ssd1677_gray2_bw_active_mask(uint8_t packed)
{
	uint8_t mask = 0U;

	for (uint8_t pixel = 0; pixel < 4U; ++pixel) {
		uint8_t gray = (uint8_t)((packed >> (6U - pixel * 2U)) & 0x03U);

		if (gray < 2U) {
			mask |= (uint8_t)(0x80U >> pixel);
		}
	}
	return mask;
}

uint8_t sq_ssd1677_gray2_ordered_dither_bw_active_mask(uint8_t packed, uint16_t x,
						       uint16_t y)
{
	static const uint8_t threshold[2][2] = {
		{0U, 2U},
		{1U, 1U},
	};
	uint8_t mask = 0U;

	for (uint8_t pixel = 0; pixel < 4U; ++pixel) {
		uint8_t gray = (uint8_t)((packed >> (6U - pixel * 2U)) & 0x03U);
		uint8_t dither_threshold = threshold[y & 0x01U][(x + pixel) & 0x01U];

		if (gray <= dither_threshold) {
			mask |= (uint8_t)(0x80U >> pixel);
		}
	}
	return mask;
}

enum sq_ssd1677_binbook_refresh_kind sq_ssd1677_binbook_refresh_decide(
	const struct sq_ssd1677_binbook_refresh_state *state, uint32_t full_refresh_cadence)
{
	if (state == NULL || !state->previous_page_valid || state->fast_refresh_count == 0U ||
	    state->fast_refresh_count >= full_refresh_cadence) {
		return SQ_SSD1677_BINBOOK_REFRESH_GRAY2_FULL;
	}
	return SQ_SSD1677_BINBOOK_REFRESH_BW_DIFFERENTIAL_PARTIAL;
}

void sq_ssd1677_binbook_refresh_record(struct sq_ssd1677_binbook_refresh_state *state,
				       enum sq_ssd1677_binbook_refresh_kind refresh)
{
	if (state == NULL) {
		return;
	}
	state->previous_page_valid = true;
	if (refresh == SQ_SSD1677_BINBOOK_REFRESH_GRAY2_FULL) {
		state->fast_refresh_count = 1U;
	} else if (state->fast_refresh_count < UINT32_MAX) {
		state->fast_refresh_count++;
	}
}

void sq_ssd1677_binbook_refresh_reset(struct sq_ssd1677_binbook_refresh_state *state)
{
	if (state == NULL) {
		return;
	}
	state->previous_page_valid = false;
	state->fast_refresh_count = 0U;
}

enum sq_ssd1677_composed_refresh_kind sq_ssd1677_composed_refresh_decide(
	const struct sq_ssd1677_composed_refresh_state *state,
	enum sq_vm_runtime_display_refresh_mode refresh)
{
	if (refresh != SQ_VM_RUNTIME_DISPLAY_REFRESH_FAST_1BPP || state == NULL ||
	    !state->previous_ops_valid) {
		return SQ_SSD1677_COMPOSED_REFRESH_FULL_SEED;
	}
	return SQ_SSD1677_COMPOSED_REFRESH_BW_DIFFERENTIAL_PARTIAL;
}

void sq_ssd1677_composed_refresh_record(struct sq_ssd1677_composed_refresh_state *state,
					enum sq_ssd1677_composed_refresh_kind refresh)
{
	if (state == NULL) {
		return;
	}
	(void)refresh;
	state->previous_ops_valid = true;
}

void sq_ssd1677_composed_refresh_reset(struct sq_ssd1677_composed_refresh_state *state)
{
	if (state == NULL) {
		return;
	}
	state->previous_ops_valid = false;
}

static bool composed_op_equal(const struct sq_vm_runtime_display_op *a,
			      const struct sq_vm_runtime_display_op *b)
{
	if (a == NULL || b == NULL || a->kind != b->kind) {
		return false;
	}
	return strcmp(a->text, b->text) == 0 && strcmp(a->fill_color, b->fill_color) == 0 &&
	       strcmp(a->stroke_color, b->stroke_color) == 0 && a->x == b->x && a->y == b->y &&
	       a->w == b->w && a->h == b->h && a->font_height == b->font_height;
}

static const char *last_clear_color(const struct sq_vm_runtime_display_op *ops, size_t op_count)
{
	const char *color = NULL;

	for (size_t i = 0; i < op_count; ++i) {
		if (ops[i].kind == SQ_VM_RUNTIME_DISPLAY_OP_CLEAR) {
			color = ops[i].text;
		}
	}
	return color;
}

static bool op_exists_in(const struct sq_vm_runtime_display_op *op,
			 const struct sq_vm_runtime_display_op *ops, size_t op_count)
{
	for (size_t i = 0; i < op_count; ++i) {
		if (composed_op_equal(op, &ops[i])) {
			return true;
		}
	}
	return false;
}

static bool logical_to_physical_point(uint16_t logical_x, uint16_t logical_y,
				      uint16_t logical_width, uint16_t logical_height,
				      uint16_t physical_width, uint16_t physical_height,
				      uint16_t rotation, uint16_t *physical_x,
				      uint16_t *physical_y)
{
	if (physical_x == NULL || physical_y == NULL || logical_x >= logical_width ||
	    logical_y >= logical_height) {
		return false;
	}
	switch (rotation) {
	case 0:
		*physical_x = logical_x;
		*physical_y = logical_y;
		break;
	case 90:
		*physical_x = logical_y;
		*physical_y = (uint16_t)(logical_width - 1U - logical_x);
		break;
	case 180:
		*physical_x = (uint16_t)(logical_width - 1U - logical_x);
		*physical_y = (uint16_t)(logical_height - 1U - logical_y);
		break;
	case 270:
		*physical_x = (uint16_t)(logical_height - 1U - logical_y);
		*physical_y = logical_x;
		break;
	default:
		return false;
	}
	return *physical_x < physical_width && *physical_y < physical_height;
}

static void window_include_point(struct sq_ssd1677_window *window, uint16_t x, uint16_t y)
{
	if (!window->valid) {
		window->valid = true;
		window->x0 = x;
		window->x1 = x;
		window->y0 = y;
		window->y1 = y;
		return;
	}
	if (x < window->x0) {
		window->x0 = x;
	}
	if (x > window->x1) {
		window->x1 = x;
	}
	if (y < window->y0) {
		window->y0 = y;
	}
	if (y > window->y1) {
		window->y1 = y;
	}
}

static void window_include_full(struct sq_ssd1677_window *window, uint16_t physical_width,
				uint16_t physical_height)
{
	if (physical_width == 0 || physical_height == 0) {
		return;
	}
	window->valid = true;
	window->x0 = 0;
	window->y0 = 0;
	window->x1 = (uint16_t)(physical_width - 1U);
	window->y1 = (uint16_t)(physical_height - 1U);
}

static void window_align_x_to_ram_bytes(struct sq_ssd1677_window *window,
					uint16_t physical_width)
{
	uint16_t ram_x0;
	uint16_t ram_x1;
	uint16_t ram_byte0;
	uint16_t ram_byte1;

	if (window == NULL || !window->valid || physical_width == 0 ||
	    window->x0 >= physical_width || window->x1 >= physical_width) {
		return;
	}
	ram_x0 = (uint16_t)(physical_width - 1U - window->x1);
	ram_x1 = (uint16_t)(physical_width - 1U - window->x0);
	ram_byte0 = (uint16_t)(ram_x0 / 8U);
	ram_byte1 = (uint16_t)(ram_x1 / 8U);
	window->x0 = (uint16_t)(physical_width - 1U - (ram_byte1 * 8U + 7U));
	window->x1 = (uint16_t)(physical_width - 1U - ram_byte0 * 8U);
}

static void window_include_logical_rect(struct sq_ssd1677_window *window, int32_t x, int32_t y,
					int32_t w, int32_t h, uint16_t logical_width,
					uint16_t logical_height, uint16_t physical_width,
					uint16_t physical_height, uint16_t rotation)
{
	int32_t left = x;
	int32_t top = y;
	int32_t right = x + w;
	int32_t bottom = y + h;

	if (w <= 0 || h <= 0 || logical_width == 0 || logical_height == 0 ||
	    physical_width == 0 || physical_height == 0) {
		return;
	}
	if (left < 0) {
		left = 0;
	}
	if (top < 0) {
		top = 0;
	}
	if (right > logical_width) {
		right = logical_width;
	}
	if (bottom > logical_height) {
		bottom = logical_height;
	}
	if (left >= right || top >= bottom) {
		return;
	}

	const uint16_t lx0 = (uint16_t)left;
	const uint16_t ly0 = (uint16_t)top;
	const uint16_t lx1 = (uint16_t)(right - 1);
	const uint16_t ly1 = (uint16_t)(bottom - 1);
	const uint16_t corners[4][2] = {
		{lx0, ly0},
		{lx1, ly0},
		{lx0, ly1},
		{lx1, ly1},
	};

	for (size_t i = 0; i < 4; ++i) {
		uint16_t physical_x = 0;
		uint16_t physical_y = 0;

		if (logical_to_physical_point(corners[i][0], corners[i][1], logical_width,
					      logical_height, physical_width, physical_height,
					      rotation, &physical_x, &physical_y)) {
			window_include_point(window, physical_x, physical_y);
		}
	}
}

static void window_include_op(struct sq_ssd1677_window *window,
			      const struct sq_vm_runtime_display_op *op, uint16_t logical_width,
			      uint16_t logical_height, uint16_t physical_width,
			      uint16_t physical_height, uint16_t rotation)
{
	if (op == NULL) {
		return;
	}
	switch (op->kind) {
	case SQ_VM_RUNTIME_DISPLAY_OP_RECT:
		window_include_logical_rect(window, op->x, op->y, op->w, op->h, logical_width,
					    logical_height, physical_width, physical_height,
					    rotation);
		break;
	case SQ_VM_RUNTIME_DISPLAY_OP_TEXT: {
		int32_t scale = op->font_height / 7;
		if (scale <= 0) {
			scale = 1;
		}
		int32_t text_len = 0;
		while (text_len < (int32_t)sizeof(op->text) && op->text[text_len] != '\0') {
			text_len++;
		}
		window_include_logical_rect(window, op->x, op->y, text_len * 6 * scale,
					    7 * scale, logical_width, logical_height,
					    physical_width, physical_height, rotation);
		break;
	}
	case SQ_VM_RUNTIME_DISPLAY_OP_BINBOOK_DRAWABLE:
		window_include_full(window, physical_width, physical_height);
		break;
	default:
		break;
	}
}

bool sq_ssd1677_composed_dirty_window(
	const struct sq_vm_runtime_display_op *previous_ops, size_t previous_op_count,
	const struct sq_vm_runtime_display_op *current_ops, size_t current_op_count,
	uint16_t logical_width, uint16_t logical_height, uint16_t physical_width,
	uint16_t physical_height, uint16_t rotation, struct sq_ssd1677_window *out)
{
	const char *previous_clear = NULL;
	const char *current_clear = NULL;

	if (out == NULL || previous_ops == NULL || current_ops == NULL) {
		return false;
	}
	memset(out, 0, sizeof(*out));
	previous_clear = last_clear_color(previous_ops, previous_op_count);
	current_clear = last_clear_color(current_ops, current_op_count);
	if ((previous_clear == NULL) != (current_clear == NULL) ||
	    (previous_clear != NULL && strcmp(previous_clear, current_clear) != 0)) {
		window_include_full(out, physical_width, physical_height);
		return out->valid;
	}

	for (size_t i = 0; i < previous_op_count; ++i) {
		if (previous_ops[i].kind != SQ_VM_RUNTIME_DISPLAY_OP_CLEAR &&
		    !op_exists_in(&previous_ops[i], current_ops, current_op_count)) {
			window_include_op(out, &previous_ops[i], logical_width, logical_height,
					  physical_width, physical_height, rotation);
		}
	}
	for (size_t i = 0; i < current_op_count; ++i) {
		if (current_ops[i].kind != SQ_VM_RUNTIME_DISPLAY_OP_CLEAR &&
		    !op_exists_in(&current_ops[i], previous_ops, previous_op_count)) {
			window_include_op(out, &current_ops[i], logical_width, logical_height,
					  physical_width, physical_height, rotation);
		}
	}
	window_align_x_to_ram_bytes(out, physical_width);
	return out->valid;
}
