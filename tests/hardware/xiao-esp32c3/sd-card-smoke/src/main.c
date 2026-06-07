/*
 * Diagnostic-only XIAO ESP32-C3 SPI SD-card smoke test.
 *
 * This app proves the bench wiring by initializing the SD card and reading one
 * sector. It is read-only and does not exercise SquidScript SD storage.
 */

#include <stdint.h>

#include <zephyr/device.h>
#include <zephyr/drivers/disk.h>
#include <zephyr/kernel.h>
#include <zephyr/sd/sd.h>
#include <zephyr/sd/sdmmc.h>

#define SD_SMOKE_READ_BLOCK 0U
#define SD_SMOKE_READ_BLOCKS 1U
#define SD_SMOKE_SECTOR_SIZE 512U

static const struct device *const sdhc_dev = DEVICE_DT_GET(DT_ALIAS(sdhc0));
static struct sd_card card;
static uint8_t block[SD_SMOKE_SECTOR_SIZE] __aligned(CONFIG_SDHC_BUFFER_ALIGNMENT);

static void print_fail(const char *step, int code)
{
	printk("SD_CARD_SMOKE_FAIL step=%s code=%d\n", step, code);
}

int main(void)
{
	uint32_t sector_count = 0;
	uint32_t sector_size = 0;
	int ret;

	printk("SD_CARD_SMOKE_START diagnostic-only XIAO ESP32-C3 SPI SD-card wiring check\n");
	printk("SD_CARD_SMOKE_WIRING sck=D8/GPIO8 mosi=D10/GPIO10 miso=D5/GPIO7 cs=D4/GPIO6\n");

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

	ret = sdmmc_read_blocks(&card, block, SD_SMOKE_READ_BLOCK, SD_SMOKE_READ_BLOCKS);
	if (ret != 0) {
		print_fail("read_block0", ret);
		return 0;
	}

	printk("SD_CARD_SMOKE_READ0_OK first_bytes=%02x%02x%02x%02x\n",
	       block[0], block[1], block[2], block[3]);
	printk("SD_CARD_SMOKE_READY sectors=%u sector_size=%u\n", sector_count, sector_size);

	return 0;
}
