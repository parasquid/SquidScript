#include "vm_runtime.h"

#include <errno.h>
#include <string.h>
#include <stddef.h>

#include <zephyr/devicetree.h>
#include <zephyr/drivers/gpio.h>
#include <zephyr/drivers/pwm.h>

#define SQ_VM_RUNTIME_WORK_STACK_SIZE 16384
#define SQ_VM_RUNTIME_BREATHE_LEVEL_MS 31

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

K_THREAD_STACK_DEFINE(sq_vm_runtime_work_stack, SQ_VM_RUNTIME_WORK_STACK_SIZE);
static struct k_work_q sq_vm_runtime_work_q;
static bool sq_vm_runtime_work_q_started;

static void runtime_trace(void *user_data, const uint8_t *message, size_t message_len)
{
	struct sq_vm_runtime *runtime = user_data;

	if (runtime->trace_count >= SQ_VM_RUNTIME_TRACE_MAX) {
		return;
	}

	size_t len = message_len;
	if (len >= SQ_VM_RUNTIME_TRACE_LEN) {
		len = SQ_VM_RUNTIME_TRACE_LEN - 1;
	}
	memcpy(runtime->traces[runtime->trace_count], message, len);
	runtime->traces[runtime->trace_count][len] = '\0';
	runtime->trace_count++;
}

static int32_t runtime_read_exact_at(void *user_data, size_t offset, uint8_t *out, size_t out_len)
{
	struct sq_vm_runtime *runtime = user_data;

	if (runtime->backend == NULL || runtime->backend->read_sqbc == NULL) {
		return -EINVAL;
	}
	return runtime->backend->read_sqbc(runtime->backend->user_data, offset, out, out_len);
}

static void runtime_debug_output(void *user_data, const uint8_t *message, size_t message_len)
{
	(void)sq_vm_runtime_record_output(user_data, message, message_len);
}

static int32_t runtime_indicator_write(void *user_data, bool value)
{
	return sq_vm_runtime_indicator_write(user_data, value);
}

static int32_t runtime_indicator_toggle(void *user_data)
{
	return sq_vm_runtime_indicator_toggle(user_data);
}

static int32_t runtime_indicator_read(void *user_data, bool *out)
{
	return sq_vm_runtime_indicator_read(user_data, out);
}

static int32_t runtime_indicator_breathe(void *user_data)
{
	return sq_vm_runtime_indicator_breathe(user_data);
}

static int32_t runtime_hardware_gpio_write(void *user_data, const uint8_t *name, size_t name_len,
					   bool value)
{
	return sq_vm_runtime_hardware_gpio_write(user_data, name, name_len, value);
}

static int32_t runtime_hardware_gpio_toggle(void *user_data, const uint8_t *name, size_t name_len)
{
	return sq_vm_runtime_hardware_gpio_toggle(user_data, name, name_len);
}

static int32_t runtime_hardware_gpio_read(void *user_data, const uint8_t *name, size_t name_len,
					  bool *out)
{
	return sq_vm_runtime_hardware_gpio_read(user_data, name, name_len, out);
}

static int32_t runtime_timer_every(void *user_data, const uint8_t *event, size_t event_len,
				   int32_t interval_ms)
{
	return sq_vm_runtime_register_timer(user_data, event, event_len, interval_ms, true);
}

static int32_t runtime_timer_after(void *user_data, const uint8_t *event, size_t event_len,
				  int32_t delay_ms)
{
	return sq_vm_runtime_register_timer(user_data, event, event_len, delay_ms, false);
}

static void clear_dispatch_state(struct sq_vm_runtime *runtime)
{
	memset(runtime->context_words, 0, sizeof(runtime->context_words));
	memset(runtime->scratch, 0, sizeof(runtime->scratch));
	memset(&runtime->result, 0, sizeof(runtime->result));
	memset(&runtime->completion, 0, sizeof(runtime->completion));
	memset(runtime->traces, 0, sizeof(runtime->traces));
	runtime->trace_count = 0;
	runtime->backend = NULL;
}

static void runtime_work_handler(struct k_work *work)
{
	struct sq_vm_runtime *runtime = CONTAINER_OF(work, struct sq_vm_runtime, work);
	int result = sq_vm_runtime_dispatch(runtime, &runtime->job_backend, runtime->event);

	runtime->result_code = result;
	runtime->status = result == 0 ? SQ_VM_RUNTIME_COMPLETE : SQ_VM_RUNTIME_ERROR;
}

void sq_vm_runtime_init(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL || runtime->work_initialized) {
		return;
	}
	if (!sq_vm_runtime_work_q_started) {
		k_work_queue_start(&sq_vm_runtime_work_q, sq_vm_runtime_work_stack,
				   K_THREAD_STACK_SIZEOF(sq_vm_runtime_work_stack), 5, NULL);
		sq_vm_runtime_work_q_started = true;
	}
	k_work_init(&runtime->work, runtime_work_handler);
	runtime->work_initialized = true;
	runtime->status = SQ_VM_RUNTIME_IDLE;
}

void sq_vm_runtime_reset(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL) {
		return;
	}
	clear_dispatch_state(runtime);
	memset(&runtime->job_backend, 0, sizeof(runtime->job_backend));
	memset(runtime->event, 0, sizeof(runtime->event));
	memset(runtime->outputs, 0, sizeof(runtime->outputs));
	runtime->output_count = 0;
	memset(runtime->timers, 0, sizeof(runtime->timers));
	runtime->indicator_state = false;
	runtime->indicator_breathe_active = false;
	runtime->indicator_breathe_step = 0;
	runtime->indicator_breathe_next_ms = 0;
	runtime->gpio_configured_mask = 0;
	runtime->gpio_state_mask = 0;
	runtime->result_code = 0;
	runtime->status = SQ_VM_RUNTIME_IDLE;
}

int sq_vm_runtime_dispatch(struct sq_vm_runtime *runtime,
			   const struct sq_vm_storage_backend *backend, const char *event)
{
	SqvmCallbacks callbacks;
	SqvmStatus status;

	if (runtime == NULL || backend == NULL || event == NULL) {
		return -EINVAL;
	}
	if (sqvm_context_size() > sizeof(runtime->context_words)) {
		return -ENOMEM;
	}

	clear_dispatch_state(runtime);
	runtime->backend = backend;
	callbacks = (SqvmCallbacks){
		.user_data = runtime,
		.trace = runtime_trace,
		.read_exact_at = runtime_read_exact_at,
		.debug_output = runtime_debug_output,
		.indicator_write = runtime_indicator_write,
		.indicator_toggle = runtime_indicator_toggle,
		.indicator_read = runtime_indicator_read,
		.indicator_breathe = runtime_indicator_breathe,
		.hardware_gpio_write = runtime_hardware_gpio_write,
		.hardware_gpio_toggle = runtime_hardware_gpio_toggle,
		.hardware_gpio_read = runtime_hardware_gpio_read,
		.timer_every = runtime_timer_every,
		.timer_after = runtime_timer_after,
	};

	status = sqvm_context_prepare(runtime->context_words, sizeof(runtime->context_words));
	if (status != SQVM_STATUS_OK) {
		return -EIO;
	}
	status = sqvm_context_init_in_place(runtime->context_words, callbacks, runtime->scratch,
					    sizeof(runtime->scratch));
	if (status != SQVM_STATUS_OK) {
		return -EIO;
	}
	status = sqvm_dispatch_start_resumable(runtime->context_words, callbacks,
					       (const uint8_t *)event, strlen(event),
					       &runtime->result);
	if (status != SQVM_STATUS_OK) {
		return -EIO;
	}

	while (runtime->result.outcome == SQVM_DISPATCH_PENDING_STORAGE) {
		int storage_result = sq_vm_storage_complete_request(backend, &runtime->result.storage,
								   &runtime->completion);
		if (storage_result != 0) {
			return storage_result;
		}
		status = sqvm_dispatch_resume_storage(runtime->context_words, callbacks,
						      &runtime->completion, &runtime->result);
		if (status != SQVM_STATUS_OK) {
			return -EIO;
		}
	}

	return runtime->result.outcome == SQVM_DISPATCH_COMPLETE ? 0 : -EIO;
}

int sq_vm_runtime_start(struct sq_vm_runtime *runtime,
			const struct sq_vm_storage_backend *backend, const char *event)
{
	size_t event_len;

	if (runtime == NULL || backend == NULL || event == NULL) {
		return -EINVAL;
	}
	sq_vm_runtime_init(runtime);
	if (runtime->status == SQ_VM_RUNTIME_RUNNING) {
		return -EBUSY;
	}
	event_len = strlen(event);
	if (event_len == 0 || event_len >= sizeof(runtime->event)) {
		return -EINVAL;
	}

	runtime->job_backend = *backend;
	memcpy(runtime->event, event, event_len + 1);
	runtime->result_code = 0;
	runtime->status = SQ_VM_RUNTIME_RUNNING;
	k_work_submit_to_queue(&sq_vm_runtime_work_q, &runtime->work);
	return 0;
}

int sq_vm_runtime_record_output(struct sq_vm_runtime *runtime, const uint8_t *message,
				size_t message_len)
{
	if (runtime == NULL || (message == NULL && message_len > 0)) {
		return -EINVAL;
	}
	size_t slot = runtime->output_count;
	if (slot >= SQ_VM_RUNTIME_OUTPUT_MAX) {
		memmove(runtime->outputs[0], runtime->outputs[1],
			(SQ_VM_RUNTIME_OUTPUT_MAX - 1) * SQ_VM_RUNTIME_OUTPUT_LEN);
		slot = SQ_VM_RUNTIME_OUTPUT_MAX - 1;
		runtime->output_count = SQ_VM_RUNTIME_OUTPUT_MAX - 1;
	}
	size_t len = message_len;
	if (len >= SQ_VM_RUNTIME_OUTPUT_LEN) {
		len = SQ_VM_RUNTIME_OUTPUT_LEN - 1;
	}
	memcpy(runtime->outputs[slot], message, len);
	runtime->outputs[slot][len] = '\0';
	runtime->output_count++;
	return 0;
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

static bool indicator_is_active_low(void)
{
#if SQ_VM_RUNTIME_HAS_INDICATOR_GPIO
	return (indicator_gpio.dt_flags & GPIO_ACTIVE_LOW) != 0;
#else
	return false;
#endif
}

static bool indicator_uses_raw_gpio(uint8_t pin)
{
#if SQ_VM_RUNTIME_HAS_INDICATOR_GPIO
	return indicator_gpio.pin == pin;
#else
	return false;
#endif
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
#if SQ_VM_RUNTIME_HAS_INDICATOR_PWM
	uint8_t raw_high_percent = indicator_is_active_low() ? (uint8_t)(100U - clamped) : clamped;
#endif

	runtime->indicator_state = clamped > 0U;
#if SQ_VM_RUNTIME_HAS_INDICATOR_PWM
	if (pwm_is_ready_dt(&indicator_pwm)) {
		uint32_t pulse = (indicator_pwm.period * (uint32_t)raw_high_percent) / 100U;
		return pwm_set_dt(&indicator_pwm, indicator_pwm.period, pulse);
	}
#endif
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
	runtime->indicator_breathe_active = false;
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
	runtime->indicator_breathe_active = true;
	runtime->indicator_breathe_step = 0;
	runtime->indicator_breathe_next_ms = now;
	return set_indicator_brightness(runtime, 0U);
}

static int parse_gpio_name(const uint8_t *name, size_t name_len, uint8_t *pin)
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

int sq_vm_runtime_hardware_gpio_write(struct sq_vm_runtime *runtime, const uint8_t *name,
				      size_t name_len, bool value)
{
	uint8_t pin;
	uint32_t bit;
	int result;

	if (runtime == NULL || parse_gpio_name(name, name_len, &pin) != 0) {
		return -EINVAL;
	}
	if (indicator_uses_raw_gpio(pin)) {
		runtime->indicator_breathe_active = false;
		runtime->indicator_state = indicator_is_active_low() ? !value : value;
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

static int sq_vm_runtime_poll_indicator_breathe(struct sq_vm_runtime *runtime)
{
	int64_t now;
	uint8_t brightness;

	if (!runtime->indicator_breathe_active) {
		return 0;
	}
	now = k_uptime_get();
	if (now < runtime->indicator_breathe_next_ms) {
		return 0;
	}

	brightness = indicator_breathe_duties[runtime->indicator_breathe_step];
	runtime->indicator_breathe_step =
		(uint8_t)((runtime->indicator_breathe_step + 1U) %
			  SQ_VM_RUNTIME_INDICATOR_BREATHE_STEPS);
	runtime->indicator_breathe_next_ms = now + SQ_VM_RUNTIME_BREATHE_LEVEL_MS;
	return set_indicator_brightness(runtime, brightness);
}

int sq_vm_runtime_register_timer(struct sq_vm_runtime *runtime, const uint8_t *event,
				 size_t event_len, int32_t interval_ms, bool repeating)
{
	if (runtime == NULL || event == NULL || event_len == 0 ||
	    event_len >= SQ_VM_RUNTIME_EVENT_LEN || interval_ms <= 0) {
		return -EINVAL;
	}
	for (size_t i = 0; i < SQ_VM_RUNTIME_TIMER_MAX; i++) {
		if (runtime->timers[i].active &&
		    strncmp(runtime->timers[i].event, (const char *)event, event_len) == 0 &&
		    runtime->timers[i].event[event_len] == '\0') {
			runtime->timers[i].repeating = repeating;
			runtime->timers[i].interval_ms = interval_ms;
			runtime->timers[i].due_ms = k_uptime_get() + interval_ms;
			return 0;
		}
	}
	for (size_t i = 0; i < SQ_VM_RUNTIME_TIMER_MAX; i++) {
		if (!runtime->timers[i].active) {
			runtime->timers[i].active = true;
			runtime->timers[i].repeating = repeating;
			runtime->timers[i].interval_ms = interval_ms;
			runtime->timers[i].due_ms = k_uptime_get() + interval_ms;
			memcpy(runtime->timers[i].event, event, event_len);
			runtime->timers[i].event[event_len] = '\0';
			return 0;
		}
	}
	return -ENOSPC;
}

int sq_vm_runtime_next_due_timer(struct sq_vm_runtime *runtime, char *event, size_t event_cap)
{
	if (runtime == NULL || event == NULL || event_cap == 0) {
		return -EINVAL;
	}
	int64_t now = k_uptime_get();
	for (size_t i = 0; i < SQ_VM_RUNTIME_TIMER_MAX; i++) {
		struct sq_vm_runtime_timer *timer = &runtime->timers[i];
		if (!timer->active || timer->due_ms > now) {
			continue;
		}
		size_t event_len = strlen(timer->event);
		if (event_len == 0 || event_len >= event_cap) {
			return -ENOSPC;
		}
		memcpy(event, timer->event, event_len + 1);
		if (timer->repeating) {
			timer->due_ms = now + timer->interval_ms;
		} else {
			memset(timer, 0, sizeof(*timer));
		}
		return 0;
	}
	return -ENOENT;
}

int sq_vm_runtime_poll(struct sq_vm_runtime *runtime)
{
	char event[SQ_VM_RUNTIME_EVENT_LEN];

	if (runtime == NULL) {
		return 0;
	}
	(void)sq_vm_runtime_poll_indicator_breathe(runtime);
	if (runtime->status == SQ_VM_RUNTIME_RUNNING || runtime->job_backend.read_sqbc == NULL) {
		return 0;
	}
	if (sq_vm_runtime_next_due_timer(runtime, event, sizeof(event)) != 0) {
		return 0;
	}
	return sq_vm_runtime_start(runtime, &runtime->job_backend, event);
}
