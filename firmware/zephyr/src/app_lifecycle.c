#include "app_lifecycle.h"

#include <errno.h>
#include <stdbool.h>
#include <string.h>

static int copy_app_id_bytes(char *out, size_t out_cap, const uint8_t *app, size_t app_len)
{
	if (out == NULL || out_cap == 0 || app == NULL || app_len == 0 || app_len >= out_cap) {
		return -EINVAL;
	}
	memset(out, 0, out_cap);
	memcpy(out, app, app_len);
	return 0;
}

static int copy_app_id_text(char *out, size_t out_cap, const char *app)
{
	if (out == NULL || out_cap == 0 || app == NULL || app[0] == '\0') {
		return -EINVAL;
	}
	size_t len = 0;

	while (len < out_cap && app[len] != '\0') {
		len++;
	}
	if (len == 0 || len >= out_cap) {
		return -EINVAL;
	}
	memset(out, 0, out_cap);
	memcpy(out, app, len);
	return 0;
}

static int set_start_reason(struct sq_vm_runtime *runtime, const char *reason)
{
	return copy_app_id_text(runtime->start_reason, sizeof(runtime->start_reason), reason);
}

static int set_step_start(struct sq_app_lifecycle_step *step, const char *app_id,
			  const char *event, bool set_current, bool temp_app)
{
	int result;

	result = copy_app_id_text(step->app_id, sizeof(step->app_id), app_id);
	if (result != 0) {
		return result;
	}
	result = copy_app_id_text(step->event, sizeof(step->event), event);
	if (result != 0) {
		return result;
	}
	step->kind = SQ_APP_LIFECYCLE_STEP_START_APP;
	step->set_current = set_current;
	step->temp_app = temp_app;
	return 0;
}

static bool app_id_is_main(const char *app_id)
{
	return app_id != NULL && strcmp(app_id, "main") == 0;
}

int sq_app_lifecycle_request_launch(struct sq_vm_runtime *runtime, const uint8_t *app,
				    size_t app_len)
{
	int result;

	if (runtime == NULL || app == NULL || app_len == 0 ||
	    app_len >= sizeof(runtime->lifecycle_target_app)) {
		return -EINVAL;
	}
	if (sq_vm_runtime_lifecycle_busy(runtime)) {
		return -EBUSY;
	}
	result = copy_app_id_bytes(runtime->lifecycle_target_app,
				   sizeof(runtime->lifecycle_target_app), app, app_len);
	if (result != 0) {
		return result;
	}
	runtime->lifecycle_target_temp = false;
	runtime->lifecycle_phase = SQ_VM_RUNTIME_LIFECYCLE_LAUNCH_REQUESTED;
	return 0;
}

int sq_app_lifecycle_request_temp_launch(struct sq_vm_runtime *runtime, const uint8_t *app,
					 size_t app_len)
{
	int result;

	if (runtime == NULL || app == NULL || app_len == 0 ||
	    app_len >= sizeof(runtime->lifecycle_target_app)) {
		return -EINVAL;
	}
	if (sq_vm_runtime_lifecycle_busy(runtime)) {
		return -EBUSY;
	}
	result = copy_app_id_bytes(runtime->lifecycle_target_app,
				   sizeof(runtime->lifecycle_target_app), app, app_len);
	if (result != 0) {
		return result;
	}
	runtime->lifecycle_target_temp = true;
	runtime->lifecycle_phase = SQ_VM_RUNTIME_LIFECYCLE_LAUNCH_REQUESTED;
	return 0;
}

int sq_app_lifecycle_request_arm(struct sq_vm_runtime *runtime, const uint8_t *app,
				 size_t app_len)
{
	int result;

	if (runtime == NULL || app == NULL || app_len == 0 ||
	    app_len >= sizeof(runtime->arm_target_app)) {
		return -EINVAL;
	}
	if (sq_vm_runtime_arm_busy(runtime)) {
		return -EBUSY;
	}
	result = copy_app_id_bytes(runtime->arm_target_app, sizeof(runtime->arm_target_app), app,
				   app_len);
	if (result != 0) {
		return result;
	}
	runtime->arm_phase = SQ_VM_RUNTIME_ARM_REQUESTED;
	return 0;
}

int sq_app_lifecycle_cancel_pending_arm(struct sq_vm_runtime *runtime, const uint8_t *app,
					size_t app_len)
{
	if (runtime == NULL || app == NULL) {
		return -EINVAL;
	}
	if (runtime->arm_phase == SQ_VM_RUNTIME_ARM_REQUESTED &&
	    strlen(runtime->arm_target_app) == app_len &&
	    memcmp(runtime->arm_target_app, app, app_len) == 0) {
		memset(runtime->arm_target_app, 0, sizeof(runtime->arm_target_app));
		runtime->arm_phase = SQ_VM_RUNTIME_ARM_IDLE;
	}
	return 0;
}

int sq_app_lifecycle_request_sleep(struct sq_vm_runtime *runtime, int32_t wake_after_ms)
{
	if (runtime == NULL || wake_after_ms <= 0) {
		return -EINVAL;
	}
	if (runtime->current_app_temp) {
		sq_vm_runtime_record_device_error(runtime, "temp app sleep unsupported");
		return -ENOTSUP;
	}
	if (sq_vm_runtime_lifecycle_busy(runtime)) {
		return -EBUSY;
	}
	runtime->lifecycle_phase = SQ_VM_RUNTIME_LIFECYCLE_SLEEP_REQUESTED;
	runtime->planned_sleep_wake_after_ms = wake_after_ms;
	return 0;
}

void sq_app_lifecycle_cancel_pending_after_dispatch_error(struct sq_vm_runtime *runtime,
							  int result)
{
	if (runtime == NULL || result == 0) {
		return;
	}
	if (runtime->lifecycle_phase == SQ_VM_RUNTIME_LIFECYCLE_LAUNCH_REQUESTED ||
	    runtime->lifecycle_phase == SQ_VM_RUNTIME_LIFECYCLE_SLEEP_REQUESTED) {
		runtime->lifecycle_phase = SQ_VM_RUNTIME_LIFECYCLE_IDLE;
		memset(runtime->lifecycle_target_app, 0, sizeof(runtime->lifecycle_target_app));
		runtime->lifecycle_target_temp = false;
		memset(runtime->lifecycle_previous_app, 0, sizeof(runtime->lifecycle_previous_app));
		runtime->lifecycle_previous_app_temp = false;
	}
	if (runtime->lifecycle_phase == SQ_VM_RUNTIME_LIFECYCLE_IDLE &&
	    runtime->arm_phase == SQ_VM_RUNTIME_ARM_REQUESTED) {
		runtime->arm_phase = SQ_VM_RUNTIME_ARM_IDLE;
		memset(runtime->arm_target_app, 0, sizeof(runtime->arm_target_app));
	}
}

void sq_app_lifecycle_cancel_pending_after_start_failure(struct sq_vm_runtime *runtime,
							 int result)
{
	if (runtime == NULL || result == 0) {
		return;
	}
	if (runtime->lifecycle_phase == SQ_VM_RUNTIME_LIFECYCLE_EXIT_FOR_LAUNCH ||
	    runtime->lifecycle_phase == SQ_VM_RUNTIME_LIFECYCLE_SLEEP_CHECKPOINT ||
	    runtime->lifecycle_phase == SQ_VM_RUNTIME_LIFECYCLE_RETURN_REQUESTED) {
		runtime->lifecycle_phase = SQ_VM_RUNTIME_LIFECYCLE_IDLE;
		memset(runtime->lifecycle_target_app, 0, sizeof(runtime->lifecycle_target_app));
		runtime->lifecycle_target_temp = false;
		memset(runtime->lifecycle_previous_app, 0, sizeof(runtime->lifecycle_previous_app));
		runtime->lifecycle_previous_app_temp = false;
	}
}

int sq_app_lifecycle_push_return(struct sq_vm_runtime *runtime, const char *app_id)
{
	if (runtime == NULL || app_id == NULL || app_id[0] == '\0') {
		return 0;
	}
	if (runtime->return_stack_count >= SQ_VM_RUNTIME_RETURN_STACK_MAX) {
		return -ENOSPC;
	}
	int result = copy_app_id_text(runtime->return_stack[runtime->return_stack_count],
				      sizeof(runtime->return_stack[0]), app_id);

	if (result != 0) {
		return result;
	}
	runtime->return_stack_temp[runtime->return_stack_count] =
		strcmp(runtime->current_app, app_id) == 0 && runtime->current_app_temp;
	runtime->return_stack_count++;
	return 0;
}

int sq_app_lifecycle_pop_return(struct sq_vm_runtime *runtime, char *out, size_t out_len)
{
	if (runtime == NULL || out == NULL || out_len == 0) {
		return -EINVAL;
	}
	if (runtime->return_stack_count == 0) {
		runtime->lifecycle_target_temp = false;
		return copy_app_id_text(out, out_len, "main");
	}
	runtime->return_stack_count--;
	int result = copy_app_id_text(out, out_len, runtime->return_stack[runtime->return_stack_count]);

	runtime->lifecycle_target_temp = runtime->return_stack_temp[runtime->return_stack_count];
	memset(runtime->return_stack[runtime->return_stack_count], 0,
	       sizeof(runtime->return_stack[0]));
	runtime->return_stack_temp[runtime->return_stack_count] = false;
	return result;
}

void sq_app_lifecycle_clear_temp_routes(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL) {
		return;
	}
	if (runtime->current_app_temp) {
		memset(runtime->current_app, 0, sizeof(runtime->current_app));
		runtime->current_app_temp = false;
	}
	if (runtime->lifecycle_target_temp) {
		memset(runtime->lifecycle_target_app, 0, sizeof(runtime->lifecycle_target_app));
		runtime->lifecycle_target_temp = false;
	}
	if (runtime->lifecycle_previous_app_temp) {
		memset(runtime->lifecycle_previous_app, 0, sizeof(runtime->lifecycle_previous_app));
		runtime->lifecycle_previous_app_temp = false;
	}
	for (size_t i = 0; i < runtime->return_stack_count;) {
		if (!runtime->return_stack_temp[i]) {
			i++;
			continue;
		}
		for (size_t j = i; j + 1 < runtime->return_stack_count; j++) {
			memcpy(runtime->return_stack[j], runtime->return_stack[j + 1],
			       sizeof(runtime->return_stack[j]));
			runtime->return_stack_temp[j] = runtime->return_stack_temp[j + 1];
		}
		runtime->return_stack_count--;
		memset(runtime->return_stack[runtime->return_stack_count], 0,
		       sizeof(runtime->return_stack[0]));
		runtime->return_stack_temp[runtime->return_stack_count] = false;
	}
}

int sq_app_lifecycle_restore_planned_route(
	struct sq_vm_runtime *runtime,
	const char return_stack[SQ_VM_RUNTIME_RETURN_STACK_MAX][SQ_APP_STORE_APP_ID_MAX],
	uint8_t return_stack_count)
{
	if (runtime == NULL || return_stack == NULL ||
	    return_stack_count > SQ_VM_RUNTIME_RETURN_STACK_MAX) {
		return -EINVAL;
	}

	runtime->lifecycle_phase = SQ_VM_RUNTIME_LIFECYCLE_IDLE;
	memset(runtime->lifecycle_target_app, 0, sizeof(runtime->lifecycle_target_app));
	runtime->lifecycle_target_temp = false;
	memset(runtime->lifecycle_previous_app, 0, sizeof(runtime->lifecycle_previous_app));
	runtime->lifecycle_previous_app_temp = false;
	runtime->dispatch_exited = false;
	memset(runtime->return_stack, 0, sizeof(runtime->return_stack));
	memset(runtime->return_stack_temp, 0, sizeof(runtime->return_stack_temp));
	runtime->return_stack_count = 0;
	for (size_t i = 0; i < return_stack_count; i++) {
		int result = sq_app_lifecycle_push_return(runtime, return_stack[i]);

		if (result != 0) {
			memset(runtime->return_stack, 0, sizeof(runtime->return_stack));
			runtime->return_stack_count = 0;
			return result;
		}
	}
	return set_start_reason(runtime, "wake");
}

int sq_app_lifecycle_next_step(struct sq_vm_runtime *runtime, const char *due_app,
			       const char *due_event, struct sq_app_lifecycle_step *out)
{
	int result;
	bool target_temp;

	if (runtime == NULL || out == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));

	if (runtime->dispatch_exited &&
	    runtime->lifecycle_phase == SQ_VM_RUNTIME_LIFECYCLE_IDLE) {
		runtime->dispatch_exited = false;
		runtime->lifecycle_phase = SQ_VM_RUNTIME_LIFECYCLE_RETURN_REQUESTED;
	}

	switch (runtime->lifecycle_phase) {
	case SQ_VM_RUNTIME_LIFECYCLE_SLEEP_CHECKPOINT:
		runtime->lifecycle_phase = SQ_VM_RUNTIME_LIFECYCLE_IDLE;
		out->kind = SQ_APP_LIFECYCLE_STEP_WRITE_SLEEP_CHECKPOINT;
		out->wake_after_ms = runtime->planned_sleep_wake_after_ms;
		return 0;

	case SQ_VM_RUNTIME_LIFECYCLE_SLEEP_REQUESTED:
		runtime->lifecycle_phase = SQ_VM_RUNTIME_LIFECYCLE_SLEEP_CHECKPOINT;
		return set_step_start(out, runtime->current_app, "power.sleep", false,
				      runtime->current_app_temp);

	case SQ_VM_RUNTIME_LIFECYCLE_EXIT_FOR_LAUNCH:
		runtime->lifecycle_phase = SQ_VM_RUNTIME_LIFECYCLE_IDLE;
		result = sq_app_lifecycle_push_return(runtime, runtime->current_app);
		if (result != 0) {
			memset(runtime->lifecycle_target_app, 0,
			       sizeof(runtime->lifecycle_target_app));
			return result;
		}
		result = set_start_reason(runtime, "launch");
		if (result != 0) {
			return result;
		}
		target_temp = runtime->lifecycle_target_temp;
		result = set_step_start(out, runtime->lifecycle_target_app, "app.start", true,
					target_temp);
		memset(runtime->lifecycle_target_app, 0, sizeof(runtime->lifecycle_target_app));
		runtime->lifecycle_target_temp = false;
		runtime->dispatch_exited = false;
		return result;

	case SQ_VM_RUNTIME_LIFECYCLE_LAUNCH_REQUESTED:
		if (runtime->current_app[0] != '\0') {
			runtime->lifecycle_phase = SQ_VM_RUNTIME_LIFECYCLE_EXIT_FOR_LAUNCH;
			return set_step_start(out, runtime->current_app, "app.exit", false,
					      runtime->current_app_temp);
		}
		if (!app_id_is_main(runtime->lifecycle_target_app)) {
			result = sq_app_lifecycle_push_return(runtime, "main");
			if (result != 0) {
				return result;
			}
		}
		runtime->lifecycle_phase = SQ_VM_RUNTIME_LIFECYCLE_IDLE;
		result = set_start_reason(runtime, "launch");
		if (result != 0) {
			return result;
		}
		target_temp = runtime->lifecycle_target_temp;
		result = set_step_start(out, runtime->lifecycle_target_app, "app.start", true,
					target_temp);
		memset(runtime->lifecycle_target_app, 0, sizeof(runtime->lifecycle_target_app));
		runtime->lifecycle_target_temp = false;
		return result;

	case SQ_VM_RUNTIME_LIFECYCLE_RETURN_REQUESTED:
		runtime->lifecycle_phase = SQ_VM_RUNTIME_LIFECYCLE_IDLE;
		result = sq_app_lifecycle_pop_return(runtime, runtime->lifecycle_target_app,
						     sizeof(runtime->lifecycle_target_app));
		if (result != 0) {
			return result;
		}
		result = set_start_reason(runtime, "return");
		if (result != 0) {
			return result;
		}
		target_temp = runtime->lifecycle_target_temp;
		result = set_step_start(out, runtime->lifecycle_target_app, "app.start", true,
					target_temp);
		memset(runtime->lifecycle_target_app, 0, sizeof(runtime->lifecycle_target_app));
		runtime->lifecycle_target_temp = false;
		return result;

	case SQ_VM_RUNTIME_LIFECYCLE_IDLE:
		break;
	}

	if (runtime->arm_phase == SQ_VM_RUNTIME_ARM_REQUESTED) {
		result = copy_app_id_text(out->app_id, sizeof(out->app_id), runtime->arm_target_app);
		if (result != 0) {
			return result;
		}
		memset(runtime->arm_target_app, 0, sizeof(runtime->arm_target_app));
		runtime->arm_phase = SQ_VM_RUNTIME_ARM_IDLE;
		out->kind = SQ_APP_LIFECYCLE_STEP_REGISTER_ARMED_APP;
		return 0;
	}

	if (due_app != NULL && due_app[0] != '\0' && due_event != NULL && due_event[0] != '\0') {
		bool due_app_is_current = strcmp(runtime->current_app, due_app) == 0;

		if (due_app_is_current) {
			return set_step_start(out, due_app, due_event, false,
					      runtime->current_app_temp);
		}
		result = sq_app_lifecycle_push_return(runtime, runtime->current_app);
		if (result != 0) {
			return result;
		}
		result = set_start_reason(runtime, "launch");
		if (result != 0) {
			return result;
		}
		result = set_step_start(out, due_app, due_event, true, false);
		return result;
	}

	out->kind = SQ_APP_LIFECYCLE_STEP_POLL_RUNTIME;
	return 0;
}

const char *sq_app_lifecycle_phase_name(enum sq_vm_runtime_lifecycle_phase phase)
{
	switch (phase) {
	case SQ_VM_RUNTIME_LIFECYCLE_IDLE:
		return "idle";
	case SQ_VM_RUNTIME_LIFECYCLE_LAUNCH_REQUESTED:
		return "launch-requested";
	case SQ_VM_RUNTIME_LIFECYCLE_EXIT_FOR_LAUNCH:
		return "exit-for-launch";
	case SQ_VM_RUNTIME_LIFECYCLE_RETURN_REQUESTED:
		return "return-requested";
	case SQ_VM_RUNTIME_LIFECYCLE_SLEEP_REQUESTED:
		return "sleep-requested";
	case SQ_VM_RUNTIME_LIFECYCLE_SLEEP_CHECKPOINT:
		return "sleep-checkpoint";
	}
	return "unknown";
}

const char *sq_app_lifecycle_arm_phase_name(enum sq_vm_runtime_arm_phase phase)
{
	switch (phase) {
	case SQ_VM_RUNTIME_ARM_IDLE:
		return "idle";
	case SQ_VM_RUNTIME_ARM_REQUESTED:
		return "requested";
	}
	return "unknown";
}
