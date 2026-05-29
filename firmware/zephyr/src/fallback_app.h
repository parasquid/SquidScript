#ifndef SQUIDSCRIPT_FALLBACK_APP_H
#define SQUIDSCRIPT_FALLBACK_APP_H

#include <stddef.h>
#include <stdint.h>

#include "vm_storage.h"

#ifdef __cplusplus
extern "C" {
#endif

struct sq_firmware_fallback_app {
	const char *app_id;
	const uint8_t *sqbc;
	size_t sqbc_len;
};

struct sq_vm_storage_backend
sq_firmware_fallback_app_backend(const struct sq_firmware_fallback_app *app);

#ifdef __cplusplus
}
#endif

#endif
