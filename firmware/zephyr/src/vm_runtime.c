#include "vm_runtime.h"

#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <stddef.h>

#include <zephyr/devicetree.h>
#include <zephyr/drivers/gpio.h>
#include <zephyr/drivers/pwm.h>

#define SQ_VM_RUNTIME_BREATHE_LEVEL_MS 31
#define SQ_SET_LITERAL_FIELD(target, field, value) \
	do { \
		(target)->field = (const uint8_t *)(value); \
		(target)->field##_len = sizeof(value) - 1; \
	} while (false)

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
		memmove(runtime->traces[0], runtime->traces[1],
			(SQ_VM_RUNTIME_TRACE_MAX - 1) * SQ_VM_RUNTIME_TRACE_LEN);
		runtime->trace_count = SQ_VM_RUNTIME_TRACE_MAX - 1;
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

static void runtime_display_clear(void *user_data, const uint8_t *color, size_t color_len)
{
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];
	int written = snprintf(line, sizeof(line), "draw=clear color=%.*s", (int)color_len,
			       color == NULL ? (const uint8_t *)"" : color);

	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(user_data, line);
	}
}

static void runtime_display_text(void *user_data, const uint8_t *text, size_t text_len,
				 const SqvmDisplayTextOptions *options)
{
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];

	if (options == NULL) {
		return;
	}
	int written = snprintf(line, sizeof(line), "draw=text text=\"%.*s\" x=%d y=%d",
			       (int)text_len, text == NULL ? (const uint8_t *)"" : text,
			       options->x, options->y);
	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(user_data, line);
	}
}

static void runtime_display_rect(void *user_data, const SqvmDisplayRectOptions *options)
{
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];

	if (options == NULL) {
		return;
	}
	int written = snprintf(line, sizeof(line), "draw=rect x=%d y=%d w=%d h=%d", options->x,
			       options->y, options->w, options->h);
	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(user_data, line);
	}
}

static void runtime_display_line(void *user_data, const SqvmDisplayLineOptions *options)
{
	char line[SQ_VM_RUNTIME_DRAWLOG_LEN];

	if (options == NULL) {
		return;
	}
	int written = snprintf(line, sizeof(line), "draw=line x1=%d y1=%d x2=%d y2=%d",
			       options->x1, options->y1, options->x2, options->y2);
	if (written > 0) {
		(void)sq_vm_runtime_record_drawlog(user_data, line);
	}
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

static int32_t runtime_app_lifecycle(void *user_data, const char *action, const uint8_t *app,
				     size_t app_len)
{
	struct sq_vm_runtime *runtime = user_data;
	char line[SQ_VM_RUNTIME_TRACE_LEN];

	if (runtime == NULL || action == NULL || (app == NULL && app_len > 0)) {
		return -EINVAL;
	}
	int written = snprintf(line, sizeof(line), "app.%s %.*s", action, (int)app_len,
			       app == NULL ? (const uint8_t *)"" : app);
	if (written > 0) {
		runtime_trace(runtime, (const uint8_t *)line, strlen(line));
	}
	return 0;
}

static int32_t runtime_app_launch(void *user_data, const uint8_t *app, size_t app_len)
{
	struct sq_vm_runtime *runtime = user_data;
	int result = runtime_app_lifecycle(user_data, "launch", app, app_len);

	if (result != 0) {
		return result;
	}
	if (runtime == NULL || app == NULL || app_len == 0 ||
	    app_len >= sizeof(runtime->pending_launch_app)) {
		return -EINVAL;
	}
	memcpy(runtime->pending_launch_app, app, app_len);
	runtime->pending_launch_app[app_len] = '\0';
	runtime->pending_launch_active = true;
	return 0;
}

static int32_t runtime_app_arm(void *user_data, const uint8_t *app, size_t app_len)
{
	struct sq_vm_runtime *runtime = user_data;
	int result = runtime_app_lifecycle(user_data, "arm", app, app_len);

	if (result != 0) {
		return result;
	}
	if (runtime == NULL || app == NULL || app_len == 0 ||
	    app_len >= sizeof(runtime->pending_arm_app)) {
		return -EINVAL;
	}
	memcpy(runtime->pending_arm_app, app, app_len);
	runtime->pending_arm_app[app_len] = '\0';
	runtime->pending_arm_active = true;
	return 0;
}

static int32_t runtime_app_disarm(void *user_data, const uint8_t *app, size_t app_len)
{
	int result = runtime_app_lifecycle(user_data, "disarm", app, app_len);

	if (result != 0) {
		return result;
	}
	struct sq_vm_runtime *runtime = user_data;
	if (runtime != NULL && app != NULL && strlen(runtime->pending_arm_app) == app_len &&
	    memcmp(runtime->pending_arm_app, app, app_len) == 0) {
		memset(runtime->pending_arm_app, 0, sizeof(runtime->pending_arm_app));
		runtime->pending_arm_active = false;
	}
	return sq_vm_runtime_clear_armed_app(user_data, app, app_len);
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

static int32_t runtime_wifi_status(void *user_data, SqvmWifiStatus *out)
{
	ARG_UNUSED(user_data);

	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
	out->active = false;
	SQ_SET_LITERAL_FIELD(out, state, "stopped");
	SQ_SET_LITERAL_FIELD(out, backend, "zephyr");
	out->driver_started = false;
	out->configured = false;
	SQ_SET_LITERAL_FIELD(out, error, "unsupported");
	return 0;
}

static int32_t runtime_wifi_scan(void *user_data, SqvmWifiScanResult *out)
{
	ARG_UNUSED(user_data);

	if (out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
	out->ok = false;
	SQ_SET_LITERAL_FIELD(out, error, "unsupported");
	out->networks = NULL;
	out->network_count = 0;
	return 0;
}

static void clear_dispatch_state(struct sq_vm_runtime *runtime)
{
	memset(runtime->context_words, 0, sizeof(runtime->context_words));
	memset(runtime->scratch, 0, sizeof(runtime->scratch));
	memset(&runtime->result, 0, sizeof(runtime->result));
	memset(&runtime->completion, 0, sizeof(runtime->completion));
	runtime->backend = NULL;
}

static void runtime_work_handler(struct k_work *work)
{
	struct sq_vm_runtime *runtime = CONTAINER_OF(work, struct sq_vm_runtime, work);
	int result = sq_vm_runtime_dispatch(runtime, &runtime->job_backend, runtime->event);

	runtime->result_code = result;
	runtime->dispatch_exited = result == 0 && runtime->result.exited;
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

size_t sq_vm_runtime_work_stack_size(void)
{
	return K_THREAD_STACK_SIZEOF(sq_vm_runtime_work_stack);
}

int sq_vm_runtime_work_stack_unused(size_t *unused)
{
	if (unused == NULL) {
		return -EINVAL;
	}

#if defined(CONFIG_INIT_STACKS) && defined(CONFIG_THREAD_STACK_INFO)
	if (!sq_vm_runtime_work_q_started) {
		*unused = sq_vm_runtime_work_stack_size();
		return 0;
	}

	return k_thread_stack_space_get(k_work_queue_thread_get(&sq_vm_runtime_work_q), unused);
#else
	*unused = 0;
	return -ENOTSUP;
#endif
}

void sq_vm_runtime_reset(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL) {
		return;
	}
	clear_dispatch_state(runtime);
	memset(&runtime->job_backend, 0, sizeof(runtime->job_backend));
	memset(runtime->event, 0, sizeof(runtime->event));
	memset(runtime->traces, 0, sizeof(runtime->traces));
	runtime->trace_count = 0;
	memset(runtime->current_app, 0, sizeof(runtime->current_app));
	memset(runtime->pending_launch_app, 0, sizeof(runtime->pending_launch_app));
	runtime->pending_launch_active = false;
	memset(runtime->pending_arm_app, 0, sizeof(runtime->pending_arm_app));
	runtime->pending_arm_active = false;
	runtime->arm_registration_active = false;
	memset(runtime->arm_registration_app, 0, sizeof(runtime->arm_registration_app));
	memset(runtime->lifecycle_target_app, 0, sizeof(runtime->lifecycle_target_app));
	runtime->lifecycle_launch_after_exit = false;
	memset(runtime->return_stack, 0, sizeof(runtime->return_stack));
	runtime->return_stack_count = 0;
	memset(runtime->armed_timers, 0, sizeof(runtime->armed_timers));
	runtime->armed_timer_count = 0;
	memset(runtime->outputs, 0, sizeof(runtime->outputs));
	runtime->output_count = 0;
	memset(runtime->drawlog, 0, sizeof(runtime->drawlog));
	runtime->drawlog_count = 0;
	memset(runtime->timers, 0, sizeof(runtime->timers));
	runtime->indicator_state = false;
	runtime->indicator_breathe_active = false;
	runtime->indicator_breathe_step = 0;
	runtime->indicator_breathe_next_ms = 0;
	runtime->gpio_configured_mask = 0;
	runtime->gpio_state_mask = 0;
	runtime->dispatch_exited = false;
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
		.display_clear = runtime_display_clear,
		.display_text = runtime_display_text,
		.display_rect = runtime_display_rect,
		.display_line = runtime_display_line,
		.indicator_write = runtime_indicator_write,
		.indicator_toggle = runtime_indicator_toggle,
		.indicator_read = runtime_indicator_read,
		.indicator_breathe = runtime_indicator_breathe,
		.hardware_gpio_write = runtime_hardware_gpio_write,
		.hardware_gpio_toggle = runtime_hardware_gpio_toggle,
		.hardware_gpio_read = runtime_hardware_gpio_read,
		.app_launch = runtime_app_launch,
		.app_arm = runtime_app_arm,
		.app_disarm = runtime_app_disarm,
		.timer_every = runtime_timer_every,
		.timer_after = runtime_timer_after,
		.wifi_status = runtime_wifi_status,
		.wifi_scan = runtime_wifi_scan,
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

	runtime->dispatch_exited = runtime->result.outcome == SQVM_DISPATCH_COMPLETE &&
				   runtime->result.exited;
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
	runtime->dispatch_exited = false;
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

int sq_vm_runtime_record_drawlog(struct sq_vm_runtime *runtime, const char *line)
{
	if (runtime == NULL || line == NULL) {
		return -EINVAL;
	}
	size_t slot = runtime->drawlog_count;
	if (slot >= SQ_VM_RUNTIME_DRAWLOG_MAX) {
		memmove(runtime->drawlog[0], runtime->drawlog[1],
			(SQ_VM_RUNTIME_DRAWLOG_MAX - 1) * SQ_VM_RUNTIME_DRAWLOG_LEN);
		slot = SQ_VM_RUNTIME_DRAWLOG_MAX - 1;
		runtime->drawlog_count = SQ_VM_RUNTIME_DRAWLOG_MAX - 1;
	}
	size_t len = 0;
	while (len < SQ_VM_RUNTIME_DRAWLOG_LEN - 1 && line[len] != '\0') {
		len++;
	}
	memcpy(runtime->drawlog[slot], line, len);
	runtime->drawlog[slot][len] = '\0';
	runtime->drawlog_count++;
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
	if (runtime->arm_registration_active) {
		for (size_t i = 0; i < SQ_VM_RUNTIME_ARMED_TIMER_MAX; i++) {
			struct sq_vm_runtime_armed_timer *timer = &runtime->armed_timers[i];
			if (timer->active &&
			    strcmp(timer->app_id, runtime->arm_registration_app) == 0 &&
			    strncmp(timer->event, (const char *)event, event_len) == 0 &&
			    timer->event[event_len] == '\0') {
				timer->repeating = repeating;
				timer->interval_ms = interval_ms;
				timer->due_ms = k_uptime_get() + interval_ms;
				return 0;
			}
		}
		for (size_t i = 0; i < SQ_VM_RUNTIME_ARMED_TIMER_MAX; i++) {
			struct sq_vm_runtime_armed_timer *timer = &runtime->armed_timers[i];
			if (!timer->active) {
				timer->active = true;
				timer->repeating = repeating;
				timer->interval_ms = interval_ms;
				timer->due_ms = k_uptime_get() + interval_ms;
				strncpy(timer->app_id, runtime->arm_registration_app,
					sizeof(timer->app_id) - 1);
				timer->app_id[sizeof(timer->app_id) - 1] = '\0';
				memcpy(timer->event, event, event_len);
				timer->event[event_len] = '\0';
				runtime->armed_timer_count++;
				return 0;
			}
		}
		return -ENOSPC;
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

int sq_vm_runtime_clear_armed_app(struct sq_vm_runtime *runtime, const uint8_t *app,
				  size_t app_len)
{
	if (runtime == NULL || app == NULL || app_len == 0 ||
	    app_len >= SQ_APP_STORE_APP_ID_MAX) {
		return -EINVAL;
	}
	for (size_t i = 0; i < SQ_VM_RUNTIME_ARMED_TIMER_MAX; i++) {
		struct sq_vm_runtime_armed_timer *timer = &runtime->armed_timers[i];
		if (timer->active && strlen(timer->app_id) == app_len &&
		    memcmp(timer->app_id, app, app_len) == 0) {
			memset(timer, 0, sizeof(*timer));
		}
	}
	runtime->armed_timer_count = 0;
	for (size_t i = 0; i < SQ_VM_RUNTIME_ARMED_TIMER_MAX; i++) {
		if (runtime->armed_timers[i].active) {
			runtime->armed_timer_count++;
		}
	}
	return 0;
}

int sq_vm_runtime_next_due_armed_timer(struct sq_vm_runtime *runtime, char *app, size_t app_cap,
				       char *event, size_t event_cap)
{
	if (runtime == NULL || app == NULL || app_cap == 0 || event == NULL || event_cap == 0) {
		return -EINVAL;
	}
	int64_t now = k_uptime_get();
	for (size_t i = 0; i < SQ_VM_RUNTIME_ARMED_TIMER_MAX; i++) {
		struct sq_vm_runtime_armed_timer *timer = &runtime->armed_timers[i];
		if (!timer->active || timer->due_ms > now) {
			continue;
		}
		size_t app_len = strlen(timer->app_id);
		size_t event_len = strlen(timer->event);
		if (app_len == 0 || app_len >= app_cap || event_len == 0 || event_len >= event_cap) {
			return -ENOSPC;
		}
		memcpy(app, timer->app_id, app_len + 1);
		memcpy(event, timer->event, event_len + 1);
		if (timer->repeating) {
			timer->due_ms = now + timer->interval_ms;
		} else {
			memset(timer, 0, sizeof(*timer));
			runtime->armed_timer_count--;
		}
		return 0;
	}
	return -ENOENT;
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
