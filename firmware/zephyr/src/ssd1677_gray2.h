#ifndef SQUIDSCRIPT_SSD1677_GRAY2_H
#define SQUIDSCRIPT_SSD1677_GRAY2_H

#include <stdint.h>

uint8_t sq_ssd1677_gray2_lsb_active_mask(uint8_t packed);
uint8_t sq_ssd1677_gray2_msb_active_mask(uint8_t packed);
uint8_t sq_ssd1677_gray2_bw_active_mask(uint8_t packed);

#endif
