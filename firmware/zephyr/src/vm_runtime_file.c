#include "vm_runtime_internal.h"

#include <limits.h>

#define FILE_COPY_BINBOOK_EXT ".binbook"

static void file_copy_set_text(const char *text, const uint8_t **out, size_t *out_len)
{
	if (text == NULL) {
		*out = NULL;
		*out_len = 0;
		return;
	}
	*out = (const uint8_t *)text;
	*out_len = strlen(text);
}

static bool file_copy_name_safe(const uint8_t *name, size_t name_len)
{
	const size_t ext_len = sizeof(FILE_COPY_BINBOOK_EXT) - 1U;

	if (name == NULL || name_len <= ext_len || name_len >= SQ_VM_RUNTIME_CONTENT_NAME_LEN ||
	    name[0] == '.') {
		return false;
	}
	if (memcmp(&name[name_len - ext_len], FILE_COPY_BINBOOK_EXT, ext_len) != 0) {
		return false;
	}
	for (size_t i = 0; i < name_len; ++i) {
		if (name[i] == '\0' || name[i] == '/' || name[i] == '\\') {
			return false;
		}
	}
	return true;
}

static bool file_copy_source_safe(const uint8_t *source, size_t source_len)
{
	if (source == NULL || source_len == 0 || source_len >= SQ_APP_STORE_PATH_MAX ||
	    source[0] == '.') {
		return false;
	}
	for (size_t i = 0; i < source_len; ++i) {
		if (source[i] == '\0') {
			return false;
		}
		if (i + 1 < source_len && source[i] == '.' && source[i + 1] == '.') {
			return false;
		}
	}
	return true;
}

static int file_copy_stream(struct sq_vm_runtime *runtime, const char *source, const char *tmp,
			    int32_t *out_bytes)
{
	struct fs_file_t in;
	struct fs_file_t out;
	int result;
	int close_result;
	int32_t total = 0;

	if (runtime == NULL || source == NULL || tmp == NULL || out_bytes == NULL) {
		return -EINVAL;
	}
	*out_bytes = 0;
	result = sq_vm_runtime_transfer_acquire(runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH);
	if (result != 0) {
		return result;
	}
	fs_file_t_init(&in);
	fs_file_t_init(&out);
	result = fs_open(&in, source, FS_O_READ);
	if (result != 0) {
		goto done;
	}
	result = fs_open(&out, tmp, FS_O_CREATE | FS_O_TRUNC | FS_O_WRITE);
	if (result != 0) {
		goto done;
	}
	while (true) {
		ssize_t read = fs_read(&in, runtime->transfer.init_scratch,
				       sizeof(runtime->transfer.init_scratch));
		if (read < 0) {
			result = (int)read;
			break;
		}
		if (read == 0) {
			result = 0;
			break;
		}
		ssize_t written = fs_write(&out, runtime->transfer.init_scratch, (size_t)read);
		if (written < 0) {
			result = (int)written;
			break;
		}
		if (written != read || total > INT32_MAX - (int32_t)written) {
			result = -EIO;
			break;
		}
		total += (int32_t)written;
	}
	if (result == 0) {
		result = fs_sync(&out);
	}
	close_result = fs_close(&out);
	if (result == 0 && close_result != 0) {
		result = close_result;
	}
done:
	close_result = fs_close(&in);
	if (result == 0 && close_result != 0) {
		result = close_result;
	}
	(void)sq_vm_runtime_transfer_release(runtime, SQ_VM_RUNTIME_TRANSFER_SCRATCH);
	if (result == 0) {
		*out_bytes = total;
	}
	return result;
}

int32_t runtime_file_pick_file(void *user_data, const uint8_t *extension,
					 size_t extension_len, SqvmFilePickFileResult *out)
{
	ARG_UNUSED(user_data);
	ARG_UNUSED(extension);
	ARG_UNUSED(extension_len);

	if (out == NULL) {
		return -EINVAL;
	}
	sqvm_file_pick_file_result_unsupported(out);
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
	sqvm_file_read_text_result_unsupported(out);
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
	sqvm_file_read_lines_result_unsupported(out);
	return 0;
}

int32_t runtime_file_copy(void *user_data, const uint8_t *source, size_t source_len,
			  const uint8_t *library, size_t library_len, const uint8_t *name,
			  size_t name_len, SqvmFileCopyResult *out)
{
	struct sq_vm_runtime *runtime = user_data;
	char source_path[SQ_APP_STORE_PATH_MAX];
	char tmp_path[SQ_APP_STORE_PATH_MAX];
	char final_path[SQ_APP_STORE_PATH_MAX];
	int32_t bytes_written = 0;
	int written;
	int result;

	if (out == NULL) {
		return -EINVAL;
	}
	sqvm_file_copy_result_unsupported(out);
	if (runtime == NULL || !file_copy_source_safe(source, source_len) ||
	    library == NULL || library_len != sizeof("books") - 1U ||
	    memcmp(library, "books", library_len) != 0 ||
	    !file_copy_name_safe(name, name_len)) {
		file_copy_set_text("invalid-name", &out->error, &out->error_len);
		return 0;
	}
	memcpy(source_path, source, source_len);
	source_path[source_len] = '\0';
	result = fs_mkdir(SQ_VM_RUNTIME_CONTENT_BOOKS_DIR);
	if (result != 0 && result != -EEXIST) {
		file_copy_set_text("volume-missing", &out->error, &out->error_len);
		return 0;
	}
	written = snprintf(tmp_path, sizeof(tmp_path), SQ_VM_RUNTIME_CONTENT_BOOKS_DIR "/%.*s.upload",
			   (int)name_len, (const char *)name);
	if (written <= 0 || (size_t)written >= sizeof(tmp_path)) {
		file_copy_set_text("invalid-name", &out->error, &out->error_len);
		return 0;
	}
	written = snprintf(final_path, sizeof(final_path), SQ_VM_RUNTIME_CONTENT_BOOKS_DIR "/%.*s",
			   (int)name_len, (const char *)name);
	if (written <= 0 || (size_t)written >= sizeof(final_path)) {
		file_copy_set_text("invalid-name", &out->error, &out->error_len);
		return 0;
	}
	result = runtime_binbook_validate_path(source_path);
	if (result != 0) {
		file_copy_set_text("invalid-content", &out->error, &out->error_len);
		return 0;
	}
	(void)fs_unlink(tmp_path);
	result = fs_rename(source_path, tmp_path);
	if (result == 0) {
		struct fs_dirent entry;
		if (fs_stat(tmp_path, &entry) == 0 && entry.size <= INT32_MAX) {
			bytes_written = (int32_t)entry.size;
		}
	} else {
		result = file_copy_stream(runtime, source_path, tmp_path, &bytes_written);
	}
	if (result != 0) {
		(void)fs_unlink(tmp_path);
		file_copy_set_text(result == -ENOSPC ? "no-space" : "io-error", &out->error,
				   &out->error_len);
		return 0;
	}
	result = runtime_binbook_validate_path(tmp_path);
	if (result != 0) {
		(void)fs_unlink(tmp_path);
		file_copy_set_text("invalid-content", &out->error, &out->error_len);
		return 0;
	}
	(void)fs_unlink(final_path);
	result = fs_rename(tmp_path, final_path);
	if (result != 0) {
		(void)fs_unlink(tmp_path);
		file_copy_set_text("io-error", &out->error, &out->error_len);
		return 0;
	}
	written = snprintf(runtime->file_copy_ref, sizeof(runtime->file_copy_ref),
			   "content:books/r/%.*s", (int)name_len, (const char *)name);
	if (written <= 0 || (size_t)written >= sizeof(runtime->file_copy_ref)) {
		file_copy_set_text("invalid-name", &out->error, &out->error_len);
		return 0;
	}
	out->ok = true;
	out->error = NULL;
	out->error_len = 0;
	out->reference = (const uint8_t *)runtime->file_copy_ref;
	out->reference_len = strlen(runtime->file_copy_ref);
	out->bytes_written = bytes_written;
	return 0;
}
