#include "vm_runtime_internal.h"

#include "squidscript_target_defaults.h"
#include "debug_log.h"

#include <zephyr/devicetree.h>
#include <zephyr/drivers/gpio.h>
#include <zephyr/drivers/pwm.h>

#define SQ_VM_RUNTIME_BREATHE_LEVEL_MS 31

static int configure_raw_gpio(struct sq_vm_runtime *runtime, uint8_t pin);
static int read_input_button_gpio(uint8_t pin, bool active_low, bool *pressed);

static const uint8_t indicator_breathe_duties[SQ_VM_RUNTIME_INDICATOR_BREATHE_STEPS] = {
	0,  0,  1,  2,	4,  6,  8,  11, 15, 18, 22, 26, 31, 35, 40, 45, 50,
	55, 60, 65, 69, 74, 78, 82, 85, 89, 92, 94, 96, 98, 99, 100, 100, 100,
	99, 98, 96, 94, 92, 89, 85, 82, 78, 74, 69, 65, 60, 55, 50, 45, 40,
	35, 31, 26, 22, 18, 15, 11, 8,  6,  4,  2,	1,  0,  0,
};

#if IS_ENABLED(CONFIG_PWM) && DT_NODE_HAS_PROP(DT_ALIAS(indicator0), pwms)
static const struct pwm_dt_spec indicator_pwm = PWM_DT_SPEC_GET(DT_ALIAS(indicator0));
#define SQ_VM_RUNTIME_HAS_INDICATOR_PWM 1
#else
#define SQ_VM_RUNTIME_HAS_INDICATOR_PWM 0
#endif

#if IS_ENABLED(CONFIG_GPIO) && DT_NODE_HAS_PROP(DT_ALIAS(indicator0), gpios)
#define SQ_VM_RUNTIME_INDICATOR_GPIO_NODE DT_ALIAS(indicator0)
#elif IS_ENABLED(CONFIG_GPIO) && DT_NODE_HAS_PROP(DT_ALIAS(led0), gpios)
#define SQ_VM_RUNTIME_INDICATOR_GPIO_NODE DT_ALIAS(led0)
#endif

#ifdef SQ_VM_RUNTIME_INDICATOR_GPIO_NODE
static const struct gpio_dt_spec indicator_gpio =
	GPIO_DT_SPEC_GET(SQ_VM_RUNTIME_INDICATOR_GPIO_NODE, gpios);
#define SQ_VM_RUNTIME_HAS_INDICATOR_GPIO 1
#else
#define SQ_VM_RUNTIME_HAS_INDICATOR_GPIO 0
#endif

#if IS_ENABLED(CONFIG_GPIO) && DT_NODE_HAS_STATUS(DT_NODELABEL(gpio0), okay)
static const struct device *const gpio0_dev = DEVICE_DT_GET(DT_NODELABEL(gpio0));
#define SQ_VM_RUNTIME_HAS_GPIO0 1
#else
#define SQ_VM_RUNTIME_HAS_GPIO0 0
#endif

#if IS_ENABLED(CONFIG_GPIO) && DT_NODE_EXISTS(DT_ALIAS(sw0)) && \
	DT_NODE_HAS_PROP(DT_ALIAS(sw0), gpios)
static const struct gpio_dt_spec input_sw0_gpio = GPIO_DT_SPEC_GET(DT_ALIAS(sw0), gpios);
#define SQ_VM_RUNTIME_HAS_SW0_GPIO 1
#else
#define SQ_VM_RUNTIME_HAS_SW0_GPIO 0
#endif

int32_t runtime_indicator_write(void *user_data, bool value)
{
	return sq_vm_runtime_indicator_write(user_data, value);
}

int32_t runtime_indicator_toggle(void *user_data)
{
	return sq_vm_runtime_indicator_toggle(user_data);
}

int32_t runtime_indicator_read(void *user_data, bool *out)
{
	return sq_vm_runtime_indicator_read(user_data, out);
}

int32_t runtime_indicator_breathe(void *user_data)
{
	int result = sq_vm_runtime_indicator_breathe(user_data);

	return result == -ENODEV ? 0 : result;
}

int32_t runtime_indicator_blink(void *user_data, int32_t on_ms, int32_t off_ms)
{
	return sq_vm_runtime_indicator_blink(user_data, on_ms, off_ms);
}

int32_t runtime_hardware_gpio_write(void *user_data, const uint8_t *name, size_t name_len,
					   bool value)
{
	return sq_vm_runtime_hardware_gpio_write(user_data, name, name_len, value);
}

int32_t runtime_hardware_gpio_toggle(void *user_data, const uint8_t *name, size_t name_len)
{
	return sq_vm_runtime_hardware_gpio_toggle(user_data, name, name_len);
}

int32_t runtime_hardware_gpio_read(void *user_data, const uint8_t *name, size_t name_len,
					  bool *out)
{
	return sq_vm_runtime_hardware_gpio_read(user_data, name, name_len, out);
}

static int configure_indicator_gpio(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL || runtime->indicator_gpio_configured) {
		return 0;
	}
	runtime->indicator_gpio_configured = true;
#if SQ_VM_RUNTIME_HAS_INDICATOR_GPIO
	if (!gpio_is_ready_dt(&indicator_gpio)) {
		return 0;
	}
	if (gpio_pin_configure_dt(&indicator_gpio, GPIO_OUTPUT_INACTIVE) != 0) {
		return 0;
	}
	runtime->indicator_gpio_available = true;
#endif
	return 0;
}

static bool runtime_indicator_active_low(const struct sq_vm_runtime *runtime)
{
	return runtime != NULL && runtime->indicator_binding_active &&
	       runtime->indicator_binding_active_low;
}

static bool indicator_uses_dt_gpio_pin(uint8_t pin)
{
#if SQ_VM_RUNTIME_HAS_INDICATOR_GPIO
	return indicator_gpio.pin == pin;
#else
	ARG_UNUSED(pin);
	return false;
#endif
}

static uint8_t runtime_indicator_pin(const struct sq_vm_runtime *runtime)
{
	if (runtime != NULL && runtime->indicator_binding_active) {
		return runtime->indicator_binding_pin;
	}
	return 0;
}

static bool runtime_indicator_uses_pin(const struct sq_vm_runtime *runtime, uint8_t pin)
{
	return runtime != NULL && runtime->indicator_binding_active &&
	       runtime->indicator_binding_pin == pin;
}

static int set_indicator_raw_output(struct sq_vm_runtime *runtime, bool raw_high)
{
#if SQ_VM_RUNTIME_HAS_INDICATOR_PWM
	if (pwm_is_ready_dt(&indicator_pwm)) {
		uint32_t pulse = raw_high ? indicator_pwm.period : 0U;
		return pwm_set_dt(&indicator_pwm, indicator_pwm.period, pulse);
	}
#endif
	(void)configure_indicator_gpio(runtime);
#if SQ_VM_RUNTIME_HAS_INDICATOR_GPIO
	if (runtime->indicator_gpio_available) {
		int result = gpio_pin_set_raw(indicator_gpio.port, indicator_gpio.pin, raw_high ? 1 : 0);
		if (result != 0) {
			return result;
		}
	}
#endif
	return 0;
}

static int set_indicator_brightness(struct sq_vm_runtime *runtime, uint8_t brightness)
{
	uint8_t clamped = brightness > 100U ? 100U : brightness;
	uint8_t pin = runtime_indicator_pin(runtime);
	bool active_low = runtime_indicator_active_low(runtime);
	ARG_UNUSED(active_low);

	if (runtime == NULL || !runtime->indicator_binding_active) {
		return -ENODEV;
	}
#if SQ_VM_RUNTIME_HAS_INDICATOR_PWM
	uint8_t raw_high_percent = active_low ? (uint8_t)(100U - clamped) : clamped;
#endif

	runtime->indicator_state = clamped > 0U;
#if SQ_VM_RUNTIME_HAS_INDICATOR_PWM
	if (indicator_uses_dt_gpio_pin(pin) && pwm_is_ready_dt(&indicator_pwm)) {
		uint32_t pulse = (indicator_pwm.period * (uint32_t)raw_high_percent) / 100U;
		return pwm_set_dt(&indicator_pwm, indicator_pwm.period, pulse);
	}
#endif
	if (!indicator_uses_dt_gpio_pin(pin)) {
		int result = configure_raw_gpio(runtime, pin);
		if (result != 0) {
			return result;
		}
#if SQ_VM_RUNTIME_HAS_GPIO0
		bool raw_high = active_low ? clamped == 0U : clamped > 0U;
		if (device_is_ready(gpio0_dev)) {
			return gpio_pin_set_raw(gpio0_dev, pin, raw_high ? 1 : 0);
		}
#endif
		return 0;
	}
	(void)configure_indicator_gpio(runtime);
#if SQ_VM_RUNTIME_HAS_INDICATOR_GPIO
	if (runtime->indicator_gpio_available) {
		int result = gpio_pin_set_dt(&indicator_gpio, clamped > 0U ? 1 : 0);
		if (result != 0) {
			return result;
		}
	}
#endif
	return 0;
}

int sq_vm_runtime_indicator_write(struct sq_vm_runtime *runtime, bool value)
{
	if (runtime == NULL) {
		return -EINVAL;
	}
	runtime->indicator_pattern = SQ_VM_RUNTIME_INDICATOR_STEADY;
	return set_indicator_brightness(runtime, value ? 100U : 0U);
}

int sq_vm_runtime_indicator_toggle(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL) {
		return -EINVAL;
	}
	return sq_vm_runtime_indicator_write(runtime, !runtime->indicator_state);
}

int sq_vm_runtime_indicator_read(struct sq_vm_runtime *runtime, bool *out)
{
	if (runtime == NULL || out == NULL) {
		return -EINVAL;
	}
	*out = runtime->indicator_state;
	return 0;
}

int sq_vm_runtime_indicator_breathe(struct sq_vm_runtime *runtime)
{
	int64_t now;

	if (runtime == NULL) {
		return -EINVAL;
	}
	now = k_uptime_get();
	runtime->indicator_pattern = SQ_VM_RUNTIME_INDICATOR_BREATHE;
	runtime->indicator_pattern_step = 0;
	runtime->indicator_pattern_on = false;
	runtime->indicator_pattern_on_ms = 0;
	runtime->indicator_pattern_off_ms = 0;
	runtime->indicator_pattern_next_ms = now;
	return set_indicator_brightness(runtime, 0U);
}

int sq_vm_runtime_indicator_blink(struct sq_vm_runtime *runtime, int32_t on_ms, int32_t off_ms)
{
	int64_t now;

	if (runtime == NULL || on_ms <= 0 || off_ms <= 0) {
		return -EINVAL;
	}
	now = k_uptime_get();
	runtime->indicator_pattern = SQ_VM_RUNTIME_INDICATOR_BLINK;
	runtime->indicator_pattern_step = 0;
	runtime->indicator_pattern_on = true;
	runtime->indicator_pattern_on_ms = on_ms;
	runtime->indicator_pattern_off_ms = off_ms;
	runtime->indicator_pattern_next_ms = now + on_ms;
	return set_indicator_brightness(runtime, 100U);
}

int parse_gpio_name(const uint8_t *name, size_t name_len, uint8_t *pin)
{
	uint32_t value = 0;

	if (name == NULL || pin == NULL || name_len < 5 || name_len > 6 ||
	    memcmp(name, "GPIO", 4) != 0) {
		return -EINVAL;
	}
	for (size_t i = 4; i < name_len; i++) {
		if (name[i] < '0' || name[i] > '9') {
			return -EINVAL;
		}
		value = (value * 10U) + (uint32_t)(name[i] - '0');
	}
	if (value > 25U) {
		return -EINVAL;
	}
	*pin = (uint8_t)value;
	return 0;
}

bool target_gpio_pin_supported(uint8_t pin)
{
	if (pin >= 64U) {
		return false;
	}
	return (SQ_TARGET_GPIO_CAPABLE_MASK & (1ULL << pin)) != 0ULL;
}

static int configure_raw_gpio(struct sq_vm_runtime *runtime, uint8_t pin)
{
	uint32_t bit = BIT(pin);

	if ((runtime->gpio_configured_mask & bit) != 0) {
		return 0;
	}
#if SQ_VM_RUNTIME_HAS_GPIO0
	if (device_is_ready(gpio0_dev)) {
		int result = gpio_pin_configure(gpio0_dev, pin, GPIO_OUTPUT);
		if (result != 0) {
			return result;
		}
	}
#endif
	runtime->gpio_configured_mask |= bit;
	return 0;
}

int configure_input_button_gpio(uint8_t pin, bool active_low, bool *pressed)
{
	if (pressed == NULL || !target_gpio_pin_supported(pin)) {
		return -EINVAL;
	}
#if SQ_VM_RUNTIME_HAS_SW0_GPIO
	if (pin == input_sw0_gpio.pin) {
		if (device_is_ready(input_sw0_gpio.port)) {
			int result = gpio_pin_configure_dt(&input_sw0_gpio, GPIO_INPUT);
			if (result != 0) {
				return result;
			}
			return read_input_button_gpio(pin, active_low, pressed);
		}
		*pressed = false;
		return 0;
	}
#endif
#if SQ_VM_RUNTIME_HAS_GPIO0
	if (device_is_ready(gpio0_dev)) {
		int flags = GPIO_INPUT | (active_low ? GPIO_PULL_UP : GPIO_PULL_DOWN);
		int result = gpio_pin_configure(gpio0_dev, pin, flags);
		if (result != 0) {
			return result;
		}
		return read_input_button_gpio(pin, active_low, pressed);
	}
#endif
	*pressed = false;
	return 0;
}

static int read_input_button_gpio(uint8_t pin, bool active_low, bool *pressed)
{
	if (pressed == NULL || !target_gpio_pin_supported(pin)) {
		return -EINVAL;
	}
#if SQ_VM_RUNTIME_HAS_SW0_GPIO
	if (pin == input_sw0_gpio.pin) {
		if (device_is_ready(input_sw0_gpio.port)) {
			int raw = gpio_pin_get_raw(input_sw0_gpio.port, input_sw0_gpio.pin);
			if (raw < 0) {
				return raw;
			}
			*pressed = active_low ? raw == 0 : raw != 0;
			return 0;
		}
		*pressed = false;
		return 0;
	}
#endif
#if SQ_VM_RUNTIME_HAS_GPIO0
	if (device_is_ready(gpio0_dev)) {
		int raw = gpio_pin_get_raw(gpio0_dev, pin);
		if (raw < 0) {
			return raw;
		}
		*pressed = active_low ? raw == 0 : raw != 0;
		return 0;
	}
#endif
	*pressed = false;
	return 0;
}

int sq_vm_runtime_hardware_gpio_write(struct sq_vm_runtime *runtime, const uint8_t *name,
				      size_t name_len, bool value)
{
	uint8_t pin;
	uint32_t bit;
	int result;

	if (runtime == NULL || parse_gpio_name(name, name_len, &pin) != 0) {
		return -EINVAL;
	}
	if (runtime_indicator_uses_pin(runtime, pin)) {
		runtime->indicator_pattern = SQ_VM_RUNTIME_INDICATOR_STEADY;
		runtime->indicator_state =
			runtime_indicator_active_low(runtime) ? !value : value;
		bit = BIT(pin);
		runtime->gpio_configured_mask |= bit;
		if (value) {
			runtime->gpio_state_mask |= bit;
		} else {
			runtime->gpio_state_mask &= ~bit;
		}
		return set_indicator_raw_output(runtime, value);
	}
	result = configure_raw_gpio(runtime, pin);
	if (result != 0) {
		return result;
	}
	bit = BIT(pin);
	if (value) {
		runtime->gpio_state_mask |= bit;
	} else {
		runtime->gpio_state_mask &= ~bit;
	}
#if SQ_VM_RUNTIME_HAS_GPIO0
	if (device_is_ready(gpio0_dev)) {
		return gpio_pin_set_raw(gpio0_dev, pin, value ? 1 : 0);
	}
#endif
	return 0;
}

int sq_vm_runtime_hardware_gpio_toggle(struct sq_vm_runtime *runtime, const uint8_t *name,
				       size_t name_len)
{
	bool value;
	int result = sq_vm_runtime_hardware_gpio_read(runtime, name, name_len, &value);

	if (result != 0) {
		return result;
	}
	return sq_vm_runtime_hardware_gpio_write(runtime, name, name_len, !value);
}

int sq_vm_runtime_hardware_gpio_read(struct sq_vm_runtime *runtime, const uint8_t *name,
				     size_t name_len, bool *out)
{
	uint8_t pin;
	uint32_t bit;

	if (runtime == NULL || out == NULL || parse_gpio_name(name, name_len, &pin) != 0) {
		return -EINVAL;
	}
	bit = BIT(pin);
	if ((runtime->gpio_configured_mask & bit) != 0) {
		*out = (runtime->gpio_state_mask & bit) != 0;
		return 0;
	}
#if SQ_VM_RUNTIME_HAS_GPIO0
	if (device_is_ready(gpio0_dev)) {
		int value = gpio_pin_get_raw(gpio0_dev, pin);
		if (value < 0) {
			return value;
		}
		*out = value != 0;
		return 0;
	}
#endif
	*out = (runtime->gpio_state_mask & bit) != 0;
	return 0;
}

int sq_vm_runtime_poll_indicator(struct sq_vm_runtime *runtime)
{
	int64_t now;
	uint8_t brightness;

	switch (runtime->indicator_pattern) {
	case SQ_VM_RUNTIME_INDICATOR_STEADY:
		return 0;
	case SQ_VM_RUNTIME_INDICATOR_BREATHE:
		now = k_uptime_get();
		if (now < runtime->indicator_pattern_next_ms) {
			return 0;
		}
		brightness = indicator_breathe_duties[runtime->indicator_pattern_step];
		runtime->indicator_pattern_step =
			(uint8_t)((runtime->indicator_pattern_step + 1U) %
			  SQ_VM_RUNTIME_INDICATOR_BREATHE_STEPS);
		runtime->indicator_pattern_next_ms = now + SQ_VM_RUNTIME_BREATHE_LEVEL_MS;
		return set_indicator_brightness(runtime, brightness);
	case SQ_VM_RUNTIME_INDICATOR_BLINK:
		now = k_uptime_get();
		if (now < runtime->indicator_pattern_next_ms) {
			return 0;
		}
		runtime->indicator_pattern_on = !runtime->indicator_pattern_on;
		runtime->indicator_pattern_next_ms =
			now + (runtime->indicator_pattern_on ? runtime->indicator_pattern_on_ms :
							runtime->indicator_pattern_off_ms);
		return set_indicator_brightness(runtime, runtime->indicator_pattern_on ? 100U : 0U);
	default:
		runtime->indicator_pattern = SQ_VM_RUNTIME_INDICATOR_STEADY;
		return 0;
	}
}

int sq_vm_runtime_poll_input_buttons(struct sq_vm_runtime *runtime)
{
	int64_t now;

	if (runtime == NULL || runtime->input_button_count == 0 ||
	    runtime->job_backend.read_sqbc == NULL) {
		return 0;
	}
	now = k_uptime_get();
	size_t active_max = runtime->active_input_button_max == 0 ? SQ_VM_RUNTIME_INPUT_BUTTON_MAX :
								    runtime->active_input_button_max;
	for (size_t i = 0; i < active_max; i++) {
		struct sq_vm_runtime_input_button *button = &runtime->input_buttons[i];

		if (!button->active) {
			continue;
		}
		if (now < button->next_poll_ms) {
			continue;
		}
		button->next_poll_ms = now + SQ_VM_RUNTIME_INPUT_POLL_MS;
		bool pressed = false;
		int result = read_input_button_gpio(button->pin, button->active_low, &pressed);
		if (result != 0) {
			return result;
		}
		if (pressed == button->pressed) {
			if (pressed) {
				button->phase = SQ_VM_RUNTIME_INPUT_PRESSED;
			} else {
				button->phase = SQ_VM_RUNTIME_INPUT_RELEASED;
			}
			continue;
		}
		if (now < button->debounce_until_ms) {
			if (pressed) {
				button->phase = SQ_VM_RUNTIME_INPUT_DEBOUNCING_PRESS;
			} else {
				button->phase = SQ_VM_RUNTIME_INPUT_DEBOUNCING_RELEASE;
			}
			continue;
		}
		button->pressed = pressed;
		button->debounce_until_ms = now + SQ_VM_RUNTIME_INPUT_DEBOUNCE_MS;
		if (pressed) {
			button->phase = SQ_VM_RUNTIME_INPUT_PRESSED;
			if (runtime->status == SQ_VM_RUNTIME_RUNNING) {
				int queued = sq_vm_runtime_queue_input_event(runtime, button->event);

				return queued == -ENOSPC ? 0 : queued;
			}
#if IS_ENABLED(CONFIG_SQUIDSCRIPT_ZEPHYR_DIAGNOSTIC)
			sq_debug_log_append("%lld:btn:%s", (long long)k_uptime_get(), button->event);
#endif
			return sq_vm_runtime_start(runtime, &runtime->job_backend, button->event);
		}
		button->phase = SQ_VM_RUNTIME_INPUT_RELEASED;
	}
	return 0;
}
