#ifndef SQUIDSCRIPT_SSD1677_GRAY2_H
#define SQUIDSCRIPT_SSD1677_GRAY2_H

#include <stdint.h>
#include <stdbool.h>

enum sq_ssd1677_binbook_refresh_kind {
	SQ_SSD1677_BINBOOK_REFRESH_GRAY2_FULL,
	SQ_SSD1677_BINBOOK_REFRESH_BW_DIFFERENTIAL_PARTIAL,
};

struct sq_ssd1677_binbook_refresh_state {
	bool previous_page_valid;
	uint32_t fast_refresh_count;
};

uint8_t sq_ssd1677_gray2_lsb_active_mask(uint8_t packed);
uint8_t sq_ssd1677_gray2_msb_active_mask(uint8_t packed);
uint8_t sq_ssd1677_gray2_bw_active_mask(uint8_t packed);
uint8_t sq_ssd1677_gray2_ordered_dither_bw_active_mask(uint8_t packed, uint16_t x,
						       uint16_t y);
enum sq_ssd1677_binbook_refresh_kind sq_ssd1677_binbook_refresh_decide(
	const struct sq_ssd1677_binbook_refresh_state *state, uint32_t full_refresh_cadence);
void sq_ssd1677_binbook_refresh_record(struct sq_ssd1677_binbook_refresh_state *state,
				       enum sq_ssd1677_binbook_refresh_kind refresh);
void sq_ssd1677_binbook_refresh_reset(struct sq_ssd1677_binbook_refresh_state *state);

#endif
