#include "vm_runtime_internal.h"

int32_t runtime_system_memory_text(void *user_data, uint8_t *out, size_t out_cap,
					  size_t *out_len)
{
	ARG_UNUSED(user_data);
	size_t heap_free_bytes = 0;
	size_t heap_allocated_bytes = 0;

	if (out == NULL || out_len == NULL || out_cap == 0) {
		return -EINVAL;
	}

#ifdef CONFIG_SYS_HEAP_RUNTIME_STATS
	struct k_heap *heaps = NULL;
	int heap_array_count = k_heap_array_get(&heaps);
	if (heap_array_count > 0 && heaps != NULL) {
		for (int i = 0; i < heap_array_count; i++) {
			struct sys_memory_stats stats;

			if (sys_heap_runtime_stats_get(&heaps[i].heap, &stats) == 0) {
				heap_free_bytes += stats.free_bytes;
				heap_allocated_bytes += stats.allocated_bytes;
			}
		}
	}
#endif

	int written = snprintf((char *)out, out_cap, "RAM %u KiB heap %zu used %zu free",
			       (unsigned int)CONFIG_SRAM_SIZE, heap_allocated_bytes,
			       heap_free_bytes);
	if (written <= 0 || (size_t)written >= out_cap) {
		return -ENOSPC;
	}
	*out_len = (size_t)written;
	return 0;
}

static int write_human_bytes(uint8_t *out, size_t out_cap, size_t *out_len, const char *label,
			     uint64_t bytes)
{
	int written;

	if (bytes >= 1024u * 1024u) {
		written = snprintf((char *)out, out_cap, "%s %llu MiB", label,
				   (unsigned long long)(bytes / (1024u * 1024u)));
	} else if (bytes >= 1024u) {
		written = snprintf((char *)out, out_cap, "%s %llu KiB", label,
				   (unsigned long long)(bytes / 1024u));
	} else {
		written = snprintf((char *)out, out_cap, "%s %llu B", label,
				   (unsigned long long)bytes);
	}
	if (written <= 0 || (size_t)written >= out_cap) {
		return -ENOSPC;
	}
	*out_len = (size_t)written;
	return 0;
}

int32_t runtime_system_storage_text(void *user_data, const uint8_t *name, size_t name_len,
					   uint8_t *out, size_t out_cap, size_t *out_len)
{
	struct sq_vm_runtime *runtime = user_data;
	struct fs_statvfs stat;
	uint64_t free_bytes;

	if (runtime == NULL || name == NULL || out == NULL || out_len == NULL || out_cap == 0) {
		return -EINVAL;
	}
	if (name_len != 4 || memcmp(name, "apps", 4) != 0) {
		return -EINVAL;
	}
	if (runtime->store_mount_point == NULL) {
		return -ENODEV;
	}
	if (fs_statvfs(runtime->store_mount_point, &stat) != 0) {
		return -EIO;
	}
	free_bytes = (uint64_t)stat.f_bfree * (uint64_t)stat.f_frsize;
	return write_human_bytes(out, out_cap, out_len, "Apps", free_bytes);
}

int32_t runtime_system_start_reason_text(void *user_data, uint8_t *out, size_t out_cap,
						size_t *out_len)
{
	struct sq_vm_runtime *runtime = user_data;
	const char *reason;
	size_t reason_len;

	if (runtime == NULL || out == NULL || out_len == NULL || out_cap == 0) {
		return -EINVAL;
	}
	reason = runtime->start_reason[0] == '\0' ? "boot" : runtime->start_reason;
	reason_len = strlen(reason);
	if (reason_len >= out_cap) {
		return -ENOSPC;
	}
	memcpy(out, reason, reason_len);
	*out_len = reason_len;
	return 0;
}

