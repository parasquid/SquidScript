#include "ssd1677_gray2.h"

#include <stddef.h>

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
