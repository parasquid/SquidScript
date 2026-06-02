#include "vm_runtime_internal.h"

int32_t runtime_file_pick_file(void *user_data, const uint8_t *extension,
					 size_t extension_len, SqvmFilePickFileResult *out)
{
	ARG_UNUSED(user_data);
	ARG_UNUSED(extension);
	ARG_UNUSED(extension_len);

	if (out == NULL) {
		return -EINVAL;
	}
	out->ok = false;
	out->error = (const uint8_t *)"unsupported";
	out->error_len = strlen("unsupported");
	out->path = NULL;
	out->path_len = 0;
	return 0;
}

int32_t runtime_file_read_text(void *user_data, const uint8_t *path, size_t path_len,
					 SqvmFileReadTextResult *out)
{
	ARG_UNUSED(user_data);
	ARG_UNUSED(path);
	ARG_UNUSED(path_len);

	if (out == NULL) {
		return -EINVAL;
	}
	out->ok = false;
	out->error = (const uint8_t *)"unsupported";
	out->error_len = strlen("unsupported");
	out->text = NULL;
	out->text_len = 0;
	return 0;
}

int32_t runtime_file_read_lines(void *user_data, const uint8_t *path, size_t path_len,
					  int32_t max_lines, SqvmFileReadLinesResult *out)
{
	ARG_UNUSED(user_data);
	ARG_UNUSED(path);
	ARG_UNUSED(path_len);
	ARG_UNUSED(max_lines);

	if (out == NULL) {
		return -EINVAL;
	}
	out->ok = false;
	out->error = (const uint8_t *)"unsupported";
	out->error_len = strlen("unsupported");
	return 0;
}

