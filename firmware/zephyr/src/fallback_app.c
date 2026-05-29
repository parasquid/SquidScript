#include "fallback_app.h"

#include <errno.h>
#include <string.h>
#include <zephyr/sys/util.h>

static int fallback_read_sqbc(void *user_data, size_t offset, uint8_t *out, size_t len)
{
	const struct sq_firmware_fallback_app *app = user_data;

	if (app == NULL || app->sqbc == NULL || out == NULL) {
		return -EINVAL;
	}
	if (offset > app->sqbc_len || len > app->sqbc_len - offset) {
		return -EIO;
	}
	memcpy(out, &app->sqbc[offset], len);
	return 0;
}

static int fallback_load_state(void *user_data, uint8_t *out, size_t out_len, size_t *len)
{
	ARG_UNUSED(user_data);
	ARG_UNUSED(out);
	ARG_UNUSED(out_len);

	if (len == NULL) {
		return -EINVAL;
	}
	*len = 0;
	return 0;
}

static int fallback_save_state(void *user_data, const uint8_t *bytes, size_t len)
{
	ARG_UNUSED(user_data);
	ARG_UNUSED(bytes);
	ARG_UNUSED(len);

	return 0;
}

static int fallback_reset_state(void *user_data)
{
	ARG_UNUSED(user_data);

	return 0;
}

struct sq_vm_storage_backend
sq_firmware_fallback_app_backend(const struct sq_firmware_fallback_app *app)
{
	return (struct sq_vm_storage_backend){
		.user_data = (void *)app,
		.read_sqbc = fallback_read_sqbc,
		.load_state = fallback_load_state,
		.save_state = fallback_save_state,
		.reset_state = fallback_reset_state,
	};
}
