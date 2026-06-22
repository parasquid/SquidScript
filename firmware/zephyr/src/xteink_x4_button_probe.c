#include "xteink_x4_button_probe.h"

#include "vm_runtime_internal.h"
#include "debug_log.h"

#include <errno.h>
#include <stdbool.h>
#include <string.h>

#include <zephyr/devicetree.h>
#include <zephyr/drivers/gpio.h>
#include <zephyr/kernel.h>

#ifdef CONFIG_ADC
#include <zephyr/drivers/adc.h>
#endif

#define SQ_X4_ADC_NODE DT_PATH(zephyr_user)
#define SQ_X4_ADC_CHANNEL_COUNT DT_PROP_LEN_OR(SQ_X4_ADC_NODE, io_channels, 0)
#define SQ_X4_POWER_NODE DT_NODELABEL(x4_power_button)
#define SQ_X4_ADC_RIGHT_MAX 750U
#define SQ_X4_ADC_LEFT_MAX 1600U
#define SQ_X4_ADC_SELECT_MAX 2200U
#define SQ_X4_ADC_BACK_MAX 2500U
#define SQ_X4_ADC_DOWN_MAX 750U
#define SQ_X4_ADC_UP_MAX 2200U

enum sq_x4_button_index {
	SQ_X4_BUTTON_INDEX_GPIO1 = 0,
	SQ_X4_BUTTON_INDEX_GPIO2 = 1,
	SQ_X4_BUTTON_INDEX_POWER = 2,
};

#if defined(CONFIG_ADC) && SQ_X4_ADC_CHANNEL_COUNT >= 2
static const struct adc_dt_spec x4_adc_channels[] = {
	ADC_DT_SPEC_GET_BY_IDX(SQ_X4_ADC_NODE, 0),
	ADC_DT_SPEC_GET_BY_IDX(SQ_X4_ADC_NODE, 1),
};
static bool x4_adc_ready;
#endif

#if DT_NODE_EXISTS(SQ_X4_POWER_NODE)
static const struct gpio_dt_spec x4_power = GPIO_DT_SPEC_GET(SQ_X4_POWER_NODE, gpios);
static bool x4_power_ready;
#endif

static uint32_t sq_x4_probe_errno(int result)
{
	return result < 0 ? (uint32_t)-result : 0u;
}

const char *sq_x4_button_probe_event(uint32_t logical)
{
	switch (logical) {
	case SQ_X4_BUTTON_PROBE_LOGICAL_BACK:
		return "key.BACK";
	case SQ_X4_BUTTON_PROBE_LOGICAL_SELECT:
		return "key.SELECT";
	case SQ_X4_BUTTON_PROBE_LOGICAL_LEFT:
		return "key.LEFT";
	case SQ_X4_BUTTON_PROBE_LOGICAL_RIGHT:
		return "key.RIGHT";
	case SQ_X4_BUTTON_PROBE_LOGICAL_UP:
		return "key.UP";
	case SQ_X4_BUTTON_PROBE_LOGICAL_DOWN:
		return "key.DOWN";
	case SQ_X4_BUTTON_PROBE_LOGICAL_POWER:
		return "key.POWER";
	default:
		return NULL;
	}
}

static uint32_t sq_x4_decode_gpio1(uint32_t raw)
{
	if (raw <= SQ_X4_ADC_RIGHT_MAX) {
		return SQ_X4_BUTTON_PROBE_LOGICAL_RIGHT;
	}
	if (raw <= SQ_X4_ADC_LEFT_MAX) {
		return SQ_X4_BUTTON_PROBE_LOGICAL_LEFT;
	}
	if (raw <= SQ_X4_ADC_SELECT_MAX) {
		return SQ_X4_BUTTON_PROBE_LOGICAL_SELECT;
	}
	if (raw <= SQ_X4_ADC_BACK_MAX) {
		return SQ_X4_BUTTON_PROBE_LOGICAL_BACK;
	}
	return SQ_X4_BUTTON_PROBE_LOGICAL_NONE;
}

static uint32_t sq_x4_decode_gpio2(uint32_t raw)
{
	if (raw <= SQ_X4_ADC_DOWN_MAX) {
		return SQ_X4_BUTTON_PROBE_LOGICAL_DOWN;
	}
	if (raw <= SQ_X4_ADC_UP_MAX) {
		return SQ_X4_BUTTON_PROBE_LOGICAL_UP;
	}
	return SQ_X4_BUTTON_PROBE_LOGICAL_NONE;
}

static int sq_x4_read_adc(size_t index, uint32_t *raw)
{
#if defined(CONFIG_ADC) && SQ_X4_ADC_CHANNEL_COUNT >= 2
	int16_t sample = 0;
	struct adc_sequence sequence = {
		.buffer = &sample,
		.buffer_size = sizeof(sample),
	};

	if (index >= ARRAY_SIZE(x4_adc_channels)) {
		return -EINVAL;
	}
	if (!device_is_ready(x4_adc_channels[index].dev)) {
		return -ENODEV;
	}
	if (!x4_adc_ready) {
		for (size_t i = 0; i < ARRAY_SIZE(x4_adc_channels); i++) {
			int result = adc_channel_setup_dt(&x4_adc_channels[i]);

			if (result != 0) {
				return result;
			}
		}
		x4_adc_ready = true;
	}
	int result = adc_sequence_init_dt(&x4_adc_channels[index], &sequence);

	if (result != 0) {
		return result;
	}
	result = adc_read_dt(&x4_adc_channels[index], &sequence);
	if (result != 0) {
		return result;
	}
	*raw = sample < 0 ? 0u : (uint32_t)sample;
	return 0;
#else
	ARG_UNUSED(index);
	ARG_UNUSED(raw);
	return -ENODEV;
#endif
}

static int sq_x4_read_power(uint32_t *raw, uint32_t *pressed)
{
#if DT_NODE_EXISTS(SQ_X4_POWER_NODE)
	int value;

	if (!device_is_ready(x4_power.port)) {
		return -ENODEV;
	}
	if (!x4_power_ready) {
		int result = gpio_pin_configure_dt(&x4_power, GPIO_INPUT);

		if (result != 0) {
			return result;
		}
		x4_power_ready = true;
	}
	value = gpio_pin_get_raw(x4_power.port, x4_power.pin);
	if (value < 0) {
		return value;
	}
	*raw = value == 0 ? 0u : 1u;
	value = gpio_pin_get_dt(&x4_power);
	if (value < 0) {
		return value;
	}
	*pressed = value == 0 ? 0u : 1u;
	return 0;
#else
	ARG_UNUSED(raw);
	ARG_UNUSED(pressed);
	return -ENODEV;
#endif
}

int sq_x4_button_probe_read(struct sq_x4_button_probe *out)
{
	int gpio1_result;
	int gpio2_result;
	int power_result;

	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
	gpio1_result = sq_x4_read_adc(SQ_X4_BUTTON_INDEX_GPIO1, &out->adc_gpio1_raw);
	out->adc_gpio1_error = sq_x4_probe_errno(gpio1_result);
	if (gpio1_result == 0) {
		out->adc_gpio1_logical = sq_x4_decode_gpio1(out->adc_gpio1_raw);
	}
	gpio2_result = sq_x4_read_adc(SQ_X4_BUTTON_INDEX_GPIO2, &out->adc_gpio2_raw);
	out->adc_gpio2_error = sq_x4_probe_errno(gpio2_result);
	if (gpio2_result == 0) {
		out->adc_gpio2_logical = sq_x4_decode_gpio2(out->adc_gpio2_raw);
	}
	power_result = sq_x4_read_power(&out->power_raw, &out->power_pressed);
	out->power_error = sq_x4_probe_errno(power_result);
	if (gpio1_result != 0 && gpio2_result != 0 && power_result != 0) {
		return -ENODEV;
	}
	return 0;
}

static int sq_x4_poll_logical_button(struct sq_vm_runtime *runtime, size_t index,
					 uint32_t observed, int64_t now)
{
	struct sq_vm_runtime_target_adc_button *button;
	const char *event;

	if (runtime == NULL || index >= ARRAY_SIZE(runtime->target_adc_buttons)) {
		return -EINVAL;
	}
	button = &runtime->target_adc_buttons[index];
	if (observed == button->logical) {
		button->candidate = observed;
		return 0;
	}
	if (observed != button->candidate) {
		button->candidate = observed;
		button->debounce_until_ms = now + SQ_VM_RUNTIME_INPUT_DEBOUNCE_MS;
		return 0;
	}
	if (now < button->debounce_until_ms) {
		return 0;
	}
	button->logical = observed;
	event = sq_x4_button_probe_event(observed);
	if (event == NULL) {
		return 0;
	}
	if (runtime->status == SQ_VM_RUNTIME_RUNNING) {
		int queued = sq_vm_runtime_queue_input_event(runtime, event);

		return queued == -ENOSPC ? 0 : queued;
	}
	sq_debug_log_append("%lld:btn:%s", (long long)k_uptime_get(), event);
	return sq_vm_runtime_start(runtime, &runtime->job_backend, event);
}

int sq_x4_button_probe_poll_runtime(struct sq_vm_runtime *runtime)
{
	struct sq_x4_button_probe probe;
	int64_t now;
	int result;

	if (runtime == NULL || runtime->job_backend.read_sqbc == NULL) {
		return 0;
	}
	now = k_uptime_get();
	if (now < runtime->target_adc_button_next_poll_ms) {
		return 0;
	}
	runtime->target_adc_button_next_poll_ms = now + SQ_VM_RUNTIME_INPUT_POLL_MS;
	result = sq_x4_button_probe_read(&probe);
	if (result != 0) {
		return 0;
	}
	if (probe.adc_gpio1_error == 0) {
		result = sq_x4_poll_logical_button(runtime, SQ_X4_BUTTON_INDEX_GPIO1, probe.adc_gpio1_logical, now);
		if (result != 0 || runtime->status == SQ_VM_RUNTIME_RUNNING) {
			return result;
		}
	}
	if (probe.adc_gpio2_error == 0) {
		result = sq_x4_poll_logical_button(runtime, SQ_X4_BUTTON_INDEX_GPIO2, probe.adc_gpio2_logical, now);
		if (result != 0 || runtime->status == SQ_VM_RUNTIME_RUNNING) {
			return result;
		}
	}
	if (probe.power_error == 0) {
		uint32_t power_logical = probe.power_pressed ? SQ_X4_BUTTON_PROBE_LOGICAL_POWER :
								  SQ_X4_BUTTON_PROBE_LOGICAL_NONE;
		result = sq_x4_poll_logical_button(runtime, SQ_X4_BUTTON_INDEX_POWER, power_logical, now);
		if (result != 0) {
			return result;
		}
	}
	return 0;
}
