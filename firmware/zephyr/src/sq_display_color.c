#include "sq_display_color.h"
#include <string.h>

static bool str_eq(const uint8_t *a, size_t a_len, const char *b)
{
	size_t b_len = strlen(b);

	return a_len == b_len && memcmp(a, b, a_len) == 0;
}

sq_display_color_t sq_display_color_parse(const uint8_t *name, size_t name_len)
{
	if (name == NULL || name_len == 0) {
		return SQ_DISPLAY_COLOR_UNSET;
	}
	if (str_eq(name, name_len, "white")) {
		return SQ_DISPLAY_COLOR_WHITE;
	}
	if (str_eq(name, name_len, "black")) {
		return SQ_DISPLAY_COLOR_BLACK;
	}
	if (name_len >= 5 && name_len <= 6 && memcmp(name, "gray", 4) == 0) {
		uint8_t value = 0;

		for (size_t i = 4; i < name_len; i++) {
			if (name[i] < '0' || name[i] > '9') {
				return SQ_DISPLAY_COLOR_UNSET;
			}
			value = (uint8_t)(value * 10 + (name[i] - '0'));
		}
		if (value > 15) {
			return SQ_DISPLAY_COLOR_UNSET;
		}
		return value;
	}
	return SQ_DISPLAY_COLOR_UNSET;
}
