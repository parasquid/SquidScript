#include "ssd1677_gray2.h"

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
