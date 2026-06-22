#ifndef SQUIDSCRIPT_VM_RUNTIME_DISPLAY_BACKEND_H
#define SQUIDSCRIPT_VM_RUNTIME_DISPLAY_BACKEND_H

#include "vm_runtime.h"

int sq_display_backend_flush(const struct sq_vm_runtime_display_op *ops, size_t op_count,
			     enum sq_vm_runtime_display_refresh_mode refresh_mode,
			     const struct sq_vm_runtime_binbook_page *binbook_page,
			     bool *needs_phase2);
int sq_display_backend_window_probe(const char *pattern);
void sq_display_backend_reset(void);
void sq_display_backend_set_phase2(bool phase2);

#endif
