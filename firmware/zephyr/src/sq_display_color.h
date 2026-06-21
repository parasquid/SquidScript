#ifndef SQ_DISPLAY_COLOR_H
#define SQ_DISPLAY_COLOR_H

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

typedef uint8_t sq_display_color_t;

#define SQ_DISPLAY_COLOR_UNSET ((sq_display_color_t)0xFF)
#define SQ_DISPLAY_COLOR_WHITE ((sq_display_color_t)0)
#define SQ_DISPLAY_COLOR_BLACK ((sq_display_color_t)15)

static inline bool sq_display_color_is_black(sq_display_color_t color)
{
	return color == SQ_DISPLAY_COLOR_BLACK;
}

static inline bool sq_display_color_is_set(sq_display_color_t color)
{
	return color != SQ_DISPLAY_COLOR_UNSET;
}

#endif
