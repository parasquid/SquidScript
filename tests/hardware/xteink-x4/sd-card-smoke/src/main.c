/*
 * Diagnostic-only XTEINK X4 SPI SD-card smoke test.
 *
 * This app proves the target's SD wiring by initializing the card, reading one
 * sector, parsing the existing FAT boot sector, and listing root directory
 * entries. It is read-only and does not exercise SquidScript SD storage.
 */

#include <errno.h>
#include <stdbool.h>
#include <stdint.h>
#include <string.h>

#include <zephyr/device.h>
#include <zephyr/drivers/disk.h>
#include <zephyr/kernel.h>
#include <zephyr/sd/sd.h>
#include <zephyr/sd/sdmmc.h>

#define SD_SMOKE_READ_BLOCK 0U
#define SD_SMOKE_READ_BLOCKS 1U
#define SD_SMOKE_SECTOR_SIZE 512U
#define SD_SMOKE_MAX_ROOT_ENTRIES 16U
#define SD_SMOKE_MBR_PARTITION_OFFSET 446U
#define SD_SMOKE_MBR_PARTITION_ENTRY_SIZE 16U
#define SD_SMOKE_MBR_FIRST_PARTITION 0U
#define SD_SMOKE_MBR_TYPE_OFFSET 4U
#define SD_SMOKE_MBR_LBA_OFFSET 8U
#define SD_SMOKE_FAT_SIGNATURE_LOW 0x55U
#define SD_SMOKE_FAT_SIGNATURE_HIGH 0xaaU
#define SD_SMOKE_FAT_DIR_ENTRY_SIZE 32U
#define SD_SMOKE_FAT_ATTR_LONG_NAME 0x0fU
#define SD_SMOKE_FAT_ATTR_DIRECTORY 0x10U

struct fat_layout {
	uint32_t volume_lba;
	uint32_t first_root_sector;
	uint32_t root_dir_sectors;
	uint32_t first_data_sector;
	uint32_t root_cluster;
	uint32_t fat_size;
	uint32_t total_sectors;
	uint16_t bytes_per_sector;
	uint16_t root_entries;
	uint8_t sectors_per_cluster;
	uint8_t fat_count;
	bool fat32;
};

static const struct device *const sdhc_dev = DEVICE_DT_GET(DT_ALIAS(sdhc0));
static struct sd_card card;
static uint8_t block[SD_SMOKE_SECTOR_SIZE] __aligned(CONFIG_SDHC_BUFFER_ALIGNMENT);

static const char *errno_name(int code)
{
	const int err = code < 0 ? -code : code;

	switch (err) {
	case ENODEV:
		return "ENODEV";
	case EIO:
		return "EIO";
	case ENOENT:
		return "ENOENT";
	case EINVAL:
		return "EINVAL";
	case ENOTSUP:
		return "ENOTSUP";
	case ETIMEDOUT:
		return "ETIMEDOUT";
	case ENOSYS:
		return "ENOSYS";
	default:
		return "UNKNOWN";
	}
}

static void print_fail(const char *step, int code)
{
	printk("SD_CARD_SMOKE_FAIL step=%s code=%d (%s)\n",
	       step, code, errno_name(code));
}

static uint16_t le16(const uint8_t *data)
{
	return (uint16_t)data[0] | ((uint16_t)data[1] << 8);
}

static uint32_t le32(const uint8_t *data)
{
	return (uint32_t)data[0] |
	       ((uint32_t)data[1] << 8) |
	       ((uint32_t)data[2] << 16) |
	       ((uint32_t)data[3] << 24);
}

static bool has_boot_signature(const uint8_t *sector)
{
	return sector[510] == SD_SMOKE_FAT_SIGNATURE_LOW &&
	       sector[511] == SD_SMOKE_FAT_SIGNATURE_HIGH;
}

static int read_sector(uint32_t sector)
{
	const int ret = sdmmc_read_blocks(&card, block, sector, SD_SMOKE_READ_BLOCKS);

	if (ret != 0) {
		print_fail("read_sector", ret);
	}
	return ret;
}

static uint32_t detect_volume_lba(void)
{
	const uint32_t entry_offset = SD_SMOKE_MBR_PARTITION_OFFSET +
				     (SD_SMOKE_MBR_FIRST_PARTITION *
				      SD_SMOKE_MBR_PARTITION_ENTRY_SIZE);
	const uint8_t partition_type = block[entry_offset + SD_SMOKE_MBR_TYPE_OFFSET];
	const uint32_t partition_lba = le32(&block[entry_offset + SD_SMOKE_MBR_LBA_OFFSET]);

	if (has_boot_signature(block) && partition_type != 0U && partition_lba != 0U) {
		printk("SD_CARD_SMOKE_PARTITION type=0x%02x lba=%u\n",
		       partition_type, partition_lba);
		return partition_lba;
	}

	printk("SD_CARD_SMOKE_PARTITION type=raw lba=0\n");
	return 0;
}

static int parse_fat_layout(uint32_t volume_lba, struct fat_layout *layout)
{
	const uint16_t bytes_per_sector = le16(&block[11]);
	const uint8_t sectors_per_cluster = block[13];
	const uint16_t reserved_sectors = le16(&block[14]);
	const uint8_t fat_count = block[16];
	const uint16_t root_entries = le16(&block[17]);
	const uint16_t total16 = le16(&block[19]);
	const uint16_t fat16_size = le16(&block[22]);
	const uint32_t total32 = le32(&block[32]);
	const uint32_t fat32_size = le32(&block[36]);
	const uint32_t root_cluster = le32(&block[44]);
	const uint32_t total_sectors = total16 != 0U ? total16 : total32;
	const uint32_t fat_size = fat16_size != 0U ? fat16_size : fat32_size;
	const bool fat32 = root_entries == 0U;
	const uint32_t root_dir_sectors =
		((uint32_t)root_entries * SD_SMOKE_FAT_DIR_ENTRY_SIZE +
		 (uint32_t)bytes_per_sector - 1U) / (uint32_t)bytes_per_sector;
	const uint32_t first_data_sector =
		(uint32_t)reserved_sectors + ((uint32_t)fat_count * fat_size) +
		root_dir_sectors;

	if (!has_boot_signature(block)) {
		print_fail("fat_signature", -EINVAL);
		return -EINVAL;
	}
	if (bytes_per_sector != SD_SMOKE_SECTOR_SIZE ||
	    sectors_per_cluster == 0U ||
	    reserved_sectors == 0U ||
	    fat_count == 0U ||
	    total_sectors == 0U ||
	    fat_size == 0U ||
	    (fat32 && root_cluster < 2U)) {
		print_fail("fat_bpb", -EINVAL);
		return -EINVAL;
	}

	layout->volume_lba = volume_lba;
	layout->bytes_per_sector = bytes_per_sector;
	layout->sectors_per_cluster = sectors_per_cluster;
	layout->fat_count = fat_count;
	layout->root_entries = root_entries;
	layout->fat_size = fat_size;
	layout->total_sectors = total_sectors;
	layout->root_dir_sectors = root_dir_sectors;
	layout->first_data_sector = first_data_sector;
	layout->root_cluster = fat32 ? root_cluster : 0U;
	layout->fat32 = fat32;
	layout->first_root_sector = fat32 ?
		volume_lba + first_data_sector +
			((root_cluster - 2U) * (uint32_t)sectors_per_cluster) :
		volume_lba + (uint32_t)reserved_sectors +
			((uint32_t)fat_count * fat_size);

	printk("SD_CARD_SMOKE_FAT volume_lba=%u type=%s total_sectors=%u fat_size=%u cluster_sectors=%u root_sector=%u\n",
	       layout->volume_lba, layout->fat32 ? "FAT32" : "FAT12_16",
	       layout->total_sectors, layout->fat_size,
	       layout->sectors_per_cluster, layout->first_root_sector);
	return 0;
}

static void copy_fat_name(const uint8_t *entry, char *name, size_t name_len)
{
	size_t out = 0U;

	for (size_t i = 0U; i < 8U && out + 1U < name_len; i++) {
		if (entry[i] == ' ') {
			break;
		}
		name[out++] = (char)entry[i];
	}

	if (entry[8] != ' ' && out + 1U < name_len) {
		name[out++] = '.';
		for (size_t i = 8U; i < 11U && out + 1U < name_len; i++) {
			if (entry[i] == ' ') {
				break;
			}
			name[out++] = (char)entry[i];
		}
	}

	name[out] = '\0';
}

static int list_fat_root(const struct fat_layout *layout)
{
	uint32_t listed = 0U;
	const uint32_t sectors_to_scan = layout->fat32 ?
		layout->sectors_per_cluster : layout->root_dir_sectors;

	printk("SD_CARD_SMOKE_ROOT_BEGIN sector=%u sectors=%u\n",
	       layout->first_root_sector, sectors_to_scan);

	for (uint32_t sector_index = 0U; sector_index < sectors_to_scan; sector_index++) {
		int ret = read_sector(layout->first_root_sector + sector_index);

		if (ret != 0) {
			return ret;
		}

		for (uint32_t offset = 0U; offset < SD_SMOKE_SECTOR_SIZE;
		     offset += SD_SMOKE_FAT_DIR_ENTRY_SIZE) {
			const uint8_t *entry = &block[offset];
			char name[13];
			const uint8_t first = entry[0];
			const uint8_t attr = entry[11];
			const uint32_t size = le32(&entry[28]);

			if (first == 0x00U) {
				printk("SD_CARD_SMOKE_ROOT_DONE entries=%u truncated=0\n", listed);
				return 0;
			}
			if (first == 0xe5U || attr == SD_SMOKE_FAT_ATTR_LONG_NAME) {
				continue;
			}

			copy_fat_name(entry, name, sizeof(name));
			printk("SD_CARD_SMOKE_ROOT_ENTRY type=%s size=%u name=%s\n",
			       (attr & SD_SMOKE_FAT_ATTR_DIRECTORY) != 0U ? "dir" : "file",
			       size, name);
			listed++;
			if (listed == SD_SMOKE_MAX_ROOT_ENTRIES) {
				printk("SD_CARD_SMOKE_ROOT_DONE entries=%u truncated=1\n", listed);
				return 0;
			}
		}
	}

	printk("SD_CARD_SMOKE_ROOT_DONE entries=%u truncated=0\n", listed);
	return 0;
}

int main(void)
{
	uint32_t sector_count = 0;
	uint32_t sector_size = 0;
	uint32_t volume_lba;
	struct fat_layout layout;
	int ret;

	printk("SD_CARD_SMOKE_START diagnostic-only XTEINK X4 SPI SD-card wiring and FAT root read check\n");
	printk("SD_CARD_SMOKE_WIRING sck=GPIO8 mosi=GPIO10 miso=GPIO7 cs=GPIO12\n");

	if (!device_is_ready(sdhc_dev)) {
		print_fail("device_ready", -ENODEV);
		return 0;
	}

	printk("SD_CARD_SMOKE_SDHC_READY\n");

	if (!sd_is_card_present(sdhc_dev)) {
		print_fail("card_present", -ENODEV);
		return 0;
	}

	printk("SD_CARD_SMOKE_CARD_PRESENT\n");

	ret = sd_init(sdhc_dev, &card);
	if (ret != 0) {
		print_fail("sd_init", ret);
		return 0;
	}

	printk("SD_CARD_SMOKE_INIT_OK\n");

	ret = sdmmc_ioctl(&card, DISK_IOCTL_GET_SECTOR_COUNT, &sector_count);
	if (ret != 0) {
		print_fail("sector_count", ret);
		return 0;
	}

	ret = sdmmc_ioctl(&card, DISK_IOCTL_GET_SECTOR_SIZE, &sector_size);
	if (ret != 0) {
		print_fail("sector_size", ret);
		return 0;
	}

	printk("SD_CARD_SMOKE_INFO sectors=%u sector_size=%u\n", sector_count, sector_size);

	ret = read_sector(SD_SMOKE_READ_BLOCK);
	if (ret != 0) {
		return 0;
	}

	printk("SD_CARD_SMOKE_READ0_OK first_bytes=%02x%02x%02x%02x\n",
	       block[0], block[1], block[2], block[3]);

	volume_lba = detect_volume_lba();
	if (volume_lba != 0U) {
		ret = read_sector(volume_lba);
		if (ret != 0) {
			return 0;
		}
	}

	ret = parse_fat_layout(volume_lba, &layout);
	if (ret != 0) {
		return 0;
	}

	ret = list_fat_root(&layout);
	if (ret != 0) {
		return 0;
	}

	printk("SD_CARD_SMOKE_READY sectors=%u sector_size=%u\n", sector_count, sector_size);

	return 0;
}
