#include "vm_runtime_internal.h"

#include "squidscript_target_defaults.h"

struct sq_vm_runtime_binding_scratch {
	SqvmDeviceBinding binding;
	SqdcDeviceBindingPlan plan;
};

static int32_t runtime_device_config_unsupported(SqvmDeviceConfigResult *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
	sqvm_device_config_result_unsupported(out);
	return 0;
}

static int32_t runtime_device_config_error(SqvmDeviceConfigResult *out, const char *error)
{
	if (out == NULL || error == NULL) {
		return -EINVAL;
	}
	memset(out, 0, sizeof(*out));
	out->ok = false;
	out->error = (const uint8_t *)error;
	out->error_len = strlen(error);
	return 0;
}

static int runtime_device_config_result_errno(const SqvmDeviceConfigResult *result)
{
	static const char unsupported_target_gpio[] = "unsupported target gpio";

	if (result == NULL || result->ok) {
		return 0;
	}
	if (result->error != NULL && result->error_len == strlen(unsupported_target_gpio) &&
	    memcmp(result->error, unsupported_target_gpio, strlen(unsupported_target_gpio)) == 0) {
		return -ENOTSUP;
	}
	return -EINVAL;
}

static int32_t runtime_device_config_ok(SqvmDeviceConfigResult *out)
{
	if (out == NULL) {
		return -EINVAL;
	}
	sqvm_device_config_result_ok(out);
	return 0;
}

static const char *runtime_device_config_status_error(SqdcStatus status)
{
	switch (status) {
	case SQDC_STATUS_OK:
		return NULL;
	case SQDC_STATUS_BUFFER_TOO_SMALL:
		return "buffer too small";
	case SQDC_STATUS_PARSE_ERROR:
		return "parse error";
	case SQDC_STATUS_TOO_MANY_RECORDS:
		return "too many records";
	case SQDC_STATUS_INVALID_ARGUMENT:
	default:
		return "invalid argument";
	}
}

static int runtime_device_config_read_file(const char *path, uint8_t *buffer, size_t buffer_len,
					   size_t *out_len)
{
	struct fs_file_t file;
	int result;
	uint8_t overflow;
	ssize_t read;
	ssize_t extra = 0;

	if (path == NULL || buffer == NULL || out_len == NULL) {
		return -EINVAL;
	}
	*out_len = 0;
	fs_file_t_init(&file);
	result = fs_open(&file, path, FS_O_READ);
	if (result != 0) {
		if (result == -EISDIR) {
			return -ENOENT;
		}
		return result;
	}
	read = fs_read(&file, buffer, buffer_len);
	if (read >= 0 && (size_t)read == buffer_len) {
		extra = fs_read(&file, &overflow, sizeof(overflow));
	}
	result = fs_close(&file);
	if (read < 0) {
		return (int)read;
	}
	if (extra < 0) {
		return (int)extra;
	}
	if (extra > 0) {
		return -ENOSPC;
	}
	*out_len = (size_t)read;
	return result;
}

static int runtime_device_config_write_file(const char *path, const uint8_t *bytes, size_t len)
{
	struct fs_file_t file;
	int result;
	ssize_t written;

	if (path == NULL || bytes == NULL || len == 0) {
		return -EINVAL;
	}

	fs_file_t_init(&file);
	result = fs_open(&file, path, FS_O_CREATE | FS_O_WRITE | FS_O_TRUNC);
	if (result != 0) {
		return result;
	}
	written = fs_write(&file, bytes, len);
	result = fs_close(&file);
	if (written < 0) {
		return (int)written;
	}
	if ((size_t)written != len) {
		return -EIO;
	}
	return result;
}

static int sq_vm_runtime_device_config_load_resource(struct sq_vm_runtime *runtime,
						     const uint8_t *resource_bytes,
						     size_t resource_len,
						     SqvmDeviceConfigResult *out)
{
	char path[SQ_APP_STORE_PATH_MAX];
	size_t bytes_len;
	SqdcStatus status;
	int result;

	if (runtime == NULL || resource_bytes == NULL || out == NULL) {
		return -EINVAL;
	}
	if (runtime->store_mount_point == NULL || runtime->current_app[0] == '\0') {
		return runtime_device_config_error(out, "no current app");
	}

	status = sqdc_is_safe_sqdevice_path(resource_bytes, resource_len);
	if (status != SQDC_STATUS_OK) {
		return runtime_device_config_error(out, "invalid resource path");
	}
	result = sq_app_store_resource_path_bytes(runtime->store_mount_point, runtime->current_app,
						  resource_bytes, resource_len, path,
						  sizeof(path));
	if (result != 0) {
		return runtime_device_config_error(out, "resource path failed");
	}
	result = sq_vm_runtime_transfer_acquire(runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION);
	if (result != 0) {
		return runtime_device_config_error(out, "transfer busy");
	}
	result = runtime_device_config_read_file(path, runtime->transfer.completion.bytes,
						 sizeof(runtime->transfer.completion.bytes),
						 &bytes_len);
	if (result == -ENOSPC) {
		(void)sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION);
		return runtime_device_config_error(out, "resource too large");
	}
	if (result != 0) {
		(void)sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION);
		return runtime_device_config_error(out, "resource read failed");
	}

	status = sqdc_parse_sqdevice(runtime->transfer.completion.bytes, bytes_len,
				     &runtime->device_config_draft);
	result = sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION);
	if (result != 0) {
		return runtime_device_config_error(out, "transfer release failed");
	}
	if (status != SQDC_STATUS_OK) {
		const char *error = runtime_device_config_status_error(status);
		return runtime_device_config_error(out, error != NULL ? error : "parse error");
	}
	runtime->device_config_draft_loaded = true;
	return runtime_device_config_ok(out);
}

int sq_vm_runtime_device_config_load(struct sq_vm_runtime *runtime, const uint8_t *source,
				     size_t source_len, SqvmDeviceConfigResult *out)
{
	static const char package_prefix[] = "package:";
	const uint8_t *resource_bytes;
	size_t resource_len;

	if (runtime == NULL || source == NULL || out == NULL) {
		return -EINVAL;
	}
	if (source_len <= sizeof(package_prefix) - 1 ||
	    memcmp(source, package_prefix, sizeof(package_prefix) - 1) != 0) {
		return runtime_device_config_error(out, "unsupported source");
	}
	if (runtime->store_mount_point == NULL || runtime->current_app[0] == '\0') {
		return runtime_device_config_error(out, "no current app");
	}

	resource_bytes = source + sizeof(package_prefix) - 1;
	resource_len = source_len - (sizeof(package_prefix) - 1);
	return sq_vm_runtime_device_config_load_resource(runtime, resource_bytes, resource_len, out);
}

#if SQ_TARGET_INDICATOR_DEFAULT_HAS_GPIO
static int runtime_device_config_append_string(SqdcConfig *config, const char *key,
					       const char *value)
{
	SqdcRecord *record;
	size_t key_len;
	size_t value_len;

	if (config == NULL || key == NULL || value == NULL ||
	    config->count >= SQDC_CONFIG_MAX_RECORDS) {
		return -EINVAL;
	}
	key_len = strlen(key);
	value_len = strlen(value);
	if (key_len == 0 || key_len > SQDC_CONFIG_KEY_CAP ||
	    value_len > SQDC_CONFIG_STRING_CAP) {
		return -EINVAL;
	}
	record = &config->records[config->count];
	memset(record, 0, sizeof(*record));
	record->present = true;
	memcpy(record->key, key, key_len);
	record->key_len = key_len;
	record->value.kind = SQDC_VALUE_STRING;
	memcpy(record->value.string, value, value_len);
	record->value.string_len = value_len;
	config->count++;
	return 0;
}

static int runtime_device_config_append_bool(SqdcConfig *config, const char *key, bool value)
{
	SqdcRecord *record;
	size_t key_len;

	if (config == NULL || key == NULL || config->count >= SQDC_CONFIG_MAX_RECORDS) {
		return -EINVAL;
	}
	key_len = strlen(key);
	if (key_len == 0 || key_len > SQDC_CONFIG_KEY_CAP) {
		return -EINVAL;
	}
	record = &config->records[config->count];
	memset(record, 0, sizeof(*record));
	record->present = true;
	memcpy(record->key, key, key_len);
	record->key_len = key_len;
	record->value.kind = SQDC_VALUE_BOOL;
	record->value.bool_value = value;
	config->count++;
	return 0;
}
#endif

int sq_vm_runtime_device_config_set(struct sq_vm_runtime *runtime, const uint8_t *key,
				    size_t key_len, SqvmDeviceConfigValue value,
				    SqvmDeviceConfigResult *out)
{
	SqdcStatus status;

	if (runtime == NULL || key == NULL || out == NULL) {
		return -EINVAL;
	}
	if (!runtime->device_config_draft_loaded) {
		return runtime_device_config_error(out, "no draft");
	}

	switch (value.kind) {
	case SQVM_DEVICE_CONFIG_VALUE_NULL:
		status = sqdc_config_set_null(&runtime->device_config_draft, key, key_len);
		break;
	case SQVM_DEVICE_CONFIG_VALUE_BOOL:
		status = sqdc_config_set_bool(&runtime->device_config_draft, key, key_len,
					      value.bool_value);
		break;
	case SQVM_DEVICE_CONFIG_VALUE_I32:
		status = sqdc_config_set_i32(&runtime->device_config_draft, key, key_len,
					     value.i32_value);
		break;
	case SQVM_DEVICE_CONFIG_VALUE_STRING:
		status = sqdc_config_set_string(&runtime->device_config_draft, key, key_len,
						value.string, value.string_len);
		break;
	default:
		status = SQDC_STATUS_INVALID_ARGUMENT;
		break;
	}
	if (status != SQDC_STATUS_OK) {
		const char *error = runtime_device_config_status_error(status);
		return runtime_device_config_error(out, error != NULL ? error : "invalid argument");
	}
	return runtime_device_config_ok(out);
}

static const SqdcRecord *runtime_device_config_find(const SqdcConfig *config, const char *key)
{
	size_t key_len;

	if (config == NULL || key == NULL) {
		return NULL;
	}
	key_len = strlen(key);
	for (size_t i = 0; i < config->count; i++) {
		const SqdcRecord *record = &config->records[i];
		if (record->present && record->key_len == key_len &&
		    memcmp(record->key, key, key_len) == 0) {
			return record;
		}
	}
	return NULL;
}

static bool runtime_device_config_string_equals(const SqdcConfig *config, const char *key,
						const char *expected)
{
	const SqdcRecord *record = runtime_device_config_find(config, key);
	size_t expected_len = strlen(expected);

	return record != NULL && record->value.kind == SQDC_VALUE_STRING &&
	       record->value.string_len == expected_len &&
	       memcmp(record->value.string, expected, expected_len) == 0;
}

static bool runtime_device_config_string_equals_bytes(const SqdcConfig *config, const char *key,
						      const uint8_t *expected,
						      size_t expected_len)
{
	const SqdcRecord *record = runtime_device_config_find(config, key);

	return record != NULL && expected != NULL && record->value.kind == SQDC_VALUE_STRING &&
	       record->value.string_len == expected_len &&
	       memcmp(record->value.string, expected, expected_len) == 0;
}

static int runtime_device_config_read_string(const SqdcConfig *config, const char *key,
					     const uint8_t **out, size_t *out_len)
{
	const SqdcRecord *record = runtime_device_config_find(config, key);

	if (record == NULL || out == NULL || out_len == NULL ||
	    record->value.kind != SQDC_VALUE_STRING) {
		return -EINVAL;
	}
	*out = record->value.string;
	*out_len = record->value.string_len;
	return 0;
}

static int runtime_device_config_read_bool(const SqdcConfig *config, const char *key, bool *out)
{
	const SqdcRecord *record = runtime_device_config_find(config, key);

	if (record == NULL || out == NULL || record->value.kind != SQDC_VALUE_BOOL) {
		return -EINVAL;
	}
	*out = record->value.bool_value;
	return 0;
}

static bool runtime_active_binding_matches(const struct sq_vm_runtime_active_binding *binding,
					   const uint8_t *alias, size_t alias_len)
{
	size_t stored_len;

	if (binding == NULL || !binding->active || alias == NULL || alias_len == 0 ||
	    alias_len >= sizeof(binding->alias)) {
		return false;
	}
	stored_len = bounded_strlen(binding->alias, sizeof(binding->alias));
	return stored_len == alias_len && memcmp(binding->alias, alias, alias_len) == 0;
}

static int runtime_activate_binding(struct sq_vm_runtime *runtime, const uint8_t *alias,
				    size_t alias_len)
{
	struct sq_vm_runtime_active_binding *slot = NULL;

	if (runtime == NULL || alias == NULL || alias_len == 0 ||
	    alias_len >= SQVM_DEVICE_BINDING_NAME_CAP) {
		return -EINVAL;
	}
	for (size_t i = 0; i < SQ_VM_RUNTIME_ACTIVE_BINDING_MAX; i++) {
		if (runtime_active_binding_matches(&runtime->active_bindings[i], alias, alias_len)) {
			return 0;
		}
		if (slot == NULL && !runtime->active_bindings[i].active) {
			slot = &runtime->active_bindings[i];
		}
	}
	if (slot == NULL) {
		return -ENOSPC;
	}
	memset(slot, 0, sizeof(*slot));
	slot->active = true;
	memcpy(slot->alias, alias, alias_len);
	slot->alias[alias_len] = '\0';
	runtime->active_binding_count++;
	return 0;
}

static int runtime_apply_indicator_gpio_binding(struct sq_vm_runtime *runtime, const uint8_t *alias,
						size_t alias_len, uint8_t pin, bool active_low)
{
	if (runtime == NULL || alias == NULL || alias_len == 0) {
		return -EINVAL;
	}
	runtime->indicator_pattern = SQ_VM_RUNTIME_INDICATOR_STEADY;
	runtime->indicator_binding_active = true;
	runtime->indicator_binding_pin = pin;
	runtime->indicator_binding_active_low = active_low;
	return runtime_activate_binding(runtime, alias, alias_len);
}

static int runtime_activate_input_button(struct sq_vm_runtime *runtime, uint8_t pin,
					 const uint8_t *event, size_t event_len,
					 bool active_low)
{
	struct sq_vm_runtime_input_button *slot = NULL;
	bool pressed = false;
	int result;

	if (runtime == NULL || event == NULL || event_len == 0 ||
	    event_len >= SQ_VM_RUNTIME_EVENT_LEN) {
		return -EINVAL;
	}
	for (size_t i = 0; i < SQ_VM_RUNTIME_INPUT_BUTTON_MAX; i++) {
		if (runtime->input_buttons[i].active && runtime->input_buttons[i].pin == pin) {
			slot = &runtime->input_buttons[i];
			break;
		}
		if (slot == NULL && !runtime->input_buttons[i].active) {
			slot = &runtime->input_buttons[i];
		}
	}
	if (slot == NULL) {
		return -ENOSPC;
	}

	if (!slot->active) {
		runtime->input_button_count++;
	}
	memset(slot, 0, sizeof(*slot));
	slot->active = true;
	slot->pin = pin;
	slot->active_low = active_low;
	slot->pressed = pressed;
	slot->phase = SQ_VM_RUNTIME_INPUT_RELEASED;
	slot->next_poll_ms = k_uptime_get() + SQ_VM_RUNTIME_INPUT_POLL_MS;
	slot->debounce_until_ms = k_uptime_get() + SQ_VM_RUNTIME_INPUT_DEBOUNCE_MS;
	memcpy(slot->event, event, event_len);
	slot->event[event_len] = '\0';
	result = configure_input_button_gpio(pin, active_low, &pressed);
	if (result != 0) {
		memset(slot, 0, sizeof(*slot));
		runtime->input_button_count--;
		return result;
	}
	slot->pressed = pressed;
	slot->phase = pressed ? SQ_VM_RUNTIME_INPUT_PRESSED : SQ_VM_RUNTIME_INPUT_RELEASED;
	return 0;
}

int sq_vm_runtime_device_config_rebind(struct sq_vm_runtime *runtime, const uint8_t *alias,
				       size_t alias_len, SqvmDeviceConfigResult *out)
{
	const uint8_t *pin_name;
	size_t pin_name_len;
	const uint8_t *event;
	size_t event_len;
	uint8_t pin;
	bool active_low;

	if (runtime == NULL || alias == NULL || out == NULL) {
		return -EINVAL;
	}
	if (!runtime->device_config_draft_loaded) {
		return runtime_device_config_error(out, "no draft");
	}
	if (runtime_device_config_string_equals(&runtime->device_config_draft, "mode",
						"gpio-button")) {
		if (!runtime_device_config_string_equals_bytes(&runtime->device_config_draft,
							       "service", alias, alias_len)) {
			return runtime_device_config_error(out, "invalid binding");
		}
		if (runtime_device_config_read_string(&runtime->device_config_draft, "pinName",
						      &pin_name, &pin_name_len) != 0 ||
		    parse_gpio_name(pin_name, pin_name_len, &pin) != 0 ||
		    runtime_device_config_read_string(&runtime->device_config_draft, "event",
						      &event, &event_len) != 0 ||
		    event_len >= SQ_VM_RUNTIME_EVENT_LEN ||
		    runtime_device_config_read_bool(&runtime->device_config_draft, "activeLow",
						    &active_low) != 0) {
			return runtime_device_config_error(out, "invalid binding");
		}
		if (!target_gpio_pin_supported(pin)) {
			return runtime_device_config_error(out, "unsupported target gpio");
		}
		if (runtime_activate_binding(runtime, alias, alias_len) != 0 ||
		    runtime_activate_input_button(runtime, pin, event, event_len, active_low) != 0) {
			return runtime_device_config_error(out, "too many bindings");
		}
		return runtime_device_config_ok(out);
	}
	if (alias_len != strlen("indicator.default") ||
	    memcmp(alias, "indicator.default", alias_len) != 0) {
		if (!runtime_device_config_string_equals_bytes(&runtime->device_config_draft,
							       "service", alias, alias_len)) {
			return runtime_device_config_error(out, "invalid binding");
		}
		if (runtime_activate_binding(runtime, alias, alias_len) != 0) {
			return runtime_device_config_error(out, "too many bindings");
		}
		return runtime_device_config_ok(out);
	}
	if (!runtime_device_config_string_equals(&runtime->device_config_draft, "service",
						 "indicator.default") ||
	    !runtime_device_config_string_equals(&runtime->device_config_draft, "mode", "gpio")) {
		return runtime_device_config_error(out, "invalid binding");
	}
	if (runtime_device_config_read_string(&runtime->device_config_draft, "pinName", &pin_name,
					      &pin_name_len) != 0 ||
	    parse_gpio_name(pin_name, pin_name_len, &pin) != 0 ||
	    runtime_device_config_read_bool(&runtime->device_config_draft, "activeLow",
					    &active_low) != 0) {
		return runtime_device_config_error(out, "invalid binding");
	}
	if (!target_gpio_pin_supported(pin)) {
		return runtime_device_config_error(out, "unsupported target gpio");
	}

	if (runtime_apply_indicator_gpio_binding(runtime, alias, alias_len, pin, active_low) != 0) {
		return runtime_device_config_error(out, "too many bindings");
	}
	return runtime_device_config_ok(out);
}

int sq_vm_runtime_apply_target_default_indicator_binding(struct sq_vm_runtime *runtime)
{
#if SQ_TARGET_INDICATOR_DEFAULT_HAS_GPIO
	char pin_name[sizeof("GPIO255")];
	int written;

	if (runtime == NULL) {
		return -EINVAL;
	}

	written = snprintf(pin_name, sizeof(pin_name), "GPIO%u",
			   SQ_TARGET_INDICATOR_DEFAULT_GPIO_PIN);
	if (written <= 0 || (size_t)written >= sizeof(pin_name)) {
		return -EINVAL;
	}

	memset(&runtime->device_config_draft, 0, sizeof(runtime->device_config_draft));
	if (runtime_device_config_append_string(&runtime->device_config_draft, "service",
						"indicator.default") != 0 ||
	    runtime_device_config_append_string(&runtime->device_config_draft, "mode",
						"gpio") != 0 ||
	    runtime_device_config_append_string(&runtime->device_config_draft, "pinName",
						pin_name) != 0 ||
	    runtime_device_config_append_bool(&runtime->device_config_draft, "activeLow",
					      SQ_TARGET_INDICATOR_DEFAULT_ACTIVE_LOW != 0) != 0) {
		return -EINVAL;
	}

	runtime->device_config_draft_loaded = true;
	if (runtime_apply_indicator_gpio_binding(runtime, (const uint8_t *)"indicator.default",
						 strlen("indicator.default"),
						 SQ_TARGET_INDICATOR_DEFAULT_GPIO_PIN,
						 SQ_TARGET_INDICATOR_DEFAULT_ACTIVE_LOW != 0) != 0) {
		return -EINVAL;
	}
	return 0;
#else
	if (runtime == NULL) {
		return -EINVAL;
	}
	runtime->indicator_binding_active = false;
	runtime->indicator_binding_pin = 0;
	runtime->indicator_binding_active_low = false;
	runtime->device_config_draft_loaded = false;
	return 0;
#endif
}

void runtime_clear_active_bindings(struct sq_vm_runtime *runtime)
{
	if (runtime == NULL) {
		return;
	}
	memset(runtime->active_bindings, 0, sizeof(runtime->active_bindings));
	runtime->active_binding_count = 0;
	runtime->indicator_pattern = SQ_VM_RUNTIME_INDICATOR_STEADY;
	runtime->indicator_pattern_step = 0;
	runtime->indicator_pattern_on = false;
	runtime->indicator_pattern_on_ms = 0;
	runtime->indicator_pattern_off_ms = 0;
	runtime->indicator_pattern_next_ms = 0;
	runtime->indicator_binding_active = false;
	runtime->indicator_binding_pin = 0;
	runtime->indicator_binding_active_low = false;
	memset(runtime->input_buttons, 0, sizeof(runtime->input_buttons));
	runtime->input_button_count = 0;
	memset(&runtime->device_config_draft, 0, sizeof(runtime->device_config_draft));
	runtime->device_config_draft_loaded = false;
}

static size_t runtime_fixed_text_len(const uint8_t *bytes, size_t cap)
{
	size_t len = 0;

	while (len < cap && bytes[len] != 0) {
		len++;
	}
	return len;
}

static int __noinline sq_vm_runtime_apply_saved_device_config(struct sq_vm_runtime *runtime)
{
	char path[SQ_APP_STORE_DEVICE_CONFIG_PATH_MAX];
	size_t bytes_len = 0;
	SqvmDeviceConfigResult result = {0};
	SqdcStatus status;
	int fs_result;

	if (runtime == NULL || runtime->store_mount_point == NULL) {
		return 0;
	}

	fs_result = sq_app_store_device_config_path(runtime->store_mount_point, path, sizeof(path));
	if (fs_result != 0) {
		return fs_result;
	}
	fs_result = sq_vm_runtime_transfer_acquire(runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION);
	if (fs_result != 0) {
		return fs_result;
	}
	fs_result = runtime_device_config_read_file(path, runtime->transfer.completion.bytes,
						    sizeof(runtime->transfer.completion.bytes),
						    &bytes_len);
	if (fs_result == -ENOENT) {
		(void)sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION);
		return 0;
	}
	if (fs_result != 0) {
		(void)sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION);
		return fs_result;
	}

	status = sqdc_decode_sqdc(runtime->transfer.completion.bytes, bytes_len,
				  &runtime->device_config_draft);
	fs_result = sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION);
	if (fs_result != 0) {
		return fs_result;
	}
	if (status != SQDC_STATUS_OK) {
		return -EINVAL;
	}
	runtime->device_config_draft_loaded = true;
	if (sq_vm_runtime_device_config_rebind(runtime, (const uint8_t *)"indicator.default",
					       strlen("indicator.default"), &result) != 0 ||
	    !result.ok) {
		return -EINVAL;
	}
	return 0;
}

static int __noinline sq_vm_runtime_apply_device_bindings(struct sq_vm_runtime *runtime)
{
	size_t count = 0;
	SqvmStatus status;
	int transfer_result;

	if (runtime == NULL || runtime->backend == NULL || runtime->backend->read_sqbc == NULL ||
	    runtime->store_mount_point == NULL || runtime->current_app[0] == '\0') {
		return 0;
	}

	transfer_result = sq_vm_runtime_transfer_acquire(runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH);
	if (transfer_result != 0) {
		return transfer_result;
	}
	status = sqvm_device_binding_count_from_reader(runtime, runtime_read_exact_at,
						       runtime->transfer.init_scratch,
						       sizeof(runtime->transfer.init_scratch),
						       &count);
	if (sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH) != 0) {
		return -EBUSY;
	}
	if (status != SQVM_STATUS_OK) {
		return sq_vm_runtime_status_to_errno(status);
	}

	for (size_t index = 0; index < count; index++) {
		SqvmDeviceConfigResult result_storage = {0};
		struct sq_vm_runtime_binding_scratch *scratch =
			(struct sq_vm_runtime_binding_scratch *)runtime->transfer.init_scratch;
		BUILD_ASSERT(sizeof(*scratch) <= sizeof(runtime->transfer.init_scratch));
		SqvmDeviceBinding *binding = &scratch->binding;
		SqdcDeviceBindingPlan *plan = &scratch->plan;
		SqvmDeviceConfigResult *result = &result_storage;
		size_t resource_len;
		size_t service_len;
		size_t binding_len;

		transfer_result =
			sq_vm_runtime_transfer_acquire(runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH);
		if (transfer_result != 0) {
			return transfer_result;
		}
		memset(scratch, 0, sizeof(*scratch));
		status = sqvm_device_binding_read_from_reader(runtime, runtime_read_exact_at,
							      runtime->transfer.init_scratch,
							      sizeof(runtime->transfer.init_scratch),
							      index, binding);
		if (status != SQVM_STATUS_OK) {
			(void)sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH);
			return sq_vm_runtime_status_to_errno(status);
		}

		service_len = runtime_fixed_text_len(binding->service, sizeof(binding->service));
		binding_len = runtime_fixed_text_len(binding->binding, sizeof(binding->binding));
		resource_len = runtime_fixed_text_len(binding->resource, sizeof(binding->resource));
		if (service_len == 0 || service_len >= sizeof(binding->service) ||
		    binding_len == 0 || binding_len >= sizeof(binding->binding) ||
		    resource_len == 0 || resource_len >= sizeof(binding->resource)) {
			(void)sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH);
			return -EINVAL;
		}

		if (sqdc_plan_device_binding(binding->service, service_len, binding->binding,
					     binding_len, binding->resource, resource_len,
					     plan, &runtime->device_config_draft) !=
		    SQDC_STATUS_OK) {
			(void)sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH);
			return -ENOTSUP;
		}

		switch (plan->kind) {
		case SQDC_DEVICE_BINDING_RESOURCE_INLINE_GPIO:
		case SQDC_DEVICE_BINDING_RESOURCE_INLINE_GPIO_BUTTON:
			runtime->device_config_draft_loaded = true;
			if (sq_vm_runtime_device_config_rebind(runtime, plan->alias,
							       plan->alias_len, result) != 0) {
				(void)sq_vm_runtime_transfer_release(runtime,
								     SQ_VM_RUNTIME_TRANSFER_SCRATCH);
				return -EINVAL;
			}
			if (sq_vm_runtime_transfer_release(runtime,
							   SQ_VM_RUNTIME_TRANSFER_SCRATCH) != 0) {
				return -EBUSY;
			}
			if (!result->ok) {
				return runtime_device_config_result_errno(result);
			}
			break;
		case SQDC_DEVICE_BINDING_RESOURCE_PACKAGE_SQDEVICE:
		{
			uint8_t alias[SQVM_DEVICE_BINDING_NAME_CAP];
			size_t alias_len = plan->alias_len;
			size_t package_resource_len = plan->resource_len;
			if (alias_len == 0 || alias_len >= sizeof(alias)) {
				(void)sq_vm_runtime_transfer_release(runtime,
								     SQ_VM_RUNTIME_TRANSFER_SCRATCH);
				return -EINVAL;
			}
			if (package_resource_len == 0 ||
			    package_resource_len >= SQVM_DEVICE_BINDING_RESOURCE_CAP) {
				(void)sq_vm_runtime_transfer_release(runtime,
								     SQ_VM_RUNTIME_TRANSFER_SCRATCH);
				return -EINVAL;
			}
			memcpy(alias, plan->alias, alias_len);
			if (sq_vm_runtime_transfer_release(runtime,
							   SQ_VM_RUNTIME_TRANSFER_SCRATCH) != 0) {
				return -EBUSY;
			}
			if (sq_vm_runtime_device_config_load_resource(runtime, plan->resource,
								      package_resource_len, result) != 0 ||
			    !result->ok) {
				return -EINVAL;
			}
			memset(result, 0, sizeof(*result));
			if (sq_vm_runtime_device_config_rebind(runtime, alias, alias_len, result) !=
			    0) {
				return -EINVAL;
			}
			if (!result->ok) {
				return runtime_device_config_result_errno(result);
			}
			break;
		}
		default:
			(void)sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH);
			return -ENOTSUP;
		}
	}
	return 0;
}

int __noinline sq_vm_runtime_prepare_app_start(struct sq_vm_runtime *runtime)
{
	int result;

	if (runtime == NULL) {
		return -EINVAL;
	}
	runtime_clear_active_bindings(runtime);
	result = sq_vm_runtime_apply_target_default_indicator_binding(runtime);
	if (result != 0) {
		return result;
	}
	result = sq_vm_runtime_apply_saved_device_config(runtime);
	if (result != 0) {
		return result;
	}
	return sq_vm_runtime_apply_device_bindings(runtime);
}

int32_t runtime_device_config_load(void *user_data, const uint8_t *source,
					  size_t source_len, SqvmDeviceConfigResult *out)
{
	return sq_vm_runtime_device_config_load(user_data, source, source_len, out);
}

int32_t runtime_device_config_set(void *user_data, const uint8_t *key,
					 size_t key_len, SqvmDeviceConfigValue value,
					 SqvmDeviceConfigResult *out)
{
	return sq_vm_runtime_device_config_set(user_data, key, key_len, value, out);
}

int32_t runtime_device_config_rebind(void *user_data, const uint8_t *alias,
					    size_t alias_len, SqvmDeviceConfigResult *out)
{
	return sq_vm_runtime_device_config_rebind(user_data, alias, alias_len, out);
}

int sq_vm_runtime_device_config_save(struct sq_vm_runtime *runtime, const uint8_t *destination,
				     size_t destination_len, SqvmDeviceConfigResult *out)
{
	char path[SQ_APP_STORE_DEVICE_CONFIG_PATH_MAX];
	size_t encoded_len = 0;
	SqdcStatus status;
	int result;

	if (runtime == NULL || destination == NULL || out == NULL) {
		return -EINVAL;
	}
	if (destination_len != strlen("flash") || memcmp(destination, "flash", destination_len) != 0) {
		return runtime_device_config_unsupported(out);
	}
	if (runtime->store_mount_point == NULL) {
		return runtime_device_config_error(out, "no store");
	}
	if (!runtime->device_config_draft_loaded) {
		return runtime_device_config_error(out, "no draft");
	}

	result = sq_app_store_prepare_filesystem(runtime->store_mount_point);
	if (result != 0) {
		return runtime_device_config_error(out, "storage prepare failed");
	}
	result = sq_app_store_device_config_path(runtime->store_mount_point, path, sizeof(path));
	if (result != 0) {
		return runtime_device_config_error(out, "config path failed");
	}

	result = sq_vm_runtime_transfer_acquire(runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION);
	if (result != 0) {
		return runtime_device_config_error(out, "transfer busy");
	}
	status = sqdc_encode_sqdc(&runtime->device_config_draft,
				  runtime->transfer.completion.bytes,
				  sizeof(runtime->transfer.completion.bytes), &encoded_len);
	if (status != SQDC_STATUS_OK) {
		const char *error = runtime_device_config_status_error(status);
		(void)sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION);
		return runtime_device_config_error(out, error != NULL ? error : "encode error");
	}
	result = runtime_device_config_write_file(path, runtime->transfer.completion.bytes,
						  encoded_len);
	if (sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_COMPLETION) != 0) {
		return runtime_device_config_error(out, "transfer release failed");
	}
	if (result != 0) {
		return runtime_device_config_error(out, "config write failed");
	}
	return runtime_device_config_ok(out);
}

int32_t runtime_device_config_save(void *user_data, const uint8_t *destination,
					  size_t destination_len, SqvmDeviceConfigResult *out)
{
	return sq_vm_runtime_device_config_save(user_data, destination, destination_len, out);
}
