/*
 * Diagnostic-only XIAO ESP32-C3 + GDEQ0426T82/SSD1677 e-paper smoke test.
 *
 * This app bypasses SquidScript and the normal firmware runtime. It writes a
 * full-screen monochrome test image using one 800-pixel row buffer so the
 * physical display wiring can be checked without introducing a resident
 * framebuffer pattern into product firmware.
 */

#include <stdbool.h>
#include <stdint.h>
#include <string.h>

#include <zephyr/device.h>
#include <zephyr/drivers/gpio.h>
#include <zephyr/kernel.h>
#include <zephyr/sys/printk.h>

#define PANEL_WIDTH 800U
#define PANEL_HEIGHT 480U
#define ROW_BYTES (PANEL_WIDTH / 8U)
#define PANEL_LAST_Y (PANEL_HEIGHT - 1U)

#define BUSY_TIMEOUT_MS 60000
#define EPAPER_FIXED_RESET_DELAY_MS 2000
#define EPAPER_FIXED_REFRESH_DELAY_MS 8000

#define EPAPER_PIN_RESET 2U
#define EPAPER_PIN_CS 3U
#define EPAPER_PIN_BUSY 4U
#define EPAPER_PIN_DC 5U
#define EPAPER_PIN_SCK 8U
#define EPAPER_PIN_SDI 10U

#define SSD1677_CMD_GDO_CTRL 0x01
#define SSD1677_CMD_ENTRY_MODE 0x11
#define SSD1677_CMD_SW_RESET 0x12
#define SSD1677_CMD_TSENSOR_SELECTION 0x18
#define SSD1677_CMD_MASTER_ACTIVATION 0x20
#define SSD1677_CMD_UPDATE_CTRL2 0x22
#define SSD1677_CMD_WRITE_RAM 0x24
#define SSD1677_CMD_BOOSTER_SOFT_START 0x0c
#define SSD1677_CMD_BORDER_WAVEFORM 0x3c
#define SSD1677_CMD_RAM_XPOS_CTRL 0x44
#define SSD1677_CMD_RAM_YPOS_CTRL 0x45
#define SSD1677_CMD_RAM_XPOS_CNTR 0x4e
#define SSD1677_CMD_RAM_YPOS_CNTR 0x4f

#define SSD1677_ENTRY_X_INC_Y_INC_HORIZONTAL 0x03
#define SSD1677_UPDATE_FULL 0xf7

static const struct device *const gpio0_dev = DEVICE_DT_GET(DT_NODELABEL(gpio0));

static int bitbang_write_byte(uint8_t value)
{
	int ret = gpio_pin_set(gpio0_dev, EPAPER_PIN_CS, 0);

	if (ret != 0) {
		return ret;
	}

	for (uint8_t mask = 0x80; mask != 0; mask >>= 1) {
		ret = gpio_pin_set(gpio0_dev, EPAPER_PIN_SCK, 0);
		if (ret != 0) {
			return ret;
		}
		ret = gpio_pin_set(gpio0_dev, EPAPER_PIN_SDI, (value & mask) != 0);
		if (ret != 0) {
			return ret;
		}
		k_busy_wait(1);
		ret = gpio_pin_set(gpio0_dev, EPAPER_PIN_SCK, 1);
		if (ret != 0) {
			return ret;
		}
		k_busy_wait(1);
	}

	ret = gpio_pin_set(gpio0_dev, EPAPER_PIN_SCK, 0);
	if (ret != 0) {
		return ret;
	}
	return gpio_pin_set(gpio0_dev, EPAPER_PIN_CS, 1);
}

static int spi_write_bytes(const uint8_t *data, size_t len)
{
	for (size_t i = 0; i < len; ++i) {
		int ret = bitbang_write_byte(data[i]);

		if (ret != 0) {
			return ret;
		}
	}
	return 0;
}

static int write_command(uint8_t command)
{
	int ret = gpio_pin_set(gpio0_dev, EPAPER_PIN_DC, 0);

	if (ret != 0) {
		return ret;
	}
	return spi_write_bytes(&command, 1);
}

static int write_data(const uint8_t *data, size_t len)
{
	int ret = gpio_pin_set(gpio0_dev, EPAPER_PIN_DC, 1);

	if (ret != 0) {
		return ret;
	}
	return spi_write_bytes(data, len);
}

static int write_command_data(uint8_t command, const uint8_t *data, size_t len)
{
	int ret = write_command(command);

	if (ret != 0 || len == 0) {
		return ret;
	}
	return write_data(data, len);
}

static int wait_ready(const char *phase)
{
	int64_t deadline = k_uptime_get() + BUSY_TIMEOUT_MS;
	int initial = gpio_pin_get(gpio0_dev, EPAPER_PIN_BUSY);

	printk("EPAPER_HELLO_BUSY phase=%s initial=%d\n", phase, initial);

	while (gpio_pin_get(gpio0_dev, EPAPER_PIN_BUSY) > 0) {
		if (k_uptime_get() >= deadline) {
			printk("EPAPER_HELLO_ERROR busy timeout phase=%s\n", phase);
			return -ETIMEDOUT;
		}
		k_msleep(10);
	}

	printk("EPAPER_HELLO_BUSY phase=%s ready=%d\n", phase,
	       gpio_pin_get(gpio0_dev, EPAPER_PIN_BUSY));
	return 0;
}

static int epaper_reset(void)
{
	int ret = gpio_pin_set(gpio0_dev, EPAPER_PIN_RESET, 1);

	if (ret != 0) {
		return ret;
	}
	k_msleep(20);
	ret = gpio_pin_set(gpio0_dev, EPAPER_PIN_RESET, 0);
	if (ret != 0) {
		return ret;
	}
	k_msleep(20);
	ret = gpio_pin_set(gpio0_dev, EPAPER_PIN_RESET, 1);
	if (ret != 0) {
		return ret;
	}
	k_msleep(200);
	ret = wait_ready("hardware-reset");
	k_msleep(EPAPER_FIXED_RESET_DELAY_MS);
	return ret;
}

static int set_full_window(void)
{
	const uint8_t x_range[] = {
		0x00,
		0x00,
		(uint8_t)((PANEL_WIDTH - 1U) & 0xffU),
		(uint8_t)((PANEL_WIDTH - 1U) >> 8),
	};
	const uint8_t y_range[] = {
		0x00,
		0x00,
		(uint8_t)(PANEL_LAST_Y & 0xffU),
		(uint8_t)(PANEL_LAST_Y >> 8),
	};
	const uint8_t x_counter[] = {0x00, 0x00};
	const uint8_t y_counter[] = {0x00, 0x00};
	int ret = write_command_data(SSD1677_CMD_RAM_XPOS_CTRL, x_range, sizeof(x_range));

	if (ret != 0) {
		return ret;
	}
	ret = write_command_data(SSD1677_CMD_RAM_YPOS_CTRL, y_range, sizeof(y_range));
	if (ret != 0) {
		return ret;
	}
	ret = write_command_data(SSD1677_CMD_RAM_XPOS_CNTR, x_counter, sizeof(x_counter));
	if (ret != 0) {
		return ret;
	}
	return write_command_data(SSD1677_CMD_RAM_YPOS_CNTR, y_counter, sizeof(y_counter));
}

static int epaper_init(void)
{
	const uint8_t gate_output[] = {
		(uint8_t)(PANEL_LAST_Y & 0xffU),
		(uint8_t)(PANEL_LAST_Y >> 8),
		0x02,
	};
	const uint8_t entry_mode = SSD1677_ENTRY_X_INC_Y_INC_HORIZONTAL;
	const uint8_t border_waveform = 0x01;
	const uint8_t temp_sensor = 0x80;
	const uint8_t booster_soft_start[] = {0xae, 0xc7, 0xc3, 0xc0, 0x80};
	int ret = epaper_reset();

	if (ret != 0) {
		return ret;
	}
	ret = write_command(SSD1677_CMD_SW_RESET);
	if (ret != 0) {
		return ret;
	}
	ret = wait_ready("software-reset");
	if (ret != 0) {
		return ret;
	}
	k_msleep(EPAPER_FIXED_RESET_DELAY_MS);
	ret = write_command_data(SSD1677_CMD_TSENSOR_SELECTION, &temp_sensor, sizeof(temp_sensor));
	if (ret != 0) {
		return ret;
	}
	ret = write_command_data(SSD1677_CMD_BOOSTER_SOFT_START, booster_soft_start,
				 sizeof(booster_soft_start));
	if (ret != 0) {
		return ret;
	}
	ret = write_command_data(SSD1677_CMD_GDO_CTRL, gate_output, sizeof(gate_output));
	if (ret != 0) {
		return ret;
	}
	ret = write_command_data(SSD1677_CMD_ENTRY_MODE, &entry_mode, sizeof(entry_mode));
	if (ret != 0) {
		return ret;
	}
	ret = write_command_data(SSD1677_CMD_BORDER_WAVEFORM, &border_waveform, sizeof(border_waveform));
	if (ret != 0) {
		return ret;
	}
	return set_full_window();
}

static const uint8_t *glyph_for(char ch)
{
	static const uint8_t glyph_space[7] = {0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00};
	static const uint8_t glyph_d[7] = {0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e};
	static const uint8_t glyph_e[7] = {0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f};
	static const uint8_t glyph_h[7] = {0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11};
	static const uint8_t glyph_l[7] = {0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f};
	static const uint8_t glyph_o[7] = {0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e};
	static const uint8_t glyph_r[7] = {0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11};
	static const uint8_t glyph_w[7] = {0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0a};

	switch (ch) {
	case 'D':
		return glyph_d;
	case 'E':
		return glyph_e;
	case 'H':
		return glyph_h;
	case 'L':
		return glyph_l;
	case 'O':
		return glyph_o;
	case 'R':
		return glyph_r;
	case 'W':
		return glyph_w;
	default:
		return glyph_space;
	}
}

static void set_black_pixel(uint8_t row[ROW_BYTES], uint16_t x)
{
	if (x >= PANEL_WIDTH) {
		return;
	}

	uint16_t ram_x = (uint16_t)(PANEL_WIDTH - 1U - x);

	row[ram_x / 8U] &= (uint8_t)~(0x80U >> (ram_x % 8U));
}

static void draw_text_row(uint8_t row[ROW_BYTES], uint16_t y, const char *text,
			  uint16_t x0, uint16_t y0, uint8_t scale)
{
	if (y < y0 || y >= y0 + (7U * scale)) {
		return;
	}

	uint8_t glyph_row = (uint8_t)((y - y0) / scale);
	uint16_t cursor_x = x0;

	for (size_t i = 0; text[i] != '\0'; ++i) {
		const uint8_t *glyph = glyph_for(text[i]);

		for (uint8_t col = 0; col < 5U; ++col) {
			if ((glyph[glyph_row] & (0x10U >> col)) == 0U) {
				continue;
			}
			for (uint8_t dx = 0; dx < scale; ++dx) {
				set_black_pixel(row, cursor_x + (uint16_t)(col * scale) + dx);
			}
		}
		cursor_x += (uint16_t)(6U * scale);
	}
}

static void render_test_row(uint8_t row[ROW_BYTES], uint16_t y)
{
	memset(row, 0xff, ROW_BYTES);

	if (y < 5U || y >= PANEL_HEIGHT - 5U) {
		memset(row, 0x00, ROW_BYTES);
		return;
	}

	for (uint16_t x = 0; x < 5U; ++x) {
		set_black_pixel(row, x);
		set_black_pixel(row, PANEL_WIDTH - 1U - x);
	}

	draw_text_row(row, y, "HELLO WORLD", 84, 176, 10);

	if (y >= 320U && y < 380U) {
		for (uint16_t x = 80U; x < 720U; ++x) {
			if (((x / 16U) % 2U) == 0U) {
				set_black_pixel(row, x);
			}
		}
	}

	if (y >= 400U && y < 424U) {
		for (uint16_t x = 80U; x < 720U; ++x) {
			set_black_pixel(row, x);
		}
	}
}

static int write_plane(uint8_t command, bool test_pattern)
{
	static uint8_t row[ROW_BYTES];
	int ret = write_command(command);

	if (ret != 0) {
		return ret;
	}

	for (uint16_t y = 0; y < PANEL_HEIGHT; ++y) {
		if (test_pattern) {
			render_test_row(row, y);
		} else {
			memset(row, 0xff, ROW_BYTES);
		}
		ret = write_data(row, sizeof(row));
		if (ret != 0) {
			return ret;
		}
	}

	return 0;
}

static int refresh_display(void)
{
	const uint8_t update = SSD1677_UPDATE_FULL;
	int ret = write_command_data(SSD1677_CMD_UPDATE_CTRL2, &update, sizeof(update));

	if (ret != 0) {
		return ret;
	}
	ret = write_command(SSD1677_CMD_MASTER_ACTIVATION);
	if (ret != 0) {
		return ret;
	}
	ret = wait_ready("refresh");
	k_msleep(EPAPER_FIXED_REFRESH_DELAY_MS);
	return ret;
}

int main(void)
{
	int ret;

	printk("EPAPER_HELLO_START diagnostic-only XIAO ESP32-C3 GDEQ0426T82 smoke test\n");

	if (!device_is_ready(gpio0_dev)) {
		printk("EPAPER_HELLO_ERROR gpio0 not ready\n");
		return 1;
	}

	ret = gpio_pin_configure(gpio0_dev, EPAPER_PIN_DC, GPIO_OUTPUT_INACTIVE);
	if (ret != 0) {
		printk("EPAPER_HELLO_ERROR dc configure %d\n", ret);
		return 1;
	}
	ret = gpio_pin_configure(gpio0_dev, EPAPER_PIN_RESET, GPIO_OUTPUT_ACTIVE);
	if (ret != 0) {
		printk("EPAPER_HELLO_ERROR reset configure %d\n", ret);
		return 1;
	}
	ret = gpio_pin_configure(gpio0_dev, EPAPER_PIN_CS, GPIO_OUTPUT_ACTIVE);
	if (ret != 0) {
		printk("EPAPER_HELLO_ERROR cs configure %d\n", ret);
		return 1;
	}
	ret = gpio_pin_configure(gpio0_dev, EPAPER_PIN_SCK, GPIO_OUTPUT_INACTIVE);
	if (ret != 0) {
		printk("EPAPER_HELLO_ERROR sck configure %d\n", ret);
		return 1;
	}
	ret = gpio_pin_configure(gpio0_dev, EPAPER_PIN_SDI, GPIO_OUTPUT_INACTIVE);
	if (ret != 0) {
		printk("EPAPER_HELLO_ERROR sdi configure %d\n", ret);
		return 1;
	}
	ret = gpio_pin_configure(gpio0_dev, EPAPER_PIN_BUSY, GPIO_INPUT);
	if (ret != 0) {
		printk("EPAPER_HELLO_ERROR busy configure %d\n", ret);
		return 1;
	}
	printk("EPAPER_HELLO_BUSY phase=configure value=%d\n",
	       gpio_pin_get(gpio0_dev, EPAPER_PIN_BUSY));

	ret = epaper_init();
	if (ret != 0) {
		printk("EPAPER_HELLO_ERROR init %d\n", ret);
		return 1;
	}
	ret = write_plane(SSD1677_CMD_WRITE_RAM, true);
	if (ret != 0) {
		printk("EPAPER_HELLO_ERROR bw-plane %d\n", ret);
		return 1;
	}
	ret = refresh_display();
	if (ret != 0) {
		printk("EPAPER_HELLO_ERROR refresh %d\n", ret);
		return 1;
	}

	printk("EPAPER_HELLO_READY visual confirmation: HELLO WORLD with border and bars\n");

	while (true) {
		k_sleep(K_SECONDS(30));
	}
}
