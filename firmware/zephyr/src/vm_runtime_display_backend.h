#ifndef SQUIDSCRIPT_VM_RUNTIME_DISPLAY_BACKEND_H
#define SQUIDSCRIPT_VM_RUNTIME_DISPLAY_BACKEND_H

#include "vm_runtime.h"

int sq_display_backend_flush(const struct sq_vm_runtime_display_op *ops, size_t op_count,
			     enum sq_vm_runtime_display_refresh_mode refresh_mode);

#endif
