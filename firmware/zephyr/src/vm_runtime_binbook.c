#include "vm_runtime_internal.h"

#include "app_store.h"

#define BINBOOK_HEADER_SIZE 256U
#define BINBOOK_SECTION_ENTRY_SIZE 40U
#define BINBOOK_PAGE_INDEX_ENTRY_SIZE 76U
#define BINBOOK_MAGIC "BINBOOK"
#define BINBOOK_VERSION_MAJOR 0U
#define BINBOOK_SECTION_PAGE_INDEX 40U
#define BINBOOK_SECTION_PAGE_DATA 50U
#define BINBOOK_PIXEL_FORMAT_GRAY2_PACKED 2U
#define BINBOOK_COMPRESSION_RLE_PACKBITS 1U
#define BINBOOK_HANDLE_ID 1U
#define BINBOOK_DRAWABLE_ID 1U

struct binbook_header_view {
	uint64_t file_size;
	uint64_t section_table_offset;
	uint32_t section_table_length;
	uint16_t section_table_entry_size;
	uint16_t section_count;
	uint16_t page_index_entry_size;
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
	if (read_le16(&header[8]) != BINBOOK_VERSION_MAJOR ||
	    read_le16(&header[12]) != BINBOOK_HEADER_SIZE) {
		return -ENOTSUP;
	}
	out->file_size = read_le64(&header[16]);
	out->section_table_offset = read_le64(&header[24]);
	out->section_table_length = read_le32(&header[32]);
	out->section_table_entry_size = read_le16(&header[36]);
	out->section_count = read_le16(&header[38]);
	out->page_index_entry_size = read_le16(&header[40]);
	out->page_data_offset = read_le64(&header[44]);
	out->page_data_length = read_le64(&header[52]);
	if (out->section_table_entry_size != BINBOOK_SECTION_ENTRY_SIZE ||
	    out->page_index_entry_size != BINBOOK_PAGE_INDEX_ENTRY_SIZE ||
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
				 struct binbook_section_view *page_index,
				 struct binbook_section_view *page_data)
{
	uint8_t bytes[BINBOOK_SECTION_ENTRY_SIZE];

	memset(page_index, 0, sizeof(*page_index));
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
		if (section.section_id == BINBOOK_SECTION_PAGE_INDEX) {
			*page_index = section;
		} else if (section.section_id == BINBOOK_SECTION_PAGE_DATA) {
			*page_data = section;
		}
	}
	if (page_index->section_id != BINBOOK_SECTION_PAGE_INDEX ||
	    page_data->section_id != BINBOOK_SECTION_PAGE_DATA ||
	    page_index->entry_size != BINBOOK_PAGE_INDEX_ENTRY_SIZE ||
	    page_index->record_count == 0 || page_data->offset != header->page_data_offset ||
	    page_data->length != header->page_data_length) {
		return -EINVAL;
	}
	return 0;
}

static int binbook_open_resource(struct sq_vm_runtime *runtime, const uint8_t *path,
				 size_t path_len, struct fs_file_t *file, char *resolved_path,
				 size_t resolved_path_len)
{
	if (runtime == NULL || path == NULL || path_len == 0 || file == NULL ||
	    resolved_path == NULL || runtime->store_mount_point == NULL ||
	    runtime->current_app[0] == '\0') {
		return -EINVAL;
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
	fs_file_t_init(file);
	return fs_open(file, resolved_path, FS_O_READ);
}

static int runtime_binbook_validate_path_details(const char *path,
						 struct binbook_header_view *out_header,
						 struct binbook_section_view *out_page_index,
						 struct binbook_section_view *out_page_data)
{
	struct binbook_header_view header;
	struct binbook_section_view page_index;
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
		result = binbook_find_sections(&file, &header, &page_index, &page_data);
	}
	(void)fs_close(&file);
	if (result == 0) {
		if (out_header != NULL) {
			*out_header = header;
		}
		if (out_page_index != NULL) {
			*out_page_index = page_index;
		}
		if (out_page_data != NULL) {
			*out_page_data = page_data;
		}
	}
	return result;
}

int runtime_binbook_validate_path(const char *path)
{
	return runtime_binbook_validate_path_details(path, NULL, NULL, NULL);
}

int32_t runtime_binbook_open(void *user_data, const uint8_t *path, size_t path_len,
			     SqvmBinBookOpenResult *out)
{
	struct sq_vm_runtime *runtime = user_data;
	struct fs_file_t file;
	struct binbook_section_view page_index;
	struct binbook_section_view page_data;
	char resolved_path[SQ_APP_STORE_PATH_MAX];
	int result;

	if (out == NULL) {
		return -EINVAL;
	}
	sqvm_binbook_open_result_unsupported(out);
	result = binbook_open_resource(runtime, path, path_len, &file, resolved_path,
				       sizeof(resolved_path));
	if (result != 0) {
		binbook_set_error("open failed", &out->error, &out->error_len);
		return 0;
	}
	(void)fs_close(&file);
	result = runtime_binbook_validate_path_details(resolved_path, NULL, &page_index, &page_data);
	if (result != 0) {
		binbook_set_error("invalid binbook", &out->error, &out->error_len);
		return 0;
	}
	memset(&runtime->binbook, 0, sizeof(runtime->binbook));
	runtime->binbook.active = true;
	strncpy(runtime->binbook.path, resolved_path, sizeof(runtime->binbook.path) - 1);
	runtime->binbook.page_index_offset = page_index.offset;
	runtime->binbook.page_data_offset = page_data.offset;
	runtime->binbook.page_index_entry_size = (uint16_t)page_index.entry_size;
	runtime->binbook.page_count = page_index.record_count;
	memset(&runtime->drawable, 0, sizeof(runtime->drawable));
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
	return 0;
}

int32_t runtime_binbook_read_page(void *user_data, SqvmHandle book, int32_t page_index,
				  SqvmBinBookReadPageResult *out)
{
	struct sq_vm_runtime *runtime = user_data;
	struct fs_file_t file;
	uint8_t bytes[BINBOOK_PAGE_INDEX_ENTRY_SIZE];
	int result;

	if (out == NULL) {
		return -EINVAL;
	}
	sqvm_binbook_read_page_result_unsupported(out);
	if (runtime == NULL || book.kind != SQVM_HANDLE_BINBOOK || book.id != BINBOOK_HANDLE_ID ||
	    !runtime->binbook.active || page_index < 0 ||
	    (uint32_t)page_index >= runtime->binbook.page_count) {
		binbook_set_error("invalid page", &out->error, &out->error_len);
		return 0;
	}
	fs_file_t_init(&file);
	result = fs_open(&file, runtime->binbook.path, FS_O_READ);
	if (result != 0) {
		binbook_set_error("open failed", &out->error, &out->error_len);
		return 0;
	}
	result = binbook_read_exact(&file,
				    runtime->binbook.page_index_offset +
					    (uint64_t)(uint32_t)page_index *
						    runtime->binbook.page_index_entry_size,
				    bytes, sizeof(bytes));
	(void)fs_close(&file);
	if (result != 0) {
		binbook_set_error("read failed", &out->error, &out->error_len);
		return 0;
	}
	memset(&runtime->drawable, 0, sizeof(runtime->drawable));
	runtime->drawable.active = true;
	strncpy(runtime->drawable.page.path, runtime->binbook.path,
		sizeof(runtime->drawable.page.path) - 1);
	runtime->drawable.page.page_index = (uint32_t)page_index;
	runtime->drawable.page.pixel_format = read_le16(&bytes[6]);
	runtime->drawable.page.compression_method = read_le16(&bytes[8]);
	runtime->drawable.page.blob_offset = runtime->binbook.page_data_offset + read_le64(&bytes[16]);
	runtime->drawable.page.compressed_size = read_le32(&bytes[24]);
	runtime->drawable.page.uncompressed_size = read_le32(&bytes[28]);
	runtime->drawable.page.stored_width = read_le16(&bytes[36]);
	runtime->drawable.page.stored_height = read_le16(&bytes[38]);
	if (runtime->drawable.page.pixel_format != BINBOOK_PIXEL_FORMAT_GRAY2_PACKED ||
	    runtime->drawable.page.compression_method != BINBOOK_COMPRESSION_RLE_PACKBITS) {
		memset(&runtime->drawable, 0, sizeof(runtime->drawable));
		binbook_set_error("unsupported page", &out->error, &out->error_len);
		return 0;
	}
	out->ok = true;
	binbook_set_error(NULL, &out->error, &out->error_len);
	out->drawable = (SqvmHandle){
		.kind = SQVM_HANDLE_DRAWABLE,
		.id = BINBOOK_DRAWABLE_ID,
	};
	return 0;
}
