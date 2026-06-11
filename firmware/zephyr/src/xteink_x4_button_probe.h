#ifndef XTEINK_X4_BUTTON_PROBE_H
#define XTEINK_X4_BUTTON_PROBE_H

#include <stdint.h>

struct sq_vm_runtime;

enum sq_x4_button_probe_logical {
	SQ_X4_BUTTON_PROBE_LOGICAL_NONE = 0,
	SQ_X4_BUTTON_PROBE_LOGICAL_BACK = 1,
	SQ_X4_BUTTON_PROBE_LOGICAL_SELECT = 2,
	SQ_X4_BUTTON_PROBE_LOGICAL_LEFT = 3,
	SQ_X4_BUTTON_PROBE_LOGICAL_RIGHT = 4,
	SQ_X4_BUTTON_PROBE_LOGICAL_UP = 5,
	SQ_X4_BUTTON_PROBE_LOGICAL_DOWN = 6,
};

struct sq_x4_button_probe {
	uint32_t adc_gpio1_raw;
	uint32_t adc_gpio1_logical;
	uint32_t adc_gpio1_error;
	uint32_t adc_gpio2_raw;
	uint32_t adc_gpio2_logical;
	uint32_t adc_gpio2_error;
	uint32_t power_raw;
	uint32_t power_pressed;
	uint32_t power_error;
};

int sq_x4_button_probe_read(struct sq_x4_button_probe *out);
int sq_x4_button_probe_poll_runtime(struct sq_vm_runtime *runtime);

#endif /* XTEINK_X4_BUTTON_PROBE_H */
