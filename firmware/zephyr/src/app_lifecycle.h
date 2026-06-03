#ifndef SQUIDSCRIPT_APP_LIFECYCLE_H
#define SQUIDSCRIPT_APP_LIFECYCLE_H

#include <stddef.h>
#include <stdint.h>

#include "vm_runtime.h"

#ifdef __cplusplus
extern "C" {
#endif

enum sq_app_lifecycle_step_kind {
	SQ_APP_LIFECYCLE_STEP_NONE = 0,
	SQ_APP_LIFECYCLE_STEP_START_APP = 1,
	SQ_APP_LIFECYCLE_STEP_REGISTER_ARMED_APP = 2,
	SQ_APP_LIFECYCLE_STEP_WRITE_SLEEP_CHECKPOINT = 3,
	SQ_APP_LIFECYCLE_STEP_POLL_RUNTIME = 4,
};

struct sq_app_lifecycle_step {
	enum sq_app_lifecycle_step_kind kind;
	char app_id[SQ_APP_STORE_APP_ID_MAX];
	char event[SQ_VM_RUNTIME_EVENT_LEN];
	bool set_current;
	bool temp_app;
	int32_t wake_after_ms;
};

int sq_app_lifecycle_request_launch(struct sq_vm_runtime *runtime, const uint8_t *app,
				    size_t app_len);
int sq_app_lifecycle_request_temp_launch(struct sq_vm_runtime *runtime, const uint8_t *app,
					 size_t app_len);
int sq_app_lifecycle_request_arm(struct sq_vm_runtime *runtime, const uint8_t *app,
				 size_t app_len);
int sq_app_lifecycle_cancel_pending_arm(struct sq_vm_runtime *runtime, const uint8_t *app,
					size_t app_len);
int sq_app_lifecycle_request_sleep(struct sq_vm_runtime *runtime, int32_t wake_after_ms);
void sq_app_lifecycle_cancel_pending_after_dispatch_error(struct sq_vm_runtime *runtime,
							  int result);
void sq_app_lifecycle_cancel_pending_after_start_failure(struct sq_vm_runtime *runtime,
							 int result);
int sq_app_lifecycle_push_return(struct sq_vm_runtime *runtime, const char *app_id);
int sq_app_lifecycle_pop_return(struct sq_vm_runtime *runtime, char *out, size_t out_len);
void sq_app_lifecycle_clear_temp_routes(struct sq_vm_runtime *runtime);
int sq_app_lifecycle_restore_planned_route(
	struct sq_vm_runtime *runtime,
	const char return_stack[SQ_VM_RUNTIME_RETURN_STACK_MAX][SQ_APP_STORE_APP_ID_MAX],
	uint8_t return_stack_count);
int sq_app_lifecycle_next_step(struct sq_vm_runtime *runtime, const char *due_app,
			       const char *due_event, struct sq_app_lifecycle_step *out);
const char *sq_app_lifecycle_phase_name(enum sq_vm_runtime_lifecycle_phase phase);
const char *sq_app_lifecycle_arm_phase_name(enum sq_vm_runtime_arm_phase phase);

#ifdef __cplusplus
}
#endif

#endif
