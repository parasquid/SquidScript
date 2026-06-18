#include "vm_runtime_internal.h"

#include "app_store.h"

#include <limits.h>

#define CONTENT_BINBOOK_EXT ".binbook"
#define CONTENT_PACKAGE_PREFIX "content:books/p/"
#define CONTENT_REMOVABLE_PREFIX "content:books/r/"

struct content_scan {
	struct sq_vm_runtime *runtime;
	SqvmContentBinBookEntry *out;
	size_t out_cap;
	size_t *out_count;
	int32_t offset;
	int32_t limit;
	int32_t total;
};

static bool content_name_safe(const char *name)
{
	size_t len;
	size_t ext_len = sizeof(CONTENT_BINBOOK_EXT) - 1U;

	if (name == NULL || name[0] == '\0' || name[0] == '.') {
		return false;
	}
	len = strlen(name);
	if (len <= ext_len || len >= SQ_VM_RUNTIME_CONTENT_NAME_LEN) {
		return false;
	}
	if (strcmp(name + len - ext_len, CONTENT_BINBOOK_EXT) != 0) {
		return false;
	}
	for (size_t i = 0; i < len; ++i) {
		if (name[i] == '/' || name[i] == '\\') {
			return false;
		}
	}
	return true;
}

static void content_set_text(const char *text, const uint8_t **out, size_t *out_len)
{
	if (text == NULL) {
		*out = NULL;
		*out_len = 0;
		return;
	}
	*out = (const uint8_t *)text;
	*out_len = strlen(text);
}

static int content_entry_ref(char source, const char *name, char *out, size_t out_len)
{
	int written;

	written = snprintf(out, out_len, "content:books/%c/%s", source, name);
	return written > 0 && (size_t)written < out_len ? 0 : -ENAMETOOLONG;
}

static int content_emit_entry(struct content_scan *scan, char source, const char *name,
			      int32_t size)
{
	size_t index;
	size_t name_cap;
	int result;

	if (scan == NULL || scan->runtime == NULL || scan->out == NULL || scan->out_count == NULL) {
		return -EINVAL;
	}
	scan->total++;
	if (scan->total <= scan->offset || *scan->out_count >= scan->out_cap ||
	    *scan->out_count >= (size_t)scan->limit ||
	    *scan->out_count >= SQ_VM_RUNTIME_CONTENT_LIST_MAX) {
		return 0;
	}
	index = *scan->out_count;
	name_cap = sizeof(scan->runtime->content_binbook_names[index]);
	strncpy(scan->runtime->content_binbook_names[index], name, name_cap - 1U);
	scan->runtime->content_binbook_names[index][name_cap - 1U] = '\0';
	result = content_entry_ref(source, name, scan->runtime->content_binbook_refs[index],
				   sizeof(scan->runtime->content_binbook_refs[index]));
	if (result != 0) {
		return result;
	}
	scan->runtime->content_binbook_entries[index] = (SqvmContentBinBookEntry){
		.name = (const uint8_t *)scan->runtime->content_binbook_names[index],
		.name_len = strlen(scan->runtime->content_binbook_names[index]),
		.reference = (const uint8_t *)scan->runtime->content_binbook_refs[index],
		.reference_len = strlen(scan->runtime->content_binbook_refs[index]),
		.size = size,
	};
	scan->out[index] = scan->runtime->content_binbook_entries[index];
	(*scan->out_count)++;
	return 0;
}

static int content_scan_dir(struct content_scan *scan, const char *dir_path, char source)
{
	struct fs_dir_t dir;
	struct fs_dirent entry;
	int result;

	fs_dir_t_init(&dir);
	result = fs_opendir(&dir, dir_path);
	if (result != 0) {
		return result;
	}
	while (true) {
		result = fs_readdir(&dir, &entry);
		if (result != 0 || entry.name[0] == '\0') {
			break;
		}
		if (!content_name_safe(entry.name)) {
			continue;
		}
		if (entry.type != FS_DIR_ENTRY_FILE) {
			char path[SQ_APP_STORE_PATH_MAX];
			int written = snprintf(path, sizeof(path), "%s/%s", dir_path, entry.name);

			if (written <= 0 || (size_t)written >= sizeof(path) ||
			    fs_stat(path, &entry) != 0 || entry.type != FS_DIR_ENTRY_FILE) {
				continue;
			}
		}
		if (entry.size > INT32_MAX) {
			continue;
		}
		result = content_emit_entry(scan, source, entry.name, (int32_t)entry.size);
		if (result != 0) {
			break;
		}
	}
	(void)fs_closedir(&dir);
	return result;
}

static int content_scan_package_books(struct content_scan *scan)
{
	char path[SQ_APP_STORE_PATH_MAX];
	int result;

	if (scan == NULL || scan->runtime == NULL || scan->runtime->store_mount_point == NULL ||
	    scan->runtime->current_app[0] == '\0') {
		return -ENOENT;
	}
	result = sq_app_store_resource_path(scan->runtime->store_mount_point,
					    scan->runtime->current_app, "books", path,
					    sizeof(path));
	if (result != 0) {
		return result;
	}
	return content_scan_dir(scan, path, 'p');
}

static int content_prepare_sd_books(void)
{
	int result = fs_mkdir(SQ_VM_RUNTIME_CONTENT_BOOKS_DIR);

	return result == -EEXIST ? 0 : result;
}

int32_t runtime_content_binbook_list(void *user_data, const uint8_t *library,
				     size_t library_len, int32_t offset, int32_t limit,
				     SqvmContentBinBookEntry *out, size_t out_cap,
				     size_t *out_count,
				     SqvmContentBinBookListResult *out_result)
{
	struct sq_vm_runtime *runtime = user_data;
	struct content_scan scan;
	bool package_listed = false;
	bool sd_listed = false;
	bool sd_warning = false;
	int result;

	if (runtime == NULL || library == NULL || out == NULL || out_count == NULL ||
	    out_result == NULL) {
		return -EINVAL;
	}
	*out_count = 0;
	sqvm_content_binbook_list_result_unsupported(out_result);
	if (library_len != sizeof("books") - 1U || memcmp(library, "books", library_len) != 0) {
		content_set_text("not found", &out_result->error, &out_result->error_len);
		return 0;
	}
	if (offset < 0) {
		offset = 0;
	}
	if (limit < 0 || limit > SQ_VM_RUNTIME_CONTENT_LIST_MAX) {
		limit = SQ_VM_RUNTIME_CONTENT_LIST_MAX;
	}
	if ((size_t)limit > out_cap) {
		limit = (int32_t)out_cap;
	}
	memset(runtime->content_binbook_entries, 0, sizeof(runtime->content_binbook_entries));
	memset(runtime->content_binbook_names, 0, sizeof(runtime->content_binbook_names));
	memset(runtime->content_binbook_refs, 0, sizeof(runtime->content_binbook_refs));
	scan = (struct content_scan){
		.runtime = runtime,
		.out = out,
		.out_cap = out_cap,
		.out_count = out_count,
		.offset = offset,
		.limit = limit,
		.total = 0,
	};
	result = content_scan_package_books(&scan);
	package_listed = result == 0;
	result = content_prepare_sd_books();
	if (result == 0) {
		result = content_scan_dir(&scan, SQ_VM_RUNTIME_CONTENT_BOOKS_DIR, 'r');
	}
	if (result == 0) {
		sd_listed = true;
	} else {
		sd_warning = true;
	}
	out_result->ok = package_listed || sd_listed;
	content_set_text(out_result->ok ? NULL : "unavailable", &out_result->error,
			 &out_result->error_len);
	content_set_text(sd_warning ? "sd-unavailable" : NULL, &out_result->warning,
			 &out_result->warning_len);
	out_result->count = scan.total;
	out_result->has_more = scan.total > offset + (int32_t)*out_count;
	return 0;
}

static bool content_ref_suffix_safe(const uint8_t *suffix, size_t suffix_len)
{
	if (suffix == NULL || suffix_len == 0 || suffix_len >= SQ_VM_RUNTIME_CONTENT_NAME_LEN ||
	    suffix[0] == '.') {
		return false;
	}
	for (size_t i = 0; i < suffix_len; ++i) {
		if (suffix[i] == '/' || suffix[i] == '\\' || suffix[i] == '\0') {
			return false;
		}
	}
	return suffix_len > sizeof(CONTENT_BINBOOK_EXT) - 1U &&
	       memcmp(&suffix[suffix_len - (sizeof(CONTENT_BINBOOK_EXT) - 1U)],
		      CONTENT_BINBOOK_EXT, sizeof(CONTENT_BINBOOK_EXT) - 1U) == 0;
}

int runtime_content_resolve_binbook_ref(struct sq_vm_runtime *runtime, const uint8_t *ref,
					size_t ref_len, char *out, size_t out_len)
{
	const uint8_t *suffix;
	size_t suffix_len;
	char resource[SQ_VM_RUNTIME_CONTENT_NAME_LEN + sizeof("books/")];
	int written;

	if (runtime == NULL || ref == NULL || out == NULL) {
		return -EINVAL;
	}
	if (ref_len > sizeof(CONTENT_PACKAGE_PREFIX) - 1U &&
	    memcmp(ref, CONTENT_PACKAGE_PREFIX, sizeof(CONTENT_PACKAGE_PREFIX) - 1U) == 0) {
		suffix = ref + sizeof(CONTENT_PACKAGE_PREFIX) - 1U;
		suffix_len = ref_len - (sizeof(CONTENT_PACKAGE_PREFIX) - 1U);
		if (!content_ref_suffix_safe(suffix, suffix_len) ||
		    runtime->store_mount_point == NULL || runtime->current_app[0] == '\0') {
			return -EINVAL;
		}
		written = snprintf(resource, sizeof(resource), "books/%.*s", (int)suffix_len,
				   (const char *)suffix);
		if (written <= 0 || (size_t)written >= sizeof(resource)) {
			return -ENAMETOOLONG;
		}
		return sq_app_store_resource_path(runtime->store_mount_point, runtime->current_app,
						  resource, out, out_len);
	}
	if (ref_len > sizeof(CONTENT_REMOVABLE_PREFIX) - 1U &&
	    memcmp(ref, CONTENT_REMOVABLE_PREFIX, sizeof(CONTENT_REMOVABLE_PREFIX) - 1U) == 0) {
		suffix = ref + sizeof(CONTENT_REMOVABLE_PREFIX) - 1U;
		suffix_len = ref_len - (sizeof(CONTENT_REMOVABLE_PREFIX) - 1U);
		if (!content_ref_suffix_safe(suffix, suffix_len)) {
			return -EINVAL;
		}
		written = snprintf(out, out_len, SQ_VM_RUNTIME_CONTENT_BOOKS_DIR "/%.*s",
				   (int)suffix_len, (const char *)suffix);
		return written > 0 && (size_t)written < out_len ? 0 : -ENAMETOOLONG;
	}
	return -ENOENT;
}
