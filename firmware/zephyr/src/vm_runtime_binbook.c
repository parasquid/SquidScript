#include "vm_runtime_internal.h"

#include "app_store.h"
#include "sq_errno.h"

#define BINBOOK_HEADER_SIZE 256U
#define BINBOOK_SECTION_ENTRY_SIZE 40U
#define BINBOOK_PAGE_INDEX_ENTRY_SIZE 76U
#define BINBOOK_NAV_INDEX_ENTRY_SIZE 48U
#define BINBOOK_CHAPTER_INDEX_ENTRY_SIZE 32U
#define BINBOOK_MAGIC "BINBOOK"
#define BINBOOK_SECTION_STRING_TABLE 1U
#define BINBOOK_SECTION_PAGE_INDEX 40U
#define BINBOOK_SECTION_NAV_INDEX 41U
#define BINBOOK_SECTION_CHAPTER_INDEX 43U
#define BINBOOK_SECTION_PAGE_DATA 50U
#define BINBOOK_PIXEL_FORMAT_GRAY2_PACKED 2U
#define BINBOOK_COMPRESSION_RLE_PACKBITS 1U
#define BINBOOK_HANDLE_ID 1U
#define BINBOOK_DRAWABLE_ID 1U
#define BINBOOK_NAV_TYPE_CHAPTER 3U
#define BINBOOK_NAV_TYPE_SECTION 4U

struct binbook_header_view {
	uint64_t file_size;
	uint64_t section_table_offset;
	uint32_t section_table_length;
	uint16_t section_table_entry_size;
	uint16_t section_count;
	uint16_t page_index_entry_size;
	uint16_t nav_index_entry_size;
	uint64_t page_data_offset;
	uint64_t page_data_length;
};

struct binbook_section_view {
	uint16_t section_id;
	uint64_t offset;
	uint64_t length;
	uint32_t entry_size;
	uint32_t record_count;
};

static uint64_t binbook_open_us_acc;
static uint64_t binbook_read_page_us_acc;

uint64_t sq_vm_runtime_binbook_drain_open_us(void)
{
	uint64_t us = binbook_open_us_acc;

	binbook_open_us_acc = 0;
	return us;
}

uint64_t sq_vm_runtime_binbook_drain_read_page_us(void)
{
	uint64_t us = binbook_read_page_us_acc;

	binbook_read_page_us_acc = 0;
	return us;
}

static struct {
	struct fs_file_t file;
	bool is_open;
	char path[SQ_APP_STORE_PATH_MAX];
} binbook_open_file;

static size_t binbook_file_open_count;

size_t test_binbook_open_count(void)
{
	return binbook_file_open_count;
}

static void binbook_open_file_close(void)
{
	if (binbook_open_file.is_open) {
		(void)fs_close(&binbook_open_file.file);
		binbook_open_file.is_open = false;
	}
	binbook_open_file.path[0] = '\0';
}

int sq_vm_runtime_binbook_release(void)
{
	binbook_open_file_close();
	return 0;
}

static uint16_t read_le16(const uint8_t *bytes)
{
	return (uint16_t)bytes[0] | ((uint16_t)bytes[1] << 8);
}

static uint32_t read_le32(const uint8_t *bytes)
{
	return (uint32_t)bytes[0] | ((uint32_t)bytes[1] << 8) | ((uint32_t)bytes[2] << 16) |
	       ((uint32_t)bytes[3] << 24);
}

static uint64_t read_le64(const uint8_t *bytes)
{
	return (uint64_t)read_le32(bytes) | ((uint64_t)read_le32(bytes + 4) << 32);
}

static void binbook_set_error(const char *error, const uint8_t **out, size_t *out_len)
{
	if (error == NULL) {
		*out = NULL;
		*out_len = 0;
		return;
	}
	*out = (const uint8_t *)error;
	*out_len = strlen(error);
}

static int binbook_read_exact(struct fs_file_t *file, uint64_t offset, uint8_t *out, size_t len)
{
	if (file == NULL || out == NULL) {
		return -EINVAL;
	}
	int result = fs_seek(file, (off_t)offset, FS_SEEK_SET);

	if (result != 0) {
		return result;
	}
	ssize_t read = fs_read(file, out, len);

	if (read < 0) {
		return (int)read;
	}
	return (size_t)read == len ? 0 : -EIO;
}

static int binbook_read_header(struct fs_file_t *file, struct binbook_header_view *out)
{
	uint8_t header[BINBOOK_HEADER_SIZE];
	int result;

	if (out == NULL) {
		return -EINVAL;
	}
	result = binbook_read_exact(file, 0, header, sizeof(header));
	if (result != 0) {
		return result;
	}
	if (memcmp(header, BINBOOK_MAGIC, sizeof(BINBOOK_MAGIC)) != 0 || header[7] != '\0') {
		return -EINVAL;
	}
	if (read_le16(&header[12]) != BINBOOK_HEADER_SIZE) {
		return -ENOTSUP;
	}
	out->file_size = read_le64(&header[16]);
	out->section_table_offset = read_le64(&header[24]);
	out->section_table_length = read_le32(&header[32]);
	out->section_table_entry_size = read_le16(&header[36]);
	out->section_count = read_le16(&header[38]);
	out->page_index_entry_size = read_le16(&header[40]);
	out->nav_index_entry_size = read_le16(&header[42]);
	out->page_data_offset = read_le64(&header[44]);
	out->page_data_length = read_le64(&header[52]);
	if (out->section_table_entry_size != BINBOOK_SECTION_ENTRY_SIZE ||
	    out->page_index_entry_size != BINBOOK_PAGE_INDEX_ENTRY_SIZE ||
	    out->nav_index_entry_size != BINBOOK_NAV_INDEX_ENTRY_SIZE ||
	    out->section_table_offset < BINBOOK_HEADER_SIZE || out->section_count == 0 ||
	    out->section_table_length <
		    (uint32_t)out->section_count * BINBOOK_SECTION_ENTRY_SIZE) {
		return -ENOTSUP;
	}
	return 0;
}

static void binbook_section_from_bytes(const uint8_t bytes[BINBOOK_SECTION_ENTRY_SIZE],
				       struct binbook_section_view *out)
{
	out->section_id = read_le16(&bytes[0]);
	out->offset = read_le64(&bytes[4]);
	out->length = read_le64(&bytes[12]);
	out->entry_size = read_le32(&bytes[20]);
	out->record_count = read_le32(&bytes[24]);
}

static int binbook_find_sections(struct fs_file_t *file, const struct binbook_header_view *header,
				 struct binbook_section_view *string_table,
				 struct binbook_section_view *page_index,
				 struct binbook_section_view *nav_index,
				 struct binbook_section_view *chapter_index,
				 struct binbook_section_view *page_data)
{
	uint8_t bytes[BINBOOK_SECTION_ENTRY_SIZE];

	memset(string_table, 0, sizeof(*string_table));
	memset(page_index, 0, sizeof(*page_index));
	memset(nav_index, 0, sizeof(*nav_index));
	memset(chapter_index, 0, sizeof(*chapter_index));
	memset(page_data, 0, sizeof(*page_data));
	for (uint16_t i = 0; i < header->section_count; ++i) {
		struct binbook_section_view section;
		uint64_t offset = header->section_table_offset +
				  (uint64_t)i * BINBOOK_SECTION_ENTRY_SIZE;
		int result = binbook_read_exact(file, offset, bytes, sizeof(bytes));

		if (result != 0) {
			return result;
		}
		binbook_section_from_bytes(bytes, &section);
		if (section.section_id == BINBOOK_SECTION_STRING_TABLE) {
			*string_table = section;
		} else if (section.section_id == BINBOOK_SECTION_PAGE_INDEX) {
			*page_index = section;
		} else if (section.section_id == BINBOOK_SECTION_NAV_INDEX) {
			*nav_index = section;
		} else if (section.section_id == BINBOOK_SECTION_CHAPTER_INDEX) {
			*chapter_index = section;
		} else if (section.section_id == BINBOOK_SECTION_PAGE_DATA) {
			*page_data = section;
		}
	}
	if (string_table->section_id != BINBOOK_SECTION_STRING_TABLE ||
	    page_index->section_id != BINBOOK_SECTION_PAGE_INDEX ||
	    nav_index->section_id != BINBOOK_SECTION_NAV_INDEX ||
	    chapter_index->section_id != BINBOOK_SECTION_CHAPTER_INDEX ||
	    page_data->section_id != BINBOOK_SECTION_PAGE_DATA ||
	    page_index->entry_size != BINBOOK_PAGE_INDEX_ENTRY_SIZE ||
	    nav_index->entry_size != BINBOOK_NAV_INDEX_ENTRY_SIZE ||
	    chapter_index->entry_size != BINBOOK_CHAPTER_INDEX_ENTRY_SIZE ||
	    page_index->record_count == 0 || page_data->offset != header->page_data_offset ||
	    page_data->length != header->page_data_length) {
		return -EINVAL;
	}
	return 0;
}

static int binbook_open_resource(struct sq_vm_runtime *runtime, const uint8_t *path,
				 size_t path_len, struct fs_file_t *file, char *resolved_path,
				 size_t resolved_path_len, const char *already_open_path,
				 bool *out_reused)
{
	if (runtime == NULL || path == NULL || path_len == 0 || file == NULL ||
	    resolved_path == NULL || runtime->store_mount_point == NULL ||
	    runtime->current_app[0] == '\0') {
		return -EINVAL;
	}
	if (out_reused != NULL) {
		*out_reused = false;
	}
	int result = runtime_content_resolve_binbook_ref(runtime, path, path_len, resolved_path,
							 resolved_path_len);
	if (result == -ENOENT) {
		result = sq_app_store_resource_path_bytes(runtime->store_mount_point,
							  runtime->current_app, path, path_len,
							  resolved_path, resolved_path_len);
	}
	if (result != 0) {
		return result;
	}
	if (already_open_path != NULL && already_open_path[0] != '\0' &&
	    strncmp(resolved_path, already_open_path, SQ_APP_STORE_PATH_MAX) == 0) {
		if (out_reused != NULL) {
			*out_reused = true;
		}
		return 0;
	}
	fs_file_t_init(file);
	return fs_open(file, resolved_path, FS_O_READ);
}

static int runtime_binbook_validate_path_details(const char *path,
						 struct binbook_header_view *out_header,
						 struct binbook_section_view *out_string_table,
						 struct binbook_section_view *out_page_index,
						 struct binbook_section_view *out_nav_index,
						 struct binbook_section_view *out_chapter_index,
						 struct binbook_section_view *out_page_data)
{
	struct binbook_header_view header;
	struct binbook_section_view string_table;
	struct binbook_section_view page_index;
	struct binbook_section_view nav_index;
	struct binbook_section_view chapter_index;
	struct binbook_section_view page_data;
	struct fs_dirent entry;
	struct fs_file_t file;
	int result;

	if (path == NULL) {
		return -EINVAL;
	}
	fs_file_t_init(&file);
	result = fs_open(&file, path, FS_O_READ);
	if (result != 0) {
		return result;
	}
	result = fs_stat(path, &entry);
	if (result == 0) {
		result = binbook_read_header(&file, &header);
	}
	if (result == 0 && header.file_size != 0 && entry.size < header.file_size) {
		result = -EIO;
	}
	if (result == 0) {
		result = binbook_find_sections(&file, &header, &string_table, &page_index,
					       &nav_index, &chapter_index, &page_data);
	}
	(void)fs_close(&file);
	if (result == 0) {
		if (out_header != NULL) {
			*out_header = header;
		}
		if (out_string_table != NULL) {
			*out_string_table = string_table;
		}
		if (out_page_index != NULL) {
			*out_page_index = page_index;
		}
		if (out_nav_index != NULL) {
			*out_nav_index = nav_index;
		}
		if (out_chapter_index != NULL) {
			*out_chapter_index = chapter_index;
		}
		if (out_page_data != NULL) {
			*out_page_data = page_data;
		}
	}
	return result;
}

int runtime_binbook_validate_path(const char *path)
{
	return runtime_binbook_validate_path_details(path, NULL, NULL, NULL, NULL, NULL, NULL);
}

int32_t runtime_binbook_open(void *user_data, const uint8_t *path, size_t path_len,
			     SqvmBinBookOpenResult *out)
{
	struct sq_vm_runtime *runtime = user_data;
	struct binbook_section_view string_table;
	struct binbook_section_view page_index;
	struct binbook_section_view nav_index;
	struct binbook_section_view chapter_index;
	struct binbook_section_view page_data;
	char resolved_path[SQ_APP_STORE_PATH_MAX];
	int result;
	uint64_t t0 = k_cycle_get_64();

	if (out == NULL) {
		return -EINVAL;
	}
	sqvm_binbook_open_result_unsupported(out);
	bool reused = false;

	result = binbook_open_resource(runtime, path, path_len, &binbook_open_file.file,
				       resolved_path, sizeof(resolved_path),
				       binbook_open_file.path, &reused);
	if (result != 0) {
		char line[SQ_VM_RUNTIME_DEVICE_ERROR_LEN];

		(void)snprintf(line, sizeof(line), "binbook.open code=%d (%s)", result,
			       sq_errno_name(result));
		(void)sq_vm_runtime_record_device_error(runtime, line);
		binbook_set_error("open failed", &out->error, &out->error_len);
		binbook_open_us_acc += k_cyc_to_us_floor64(k_cycle_get_64() - t0);
		return 0;
	}
	if (!reused) {
		binbook_open_file.is_open = true;
		strncpy(binbook_open_file.path, resolved_path,
			sizeof(binbook_open_file.path) - 1);
		binbook_file_open_count++;
	}
	result = runtime_binbook_validate_path_details(resolved_path, NULL, &string_table,
						      &page_index, &nav_index, &chapter_index,
						      &page_data);
	if (result != 0) {
		char line[SQ_VM_RUNTIME_DEVICE_ERROR_LEN];

		(void)snprintf(line, sizeof(line), "binbook.validate code=%d (%s)", result,
			       sq_errno_name(result));
		(void)sq_vm_runtime_record_device_error(runtime, line);
		binbook_set_error("invalid binbook", &out->error, &out->error_len);
		binbook_open_file_close();
		binbook_open_us_acc += k_cyc_to_us_floor64(k_cycle_get_64() - t0);
		return 0;
	}
	memset(&runtime->binbook, 0, sizeof(runtime->binbook));
	runtime->binbook.active = true;
	strncpy(runtime->binbook.path, resolved_path, sizeof(runtime->binbook.path) - 1);
	runtime->binbook.string_table_offset = string_table.offset;
	runtime->binbook.page_index_offset = page_index.offset;
	runtime->binbook.nav_index_offset = nav_index.offset;
	runtime->binbook.chapter_index_offset = chapter_index.offset;
	runtime->binbook.page_data_offset = page_data.offset;
	runtime->binbook.string_table_length = (uint32_t)string_table.length;
	runtime->binbook.page_index_entry_size = (uint16_t)page_index.entry_size;
	runtime->binbook.nav_index_entry_size = (uint16_t)nav_index.entry_size;
	runtime->binbook.chapter_index_entry_size = (uint16_t)chapter_index.entry_size;
	runtime->binbook.page_count = page_index.record_count;
	runtime->binbook.nav_count = nav_index.record_count;
	runtime->binbook.chapter_count = chapter_index.record_count;
	memset(&runtime->drawable, 0, sizeof(runtime->drawable));
	binbook_open_us_acc += k_cyc_to_us_floor64(k_cycle_get_64() - t0);
	out->ok = true;
	binbook_set_error(NULL, &out->error, &out->error_len);
	out->book = (SqvmHandle){
		.kind = SQVM_HANDLE_BINBOOK,
		.id = BINBOOK_HANDLE_ID,
	};
	return 0;
}

int32_t runtime_binbook_info(void *user_data, SqvmHandle book, SqvmBinBookInfoResult *out)
{
	struct sq_vm_runtime *runtime = user_data;

	if (out == NULL) {
		return -EINVAL;
	}
	sqvm_binbook_info_result_unsupported(out);
	if (runtime == NULL || book.kind != SQVM_HANDLE_BINBOOK || book.id != BINBOOK_HANDLE_ID ||
	    !runtime->binbook.active) {
		binbook_set_error("invalid book", &out->error, &out->error_len);
		return 0;
	}
	out->ok = true;
	binbook_set_error(NULL, &out->error, &out->error_len);
	out->title = NULL;
	out->title_len = 0;
	out->page_count = (int32_t)runtime->binbook.page_count;
	out->chapter_count = (int32_t)runtime->binbook.chapter_count;
	return 0;
}

static int binbook_read_title(struct fs_file_t *file, const struct sq_vm_runtime_binbook_handle *book,
			      uint32_t title_offset, uint32_t title_len, char *out, size_t out_cap)
{
	size_t copy_len;
	int result;

	if (file == NULL || book == NULL || out == NULL || out_cap == 0U) {
		return -EINVAL;
	}
	out[0] = '\0';
	if (title_len == 0U) {
		return 0;
	}
	if ((uint64_t)title_offset + title_len > book->string_table_length) {
		return -EINVAL;
	}
	copy_len = title_len;
	if (copy_len >= out_cap) {
		copy_len = out_cap - 1U;
	}
	result = binbook_read_exact(file, book->string_table_offset + title_offset, (uint8_t *)out,
				    copy_len);
	if (result != 0) {
		return result;
	}
	out[copy_len] = '\0';
	return 0;
}

static int binbook_read_chapter_entry(struct fs_file_t *file,
				      const struct sq_vm_runtime_binbook_handle *book,
				      uint32_t index, SqvmBinBookChapterEntry *entry, char *title,
				      size_t title_cap)
{
	uint8_t bytes[BINBOOK_CHAPTER_INDEX_ENTRY_SIZE];
	uint32_t chapter_index;
	uint32_t title_offset;
	uint32_t title_len;
	uint16_t level;
	uint16_t nav_type;
	uint32_t page_index;
	int result;

	if (file == NULL || book == NULL || entry == NULL || title == NULL ||
	    index >= book->chapter_count) {
		return -EINVAL;
	}
	result = binbook_read_exact(file,
				    book->chapter_index_offset +
					    (uint64_t)index * book->chapter_index_entry_size,
				    bytes, sizeof(bytes));
	if (result != 0) {
		return result;
	}
	chapter_index = read_le32(&bytes[0]);
	title_offset = read_le32(&bytes[8]);
	title_len = read_le32(&bytes[12]);
	page_index = read_le32(&bytes[16]);
	level = read_le16(&bytes[20]);
	nav_type = read_le16(&bytes[22]);
	if (chapter_index != index || page_index >= book->page_count ||
	    (nav_type != BINBOOK_NAV_TYPE_CHAPTER && nav_type != BINBOOK_NAV_TYPE_SECTION)) {
		return -EINVAL;
	}
	result = binbook_read_title(file, book, title_offset, title_len, title, title_cap);
	if (result != 0) {
		return result;
	}
	*entry = (SqvmBinBookChapterEntry){
		.index = (int32_t)chapter_index,
		.title = (const uint8_t *)title,
		.title_len = strlen(title),
		.page_index = (int32_t)page_index,
		.level = (int32_t)level,
		.entry_type = (int32_t)nav_type,
	};
	return 0;
}

int32_t runtime_binbook_chapters(void *user_data, SqvmHandle book, int32_t offset, int32_t limit,
				 SqvmBinBookChapterEntry *out, size_t out_cap, size_t *out_count,
				 SqvmBinBookChapterListResult *out_result)
{
	struct sq_vm_runtime *runtime = user_data;
	size_t emitted = 0;
	uint32_t start;
	uint32_t capped_limit;
	int result;

	if (out_result == NULL || out_count == NULL) {
		return -EINVAL;
	}
	sqvm_binbook_chapter_list_result_unsupported(out_result);
	*out_count = 0;
	if (runtime == NULL || book.kind != SQVM_HANDLE_BINBOOK || book.id != BINBOOK_HANDLE_ID ||
	    !runtime->binbook.active || offset < 0 || limit < 0 || out == NULL) {
		binbook_set_error("invalid chapters", &out_result->error, &out_result->error_len);
		return 0;
	}
	if (!binbook_open_file.is_open) {
		binbook_set_error("no open book", &out_result->error, &out_result->error_len);
		return 0;
	}
	memset(runtime->binbook_chapter_entries, 0, sizeof(runtime->binbook_chapter_entries));
	memset(runtime->binbook_chapter_titles, 0, sizeof(runtime->binbook_chapter_titles));
	start = (uint32_t)offset;
	capped_limit = (uint32_t)limit;
	for (uint32_t index = start;
	     index < runtime->binbook.chapter_count && emitted < out_cap &&
	     emitted < SQ_VM_RUNTIME_CONTENT_LIST_MAX && emitted < capped_limit;
	     ++index) {
		result = binbook_read_chapter_entry(
			&binbook_open_file.file, &runtime->binbook, index,
			&runtime->binbook_chapter_entries[emitted],
			runtime->binbook_chapter_titles[emitted],
			sizeof(runtime->binbook_chapter_titles[emitted]));
		if (result != 0) {
			break;
		}
		out[emitted] = runtime->binbook_chapter_entries[emitted];
		emitted++;
	}
	if (result != 0) {
		binbook_set_error("read failed", &out_result->error, &out_result->error_len);
		return 0;
	}
	out_result->ok = true;
	binbook_set_error(NULL, &out_result->error, &out_result->error_len);
	out_result->count = (int32_t)runtime->binbook.chapter_count;
	out_result->has_more = (uint32_t)offset + (uint32_t)emitted < runtime->binbook.chapter_count;
	*out_count = emitted;
	return 0;
}

int32_t runtime_binbook_chapter(void *user_data, SqvmHandle book, int32_t index,
				SqvmBinBookChapterResult *out)
{
	struct sq_vm_runtime *runtime = user_data;
	int result;

	if (out == NULL) {
		return -EINVAL;
	}
	sqvm_binbook_chapter_result_unsupported(out);
	if (runtime == NULL || book.kind != SQVM_HANDLE_BINBOOK || book.id != BINBOOK_HANDLE_ID ||
	    !runtime->binbook.active || index < 0 ||
	    (uint32_t)index >= runtime->binbook.chapter_count) {
		binbook_set_error("invalid chapter", &out->error, &out->error_len);
		return 0;
	}
	if (!binbook_open_file.is_open) {
		binbook_set_error("no open book", &out->error, &out->error_len);
		return 0;
	}
	memset(&runtime->binbook_chapter_entries[0], 0, sizeof(runtime->binbook_chapter_entries[0]));
	memset(runtime->binbook_chapter_titles[0], 0, sizeof(runtime->binbook_chapter_titles[0]));
	result = binbook_read_chapter_entry(&binbook_open_file.file, &runtime->binbook,
					    (uint32_t)index,
					    &runtime->binbook_chapter_entries[0],
					    runtime->binbook_chapter_titles[0],
					    sizeof(runtime->binbook_chapter_titles[0]));
	if (result != 0) {
		binbook_set_error("read failed", &out->error, &out->error_len);
		return 0;
	}
	out->ok = true;
	binbook_set_error(NULL, &out->error, &out->error_len);
	out->chapter = runtime->binbook_chapter_entries[0];
	return 0;
}

int32_t runtime_binbook_read_page(void *user_data, SqvmHandle book, int32_t page_index,
				  SqvmBinBookReadPageResult *out)
{
	struct sq_vm_runtime *runtime = user_data;
	uint8_t bytes[BINBOOK_PAGE_INDEX_ENTRY_SIZE];
	struct rust_binbook_page_meta meta;
	int result;
	uint64_t t0 = k_cycle_get_64();

	if (out == NULL) {
		return -EINVAL;
	}
	sqvm_binbook_read_page_result_unsupported(out);
	if (runtime == NULL || book.kind != SQVM_HANDLE_BINBOOK || book.id != BINBOOK_HANDLE_ID ||
	    !runtime->binbook.active || page_index < 0 ||
	    (uint32_t)page_index >= runtime->binbook.page_count) {
		binbook_set_error("invalid page", &out->error, &out->error_len);
		binbook_read_page_us_acc += k_cyc_to_us_floor64(k_cycle_get_64() - t0);
		return 0;
	}
	if (!binbook_open_file.is_open) {
		binbook_set_error("no open book", &out->error, &out->error_len);
		binbook_read_page_us_acc += k_cyc_to_us_floor64(k_cycle_get_64() - t0);
		return 0;
	}
	result = binbook_read_exact(&binbook_open_file.file,
				    runtime->binbook.page_index_offset +
					    (uint64_t)(uint32_t)page_index *
						    runtime->binbook.page_index_entry_size,
				    bytes, sizeof(bytes));
	if (result != 0) {
		binbook_set_error("read failed", &out->error, &out->error_len);
		binbook_read_page_us_acc += k_cyc_to_us_floor64(k_cycle_get_64() - t0);
		return 0;
	}
	if (rust_binbook_page_meta(bytes, sizeof(bytes),
				   0,
				   runtime->binbook.page_data_offset,
				   0, &meta) != 0 || !meta.ok) {
		binbook_set_error("parse failed", &out->error, &out->error_len);
		binbook_read_page_us_acc += k_cyc_to_us_floor64(k_cycle_get_64() - t0);
		return 0;
	}
	memset(&runtime->drawable, 0, sizeof(runtime->drawable));
	runtime->drawable.active = true;
	strncpy(runtime->drawable.page.path, runtime->binbook.path,
		sizeof(runtime->drawable.page.path) - 1);
	runtime->drawable.page.page_index = (uint32_t)page_index;
	runtime->drawable.page.pixel_format = meta.pixel_format;
	runtime->drawable.page.compression_method = meta.compression_method;
	runtime->drawable.page.blob_offset = meta.blob_offset;
	runtime->drawable.page.compressed_size = meta.compressed_size;
	runtime->drawable.page.uncompressed_size = meta.uncompressed_size;
	runtime->drawable.page.stored_width = meta.stored_width;
	runtime->drawable.page.stored_height = meta.stored_height;
	binbook_read_page_us_acc += k_cyc_to_us_floor64(k_cycle_get_64() - t0);
	out->ok = true;
	binbook_set_error(NULL, &out->error, &out->error_len);
	out->drawable = (SqvmHandle){
		.kind = SQVM_HANDLE_DRAWABLE,
		.id = BINBOOK_DRAWABLE_ID,
	};
	return 0;
}
