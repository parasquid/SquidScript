#include "vm_runtime_internal.h"

int32_t runtime_timer_every(void *user_data, const uint8_t *event, size_t event_len,
				   int32_t interval_ms)
{
	return sq_vm_runtime_register_timer(user_data, event, event_len, interval_ms, true);
}

int32_t runtime_timer_after(void *user_data, const uint8_t *event, size_t event_len,
				  int32_t delay_ms)
{
	return sq_vm_runtime_register_timer(user_data, event, event_len, delay_ms, false);
}

int32_t runtime_power_sleep(void *user_data, int32_t wake_after_ms)
{
	struct sq_vm_runtime *runtime = user_data;

	if (runtime == NULL || wake_after_ms <= 0) {
		return -EINVAL;
	}
	(void)sq_vm_runtime_record_trace(runtime, (const uint8_t *)"service.power.sleep",
					 sizeof("service.power.sleep") - 1);
	return sq_app_lifecycle_request_sleep(runtime, wake_after_ms);
}

int sq_vm_runtime_register_timer(struct sq_vm_runtime *runtime, const uint8_t *event,
				 size_t event_len, int32_t interval_ms, bool repeating)
{
	if (runtime == NULL || event == NULL || event_len == 0 ||
	    event_len >= SQ_VM_RUNTIME_EVENT_LEN || interval_ms <= 0) {
		return -EINVAL;
	}
	size_t active_max = runtime->active_timer_max == 0 ? SQ_VM_RUNTIME_TIMER_MAX :
							 runtime->active_timer_max;
	for (size_t i = 0; i < active_max; i++) {
		if (runtime->timers[i].active &&
		    strncmp(runtime->timers[i].event, (const char *)event, event_len) == 0 &&
		    runtime->timers[i].event[event_len] == '\0') {
			runtime->timers[i].repeating = repeating;
			runtime->timers[i].interval_ms = interval_ms;
			runtime->timers[i].due_ms = k_uptime_get() + interval_ms;
			return 0;
		}
	}
	for (size_t i = 0; i < active_max; i++) {
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

int sq_vm_runtime_register_armed_timer(struct sq_vm_runtime *runtime, const char *app,
				       const uint8_t *event, size_t event_len,
				       int32_t interval_ms, bool repeating)
{
	if (runtime == NULL || app == NULL || app[0] == '\0' ||
	    strlen(app) >= SQ_APP_STORE_APP_ID_MAX || event == NULL || event_len == 0 ||
	    event_len >= SQ_VM_RUNTIME_EVENT_LEN || interval_ms <= 0) {
		return -EINVAL;
	}
	size_t active_max = runtime->active_armed_timer_max == 0 ? SQ_VM_RUNTIME_ARMED_TIMER_MAX :
								 runtime->active_armed_timer_max;
	for (size_t i = 0; i < active_max; i++) {
		struct sq_vm_runtime_armed_timer *timer = &runtime->armed_timers[i];
		if (timer->active && strcmp(timer->app_id, app) == 0 &&
		    strncmp(timer->event, (const char *)event, event_len) == 0 &&
		    timer->event[event_len] == '\0') {
			timer->repeating = repeating;
			timer->interval_ms = interval_ms;
			timer->due_ms = k_uptime_get() + interval_ms;
			return 0;
		}
	}
	for (size_t i = 0; i < active_max; i++) {
		struct sq_vm_runtime_armed_timer *timer = &runtime->armed_timers[i];
		if (!timer->active) {
			timer->active = true;
			timer->repeating = repeating;
			timer->interval_ms = interval_ms;
			timer->due_ms = k_uptime_get() + interval_ms;
			strncpy(timer->app_id, app, sizeof(timer->app_id) - 1);
			timer->app_id[sizeof(timer->app_id) - 1] = '\0';
			memcpy(timer->event, event, event_len);
			timer->event[event_len] = '\0';
			runtime->armed_timer_count++;
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
	size_t active_max = runtime->active_armed_timer_max == 0 ? SQ_VM_RUNTIME_ARMED_TIMER_MAX :
								 runtime->active_armed_timer_max;
	for (size_t i = 0; i < active_max; i++) {
		struct sq_vm_runtime_armed_timer *timer = &runtime->armed_timers[i];
		if (timer->active && strlen(timer->app_id) == app_len &&
		    memcmp(timer->app_id, app, app_len) == 0) {
			memset(timer, 0, sizeof(*timer));
		}
	}
	runtime->armed_timer_count = 0;
	for (size_t i = 0; i < active_max; i++) {
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
	size_t active_max = runtime->active_armed_timer_max == 0 ? SQ_VM_RUNTIME_ARMED_TIMER_MAX :
								 runtime->active_armed_timer_max;
	for (size_t i = 0; i < active_max; i++) {
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
	size_t active_max = runtime->active_timer_max == 0 ? SQ_VM_RUNTIME_TIMER_MAX :
							 runtime->active_timer_max;
	for (size_t i = 0; i < active_max; i++) {
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
