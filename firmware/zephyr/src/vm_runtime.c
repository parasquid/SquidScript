#include "vm_runtime.h"

#include <errno.h>
#include <string.h>
#include <stddef.h>

#include <zephyr/devicetree.h>
#include <zephyr/drivers/gpio.h>

#define SQ_VM_RUNTIME_WORK_STACK_SIZE 16384
#define SQ_VM_RUNTIME_BREATHE_LEVEL_MS 40
#define SQ_VM_RUNTIME_BREATHE_PWM_MS 16

#if IS_ENABLED(CONFIG_GPIO) && DT_NODE_HAS_STATUS(DT_ALIAS(led0), okay)
static const struct gpio_dt_spec indicator_led = GPIO_DT_SPEC_GET(DT_ALIAS(led0), gpios);
#define SQ_VM_RUNTIME_HAS_INDICATOR_LED 1
#else
#define SQ_VM_RUNTIME_HAS_INDICATOR_LED 0
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
	runtime->indicator_breathe_rising = false;
	runtime->indicator_breathe_phase = 0;
	runtime->indicator_breathe_next_ms = 0;
	runtime->indicator_breathe_frame_ms = 0;
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
#if SQ_VM_RUNTIME_HAS_INDICATOR_LED
	if (!gpio_is_ready_dt(&indicator_led)) {
		return 0;
	}
	if (gpio_pin_configure_dt(&indicator_led, GPIO_OUTPUT_INACTIVE) != 0) {
		return 0;
	}
	runtime->indicator_gpio_available = true;
#endif
	return 0;
}

static int set_indicator_output(struct sq_vm_runtime *runtime, bool value)
{
	runtime->indicator_state = value;
	(void)configure_indicator_gpio(runtime);
#if SQ_VM_RUNTIME_HAS_INDICATOR_LED
	if (runtime->indicator_gpio_available) {
		int result = gpio_pin_set_dt(&indicator_led, value ? 1 : 0);
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
	return set_indicator_output(runtime, value);
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
	runtime->indicator_breathe_rising = true;
	runtime->indicator_breathe_phase = 0;
	runtime->indicator_breathe_next_ms = now + SQ_VM_RUNTIME_BREATHE_LEVEL_MS;
	runtime->indicator_breathe_frame_ms = now;
	return set_indicator_output(runtime, false);
}

static int sq_vm_runtime_poll_indicator_breathe(struct sq_vm_runtime *runtime)
{
	int64_t now;
	int64_t frame_delta;
	uint8_t on_ms;
	bool on;

	if (!runtime->indicator_breathe_active) {
		return 0;
	}
	now = k_uptime_get();
	if (now >= runtime->indicator_breathe_next_ms) {
		runtime->indicator_breathe_next_ms = now + SQ_VM_RUNTIME_BREATHE_LEVEL_MS;
		if (runtime->indicator_breathe_rising) {
			if (runtime->indicator_breathe_phase >= SQ_VM_RUNTIME_INDICATOR_BREATHE_PHASES) {
				runtime->indicator_breathe_rising = false;
				runtime->indicator_breathe_phase--;
			} else {
				runtime->indicator_breathe_phase++;
			}
		} else if (runtime->indicator_breathe_phase == 0) {
			runtime->indicator_breathe_rising = true;
			runtime->indicator_breathe_phase++;
		} else {
			runtime->indicator_breathe_phase--;
		}
	}

	frame_delta = now - runtime->indicator_breathe_frame_ms;
	if (frame_delta >= SQ_VM_RUNTIME_BREATHE_PWM_MS || frame_delta < 0) {
		int64_t periods = frame_delta / SQ_VM_RUNTIME_BREATHE_PWM_MS;
		if (periods < 1) {
			periods = 1;
		}
		runtime->indicator_breathe_frame_ms += periods * SQ_VM_RUNTIME_BREATHE_PWM_MS;
		frame_delta = now - runtime->indicator_breathe_frame_ms;
	}
	on_ms = runtime->indicator_breathe_phase / 2;
	on = on_ms > 0 && frame_delta < on_ms;
	return set_indicator_output(runtime, on);
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
