#include "vm_runtime_display_backend.h"
#include "ssd1677_gray2.h"
#include "debug_log.h"

#include <errno.h>
#include <stdlib.h>
#include <string.h>

#include <zephyr/devicetree.h>
#include <zephyr/drivers/gpio.h>
#include <zephyr/drivers/spi.h>
#include <zephyr/fs/fs.h>
#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(squidscript_ssd1677, LOG_LEVEL_INF);

#define SSD1677_NODE DT_ALIAS(epaper0)

static bool ssd1677_color_is_black(sq_display_color_t color)
{
	return sq_display_color_is_black(color);
}

#if defined(CONFIG_ZTEST)
static void ssd1677_test_set_pixel(uint8_t *line, size_t line_len, uint16_t physical_x,
				   bool black)
{
	size_t panel_width = line_len * 8U;

	if (line == NULL || physical_x >= panel_width) {
		return;
	}
	size_t ram_x = panel_width - 1U - physical_x;
	uint8_t mask = (uint8_t)(0x80U >> (ram_x % 8U));

	if (black) {
		line[ram_x / 8U] &= (uint8_t)~mask;
	} else {
		line[ram_x / 8U] |= mask;
	}
}

static void ssd1677_test_draw_rect_row(uint8_t *line, size_t line_len, uint16_t physical_y,
				       const struct sq_vm_runtime_display_op *op)
{
	bool fill_black = ssd1677_color_is_black(op->u.rect.fill_color);
	bool stroke_black = ssd1677_color_is_black(op->u.rect.stroke_color);
	bool has_fill = sq_display_color_is_set(op->u.rect.fill_color);
	bool has_stroke = sq_display_color_is_set(op->u.rect.stroke_color);
	int32_t row_y = physical_y;
	int32_t left = op->x;
	int32_t top = op->y;
	int32_t right = op->x + op->u.rect.w;
	int32_t bottom = op->y + op->u.rect.h;

	if (op->u.rect.w <= 0 || op->u.rect.h <= 0 || row_y < top || row_y >= bottom) {
		return;
	}
	if (left < 0) {
		left = 0;
	}
	if (right > (int32_t)(line_len * 8U)) {
		right = (int32_t)(line_len * 8U);
	}
	if (left >= right) {
		return;
	}
	if (has_fill) {
		for (int32_t x = left; x < right; ++x) {
			ssd1677_test_set_pixel(line, line_len, (uint16_t)x, fill_black);
		}
	}
	if (has_stroke && (row_y == top || row_y == bottom - 1)) {
		for (int32_t x = left; x < right; ++x) {
			ssd1677_test_set_pixel(line, line_len, (uint16_t)x, stroke_black);
		}
	} else if (has_stroke) {
		if (op->x >= 0 && op->x < (int32_t)(line_len * 8U)) {
			ssd1677_test_set_pixel(line, line_len, (uint16_t)op->x, stroke_black);
		}
		if (op->x + op->u.rect.w - 1 >= 0 && op->x + op->u.rect.w - 1 < (int32_t)(line_len * 8U)) {
			ssd1677_test_set_pixel(line, line_len, (uint16_t)(op->x + op->u.rect.w - 1),
					       stroke_black);
		}
	}
}

void sq_ssd1677_test_render_1bpp_row(uint8_t *line, size_t line_len, uint16_t physical_y,
				     const struct sq_vm_runtime_display_op *ops,
				     size_t op_count)
{
	if (line == NULL || line_len == 0) {
		return;
	}
	memset(line, 0xff, line_len);
	if (ops == NULL) {
		return;
	}
	for (size_t i = 0; i < op_count; ++i) {
		switch (ops[i].kind) {
		case SQ_VM_RUNTIME_DISPLAY_OP_CLEAR:
			memset(line, ssd1677_color_is_black(ops[i].u.clear.color) ? 0x00 : 0xff, line_len);
			break;
		case SQ_VM_RUNTIME_DISPLAY_OP_RECT:
			ssd1677_test_draw_rect_row(line, line_len, physical_y, &ops[i]);
			break;
		default:
			break;
		}
	}
}

bool sq_ssd1677_test_row_has_black_pixel(const uint8_t *line, size_t line_len,
					 uint16_t physical_x)
{
	size_t panel_width = line_len * 8U;

	if (line == NULL || physical_x >= panel_width) {
		return false;
	}
	size_t ram_x = panel_width - 1U - physical_x;

	return (line[ram_x / 8U] & (0x80U >> (ram_x % 8U))) == 0;
}
#endif

#if IS_ENABLED(CONFIG_SQUIDSCRIPT_TARGET_DISPLAY_SSD1677_EXPECTED) && \
	DT_NODE_HAS_STATUS(SSD1677_NODE, okay)

#define PANEL_WIDTH DT_PROP(SSD1677_NODE, width)
#define PANEL_HEIGHT DT_PROP(SSD1677_NODE, height)
#define ROW_BYTES (PANEL_WIDTH / 8U)
#define PANEL_LAST_Y (PANEL_HEIGHT - 1U)
#define BUSY_TIMEOUT_MS 60000

#if CONFIG_SQ_TARGET_DISPLAY_PHYSICAL_WIDTH != PANEL_WIDTH
#error "target display physical width must match the SSD1677 devicetree width"
#endif

#if CONFIG_SQ_TARGET_DISPLAY_PHYSICAL_HEIGHT != PANEL_HEIGHT
#error "target display physical height must match the SSD1677 devicetree height"
#endif

#define LOGICAL_WIDTH CONFIG_SQ_TARGET_DISPLAY_LOGICAL_WIDTH
#define LOGICAL_HEIGHT CONFIG_SQ_TARGET_DISPLAY_LOGICAL_HEIGHT
#define LOGICAL_ROTATION CONFIG_SQ_TARGET_DISPLAY_ROTATION

#define SSD1677_CMD_GDO_CTRL 0x01
#define SSD1677_CMD_GATE_VOLTAGE 0x03
#define SSD1677_CMD_SOURCE_VOLTAGE 0x04
#define SSD1677_CMD_BOOSTER_SOFT_START 0x0c
#define SSD1677_CMD_ENTRY_MODE 0x11
#define SSD1677_CMD_SW_RESET 0x12
#define SSD1677_CMD_TSENSOR_SELECTION 0x18
#define SSD1677_CMD_DISPLAY_UPDATE_CTRL 0x21
#define SSD1677_CMD_MASTER_ACTIVATION 0x20
#define SSD1677_CMD_UPDATE_CTRL2 0x22
#define SSD1677_CMD_WRITE_RAM 0x24
#define SSD1677_CMD_VCOM_VOLTAGE 0x2c
#define SSD1677_CMD_WRITE_RED_RAM 0x26
#define SSD1677_CMD_WRITE_LUT 0x32
#define SSD1677_CMD_BORDER_WAVEFORM 0x3c
#define SSD1677_CMD_RAM_XPOS_CTRL 0x44
#define SSD1677_CMD_RAM_YPOS_CTRL 0x45
#define SSD1677_CMD_RAM_XPOS_CNTR 0x4e
#define SSD1677_CMD_RAM_YPOS_CNTR 0x4f

#define SSD1677_ENTRY_X_INC_Y_INC_HORIZONTAL 0x03
#define SSD1677_UPDATE_FULL 0xf7
#define SSD1677_UPDATE_GRAYSCALE 0xc7
#define SSD1677_UPDATE_PARTIAL 0xfc
#define SSD1677_BINBOOK_FULL_REFRESH_CADENCE 5U
#define BINBOOK_PIXEL_FORMAT_GRAY2_PACKED 2U
#define BINBOOK_COMPRESSION_RLE_PACKBITS 1U
#define BINBOOK_GRAY2_ROW_BYTES (PANEL_WIDTH / 4U)
#define BINBOOK_GRAY2_PAGE_BYTES (BINBOOK_GRAY2_ROW_BYTES * PANEL_HEIGHT)

enum ssd1677_display_mode {
	SSD1677_DISPLAY_MODE_NONE,
	SSD1677_DISPLAY_MODE_BW,
	SSD1677_DISPLAY_MODE_GRAYSCALE,
};

static const struct device *const spi_dev = DEVICE_DT_GET(DT_PHANDLE(SSD1677_NODE, spi));
static const struct gpio_dt_spec cs_gpio = GPIO_DT_SPEC_GET(SSD1677_NODE, cs_gpios);
static const struct gpio_dt_spec dc_gpio = GPIO_DT_SPEC_GET(SSD1677_NODE, dc_gpios);
static const struct gpio_dt_spec reset_gpio = GPIO_DT_SPEC_GET(SSD1677_NODE, reset_gpios);
static const struct gpio_dt_spec busy_gpio = GPIO_DT_SPEC_GET(SSD1677_NODE, busy_gpios);
static const struct spi_config spi_cfg = {
	.frequency = CONFIG_SQ_DISPLAY_SPI_MAX_FREQUENCY,
	.operation = SPI_WORD_SET(8) | SPI_TRANSFER_MSB,
	.slave = 0,
};

static enum ssd1677_display_mode display_mode;
static uint8_t row[ROW_BYTES];
static struct sq_ssd1677_binbook_refresh_state binbook_refresh_state;
static struct sq_vm_runtime_binbook_page binbook_previous_page;

#define FB_FRAMEBUFFER_SIZE (PANEL_WIDTH * PANEL_HEIGHT / 8U)
static uint8_t fb_framebuffer[FB_FRAMEBUFFER_SIZE];

static int configure_display(void);

static const uint8_t ssd1677_lut_4g[] = {
	0x80, 0x48, 0x4a, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	0x0a, 0x48, 0x68, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	0x88, 0x48, 0x60, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	0xa8, 0x48, 0x45, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	0x07, 0x1e, 0x1c, 0x02, 0x00,
	0x05, 0x01, 0x05, 0x01, 0x02,
	0x08, 0x01, 0x01, 0x04, 0x04,
	0x00, 0x02, 0x01, 0x02, 0x02,
	0x00, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x01,
	0x22, 0x22, 0x22, 0x22, 0x22,
	0x17, 0x41, 0xa8, 0x32, 0x30,
	0x00, 0x00,
};

struct packbits_reader {
	struct fs_file_t file;
	uint32_t compressed_left;
	uint8_t repeat_value;
	uint8_t repeat_remaining;
	uint8_t literal_remaining;
	uint8_t chunk[512];
	uint32_t chunk_len;
	uint32_t chunk_pos;
};

static int spi_write_bytes(const uint8_t *data, size_t len)
{
	const struct spi_buf buf = {
		.buf = (void *)data,
		.len = len,
	};
	const struct spi_buf_set tx = {
		.buffers = &buf,
		.count = 1,
	};
	int ret = gpio_pin_set_dt(&cs_gpio, 1);

	if (ret != 0) {
		return ret;
	}
	ret = spi_write(spi_dev, &spi_cfg, &tx);
	int cs_ret = gpio_pin_set_dt(&cs_gpio, 0);

	return ret != 0 ? ret : cs_ret;
}

static int write_command(uint8_t command)
{
	int ret = gpio_pin_set_dt(&dc_gpio, 0);

	if (ret != 0) {
		return ret;
	}
	return spi_write_bytes(&command, 1);
}

static int write_data(const uint8_t *data, size_t len)
{
	int ret = gpio_pin_set_dt(&dc_gpio, 1);

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

static int wait_ready(const char *phase, bool *observed_busy)
{
	int64_t deadline = k_uptime_get() + BUSY_TIMEOUT_MS;
	int initial = gpio_pin_get_dt(&busy_gpio);

	if (observed_busy != NULL && initial > 0) {
		*observed_busy = true;
	}
	while (gpio_pin_get_dt(&busy_gpio) > 0) {
		if (k_uptime_get() >= deadline) {
			LOG_ERR("display busy timeout phase=%s", phase);
			return -ETIMEDOUT;
		}
		if (observed_busy != NULL) {
			*observed_busy = true;
		}
		k_msleep(10);
	}
	LOG_INF("display busy phase=%s initial=%d ready=%d observed=%d", phase, initial,
		gpio_pin_get_dt(&busy_gpio), observed_busy != NULL && *observed_busy);
	return 0;
}

static int epaper_reset(void)
{
	int ret = gpio_pin_set_dt(&reset_gpio, 0);

	if (ret != 0) {
		return ret;
	}
	k_msleep(20);
	ret = gpio_pin_set_dt(&reset_gpio, 1);
	if (ret != 0) {
		return ret;
	}
	k_msleep(20);
	ret = gpio_pin_set_dt(&reset_gpio, 0);
	if (ret != 0) {
		return ret;
	}
	k_msleep(200);
	return wait_ready("hardware-reset", NULL);
}

static int set_window(uint16_t x0, uint16_t y0, uint16_t x1, uint16_t y1)
{
	const uint8_t x_range[] = {
		(uint8_t)(x0 & 0xffU),
		(uint8_t)(x0 >> 8),
		(uint8_t)(x1 & 0xffU),
		(uint8_t)(x1 >> 8),
	};
	const uint8_t y_range[] = {
		(uint8_t)(y0 & 0xffU),
		(uint8_t)(y0 >> 8),
		(uint8_t)(y1 & 0xffU),
		(uint8_t)(y1 >> 8),
	};
	const uint8_t x_start[] = {(uint8_t)(x0 & 0xffU), (uint8_t)(x0 >> 8)};
	const uint8_t y_start[] = {(uint8_t)(y0 & 0xffU), (uint8_t)(y0 >> 8)};
	int ret = write_command_data(SSD1677_CMD_RAM_XPOS_CTRL, x_range, sizeof(x_range));

	if (ret != 0) {
		return ret;
	}
	ret = write_command_data(SSD1677_CMD_RAM_YPOS_CTRL, y_range, sizeof(y_range));
	if (ret != 0) {
		return ret;
	}
	ret = write_command_data(SSD1677_CMD_RAM_XPOS_CNTR, x_start, sizeof(x_start));
	if (ret != 0) {
		return ret;
	}
	return write_command_data(SSD1677_CMD_RAM_YPOS_CNTR, y_start, sizeof(y_start));
}

static int set_full_window(void)
{
	return set_window(0, 0, PANEL_WIDTH - 1U, PANEL_LAST_Y);
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
	ret = wait_ready("software-reset", NULL);
	if (ret != 0) {
		return ret;
	}
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
	ret = write_command_data(SSD1677_CMD_BORDER_WAVEFORM, &border_waveform,
				 sizeof(border_waveform));
	if (ret != 0) {
		return ret;
	}
	display_mode = SSD1677_DISPLAY_MODE_BW;
	return set_full_window();
}

static int init_grayscale_display(void)
{
	const uint8_t gate_output[] = {
		(uint8_t)(PANEL_LAST_Y & 0xffU),
		(uint8_t)(PANEL_LAST_Y >> 8),
		0x02,
	};
	const uint8_t entry_mode = SSD1677_ENTRY_X_INC_Y_INC_HORIZONTAL;
	const uint8_t border_waveform = 0x00;
	const uint8_t temp_sensor = 0x80;
	const uint8_t booster_soft_start[] = {0xae, 0xc7, 0xc3, 0xc0, 0x80};
	const uint8_t source_voltage[] = {ssd1677_lut_4g[106], ssd1677_lut_4g[107],
					  ssd1677_lut_4g[108]};
	int ret = epaper_reset();

	if (ret != 0) {
		return ret;
	}
	ret = write_command(SSD1677_CMD_SW_RESET);
	if (ret != 0) {
		return ret;
	}
	ret = wait_ready("software-reset", NULL);
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
	ret = write_command_data(SSD1677_CMD_BORDER_WAVEFORM, &border_waveform,
				 sizeof(border_waveform));
	if (ret != 0) {
		return ret;
	}
	ret = write_command_data(SSD1677_CMD_TSENSOR_SELECTION, &temp_sensor, sizeof(temp_sensor));
	if (ret != 0) {
		return ret;
	}
	ret = set_full_window();
	if (ret != 0) {
		return ret;
	}
	ret = write_command_data(SSD1677_CMD_WRITE_LUT, ssd1677_lut_4g, 105U);
	if (ret != 0) {
		return ret;
	}
	ret = write_command_data(SSD1677_CMD_GATE_VOLTAGE, &ssd1677_lut_4g[105], 1U);
	if (ret != 0) {
		return ret;
	}
	ret = write_command_data(SSD1677_CMD_SOURCE_VOLTAGE, source_voltage,
				 sizeof(source_voltage));
	if (ret != 0) {
		return ret;
	}
	ret = write_command_data(SSD1677_CMD_VCOM_VOLTAGE, &ssd1677_lut_4g[109], 1U);
	if (ret != 0) {
		return ret;
	}
	display_mode = SSD1677_DISPLAY_MODE_GRAYSCALE;
	return 0;
}

static const uint8_t *glyph_for(char ch)
{
	static const uint8_t glyph_space[7] = {0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00};
	static const uint8_t glyph_digits[10][7] = {
		{0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e},
		{0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e},
		{0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f},
		{0x1e, 0x01, 0x01, 0x0e, 0x01, 0x01, 0x1e},
		{0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02},
		{0x1f, 0x10, 0x10, 0x1e, 0x01, 0x01, 0x1e},
		{0x06, 0x08, 0x10, 0x1e, 0x11, 0x11, 0x0e},
		{0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08},
		{0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e},
		{0x0e, 0x11, 0x11, 0x0f, 0x01, 0x02, 0x0c},
	};
	static const uint8_t glyph_letters[26][7] = {
		{0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11},
		{0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e},
		{0x0e, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0e},
		{0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e},
		{0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f},
		{0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10},
		{0x0e, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0f},
		{0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11},
		{0x0e, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0e},
		{0x07, 0x02, 0x02, 0x02, 0x12, 0x12, 0x0c},
		{0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11},
		{0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f},
		{0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11},
		{0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11},
		{0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e},
		{0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10},
		{0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d},
		{0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11},
		{0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e},
		{0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04},
		{0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e},
		{0x11, 0x11, 0x11, 0x11, 0x11, 0x0a, 0x04},
		{0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0a},
		{0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11},
		{0x11, 0x11, 0x0a, 0x04, 0x04, 0x04, 0x04},
		{0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f},
	};
	static const uint8_t glyph_colon[7] = {0x00, 0x04, 0x04, 0x00, 0x04, 0x04, 0x00};
	static const uint8_t glyph_dash[7] = {0x00, 0x00, 0x00, 0x1f, 0x00, 0x00, 0x00};
	static const uint8_t glyph_slash[7] = {0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10};

	if (ch >= 'a' && ch <= 'z') {
		ch = (char)(ch - 'a' + 'A');
	}
	if (ch >= 'A' && ch <= 'Z') {
		return glyph_letters[ch - 'A'];
	}
	if (ch >= '0' && ch <= '9') {
		return glyph_digits[ch - '0'];
	}
	if (ch == ':') {
		return glyph_colon;
	}
	if (ch == '-' || ch == '_') {
		return glyph_dash;
	}
	if (ch == '/') {
		return glyph_slash;
	}
	return glyph_space;
}

static bool logical_to_physical(uint16_t logical_x, uint16_t logical_y,
				uint16_t *physical_x, uint16_t *physical_y)
{
	if (logical_x >= LOGICAL_WIDTH || logical_y >= LOGICAL_HEIGHT || physical_x == NULL ||
	    physical_y == NULL) {
		return false;
	}
	switch (LOGICAL_ROTATION) {
	case 0:
		*physical_x = logical_x;
		*physical_y = logical_y;
		break;
	case 90:
		*physical_x = logical_y;
		*physical_y = (uint16_t)(LOGICAL_WIDTH - 1U - logical_x);
		break;
	case 180:
		*physical_x = (uint16_t)(LOGICAL_WIDTH - 1U - logical_x);
		*physical_y = (uint16_t)(LOGICAL_HEIGHT - 1U - logical_y);
		break;
	case 270:
		*physical_x = (uint16_t)(LOGICAL_HEIGHT - 1U - logical_y);
		*physical_y = logical_x;
		break;
	default:
		return false;
	}
	return *physical_x < PANEL_WIDTH && *physical_y < PANEL_HEIGHT;
}

static void set_black_pixel(uint8_t line[ROW_BYTES], uint16_t x)
{
	if (x >= PANEL_WIDTH) {
		return;
	}
	uint16_t ram_x = (uint16_t)(PANEL_WIDTH - 1U - x);

	line[ram_x / 8U] &= (uint8_t)~(0x80U >> (ram_x % 8U));
}

static void set_pixel(uint8_t line[ROW_BYTES], uint16_t x, bool black)
{
	if (x >= PANEL_WIDTH) {
		return;
	}
	if (black) {
		set_black_pixel(line, x);
		return;
	}
	uint16_t ram_x = (uint16_t)(PANEL_WIDTH - 1U - x);

	line[ram_x / 8U] |= (uint8_t)(0x80U >> (ram_x % 8U));
}

static const struct sq_vm_runtime_display_op *find_binbook_drawable_op(
	const struct sq_vm_runtime_display_op *ops, size_t op_count)
{
	for (size_t i = op_count; i > 0; --i) {
		if (ops[i - 1].kind == SQ_VM_RUNTIME_DISPLAY_OP_BINBOOK_DRAWABLE) {
			return &ops[i - 1];
		}
	}
	return NULL;
}

static bool is_binbook_page_only_stream(const struct sq_vm_runtime_display_op *ops, size_t op_count)
{
	bool found_binbook = false;

	for (size_t i = 0; i < op_count; ++i) {
		switch (ops[i].kind) {
		case SQ_VM_RUNTIME_DISPLAY_OP_CLEAR:
			break;
		case SQ_VM_RUNTIME_DISPLAY_OP_BINBOOK_DRAWABLE:
			if (found_binbook) {
				return false;
			}
			found_binbook = true;
			break;
		default:
			return false;
		}
	}
	return found_binbook;
}

static void composed_remember_previous_ops(const struct sq_vm_runtime_display_op *ops,
					   size_t op_count)
{
	ARG_UNUSED(ops);
	ARG_UNUSED(op_count);
}

static int packbits_read_raw(struct packbits_reader *reader, uint8_t *out)
{
	if (reader->compressed_left == 0) {
		return -EIO;
	}
	if (reader->chunk_pos < reader->chunk_len) {
		*out = reader->chunk[reader->chunk_pos++];
		reader->compressed_left--;
		return 0;
	}
	size_t to_read = reader->compressed_left;

	if (to_read > sizeof(reader->chunk)) {
		to_read = sizeof(reader->chunk);
	}
	ssize_t read = fs_read(&reader->file, reader->chunk, to_read);

	if (read <= 0) {
		return read == 0 ? -EIO : (int)read;
	}
	reader->chunk_len = (uint32_t)read;
	reader->chunk_pos = 0;
	*out = reader->chunk[reader->chunk_pos++];
	reader->compressed_left--;
	return 0;
}

static int packbits_next_byte(struct packbits_reader *reader, uint8_t *out)
{
	if (reader->repeat_remaining > 0) {
		reader->repeat_remaining--;
		*out = reader->repeat_value;
		return 0;
	}
	if (reader->literal_remaining > 0) {
		reader->literal_remaining--;
		return packbits_read_raw(reader, out);
	}
	uint8_t control = 0;
	int ret = packbits_read_raw(reader, &control);

	if (ret != 0) {
		return ret;
	}
	if (control <= 127U) {
		reader->literal_remaining = control + 1U;
		reader->literal_remaining--;
		return packbits_read_raw(reader, out);
	}
	ret = packbits_read_raw(reader, &reader->repeat_value);
	if (ret != 0) {
		return ret;
	}
	reader->repeat_remaining = (control & 0x7fU);
	*out = reader->repeat_value;
	return 0;
}

static void apply_active_mask_to_row(uint8_t line[ROW_BYTES], uint16_t x, uint8_t active_mask)
{
	for (uint8_t pixel = 0; pixel < 4U; ++pixel) {
		if ((active_mask & (0x80U >> pixel)) != 0U) {
			set_black_pixel(line, x + pixel);
		}
	}
}

static int stream_binbook_gray2_plane(const struct sq_vm_runtime_binbook_page *page,
				      uint8_t command, bool msb_plane)
{
	struct packbits_reader reader = {0};
	int ret;

	fs_file_t_init(&reader.file);
	ret = fs_open(&reader.file, page->path, FS_O_READ);
	if (ret != 0) {
		return ret;
	}
	ret = fs_seek(&reader.file, (off_t)page->blob_offset, FS_SEEK_SET);
	if (ret != 0) {
		(void)fs_close(&reader.file);
		return ret;
	}
	ret = write_command(command);
	if (ret != 0) {
		(void)fs_close(&reader.file);
		return ret;
	}
	reader.compressed_left = page->compressed_size;
	for (uint16_t y = 0; y < PANEL_HEIGHT; ++y) {
		memset(row, 0xff, sizeof(row));
		for (uint16_t x = 0; x < PANEL_WIDTH; x += 4U) {
			uint8_t packed = 0;
			uint8_t active_mask;

			ret = packbits_next_byte(&reader, &packed);
			if (ret != 0) {
				(void)fs_close(&reader.file);
				return ret;
			}
			active_mask = msb_plane ? sq_ssd1677_gray2_msb_active_mask(packed)
						: sq_ssd1677_gray2_lsb_active_mask(packed);
			apply_active_mask_to_row(row, x, active_mask);
		}
		ret = write_data(row, sizeof(row));
		if (ret != 0) {
			(void)fs_close(&reader.file);
			return ret;
		}
	}
	(void)fs_close(&reader.file);
	return 0;
}

static int stream_binbook_gray2_page(const struct sq_vm_runtime_binbook_page *page)
{
	int ret;

	if (page == NULL || page->path[0] == '\0' ||
	    page->pixel_format != BINBOOK_PIXEL_FORMAT_GRAY2_PACKED ||
	    page->compression_method != BINBOOK_COMPRESSION_RLE_PACKBITS ||
	    page->stored_width != PANEL_WIDTH || page->stored_height != PANEL_HEIGHT ||
	    page->uncompressed_size != BINBOOK_GRAY2_PAGE_BYTES || page->compressed_size == 0) {
		return -ENOTSUP;
	}
	ret = stream_binbook_gray2_plane(page, SSD1677_CMD_WRITE_RED_RAM, true);
	if (ret != 0) {
		return ret;
	}
	return stream_binbook_gray2_plane(page, SSD1677_CMD_WRITE_RAM, false);
}

static int validate_binbook_gray2_page(const struct sq_vm_runtime_binbook_page *page)
{
	if (page == NULL || page->path[0] == '\0' ||
	    page->pixel_format != BINBOOK_PIXEL_FORMAT_GRAY2_PACKED ||
	    page->compression_method != BINBOOK_COMPRESSION_RLE_PACKBITS ||
	    page->stored_width != PANEL_WIDTH || page->stored_height != PANEL_HEIGHT ||
	    page->uncompressed_size != BINBOOK_GRAY2_PAGE_BYTES || page->compressed_size == 0) {
		return -ENOTSUP;
	}
	return 0;
}

static enum sq_ssd1677_binbook_refresh_kind binbook_refresh_kind(void)
{
	enum sq_ssd1677_binbook_refresh_kind kind =
		sq_ssd1677_binbook_refresh_decide(&binbook_refresh_state,
						  SSD1677_BINBOOK_FULL_REFRESH_CADENCE);
	sq_debug_log_append("%lld:binbook_refresh_decide:prev_valid=%d:fast_count=%d:cadence=%d:result=%d",
			    (long long)k_uptime_get(),
			    (int)binbook_refresh_state.previous_page_valid,
			    (int)binbook_refresh_state.fast_refresh_count,
			    (int)SSD1677_BINBOOK_FULL_REFRESH_CADENCE,
			    (int)kind);
	return kind;
}

static void binbook_remember_previous_page(const struct sq_vm_runtime_binbook_page *page)
{
	if (page == NULL) {
		sq_debug_log_append("%lld:binbook_prev_clear", (long long)k_uptime_get());
		sq_ssd1677_binbook_refresh_reset(&binbook_refresh_state);
		memset(&binbook_previous_page, 0, sizeof(binbook_previous_page));
		return;
	}
	sq_debug_log_append("%lld:binbook_prev_set", (long long)k_uptime_get());
	binbook_previous_page = *page;
}

static void binbook_record_refresh(enum sq_ssd1677_binbook_refresh_kind refresh)
{
	sq_ssd1677_binbook_refresh_record(&binbook_refresh_state, refresh);
}

static int stream_binbook_gray2_bw_plane(const struct sq_vm_runtime_binbook_page *page,
					 uint8_t command, bool ordered_dither)
{
	struct packbits_reader reader = {0};
	int ret;

	fs_file_t_init(&reader.file);
	ret = fs_open(&reader.file, page->path, FS_O_READ);
	if (ret != 0) {
		return ret;
	}
	ret = fs_seek(&reader.file, (off_t)page->blob_offset, FS_SEEK_SET);
	if (ret != 0) {
		(void)fs_close(&reader.file);
		return ret;
	}
	ret = write_command(command);
	if (ret != 0) {
		(void)fs_close(&reader.file);
		return ret;
	}
	reader.compressed_left = page->compressed_size;
	for (uint16_t y = 0; y < PANEL_HEIGHT; ++y) {
		memset(row, 0xff, sizeof(row));
		for (uint16_t x = 0; x < PANEL_WIDTH; x += 4U) {
			uint8_t packed = 0;
			uint8_t active_mask;

			ret = packbits_next_byte(&reader, &packed);
			if (ret != 0) {
				(void)fs_close(&reader.file);
				return ret;
			}
			active_mask = ordered_dither ?
					      sq_ssd1677_gray2_ordered_dither_bw_active_mask(
						      packed, x, y) :
					      sq_ssd1677_gray2_bw_active_mask(packed);
			apply_active_mask_to_row(row, x, active_mask);
		}
		ret = write_data(row, sizeof(row));
		if (ret != 0) {
			(void)fs_close(&reader.file);
			return ret;
		}
	}
	(void)fs_close(&reader.file);
	return 0;
}

static int stream_binbook_gray2_bw_page(const struct sq_vm_runtime_binbook_page *page,
					bool ordered_dither)
{
	int ret = validate_binbook_gray2_page(page);

	if (ret != 0) {
		return ret;
	}
	return stream_binbook_gray2_bw_plane(page, SSD1677_CMD_WRITE_RAM, ordered_dither);
}

static int stream_binbook_gray2_bw_previous_page(bool ordered_dither)
{
	int ret;

	if (!binbook_refresh_state.previous_page_valid) {
		return -EIO;
	}
	ret = validate_binbook_gray2_page(&binbook_previous_page);
	if (ret != 0) {
		return ret;
	}
	return stream_binbook_gray2_bw_plane(&binbook_previous_page, SSD1677_CMD_WRITE_RED_RAM,
					     ordered_dither);
}

static void draw_text_row(uint8_t line[ROW_BYTES], uint16_t y,
			  const struct sq_vm_runtime_display_op *op)
{
	bool text_black = !sq_display_color_is_set(op->u.text.color) ||
			  ssd1677_color_is_black(op->u.text.color);

	if (op->kind != SQ_VM_RUNTIME_DISPLAY_OP_TEXT || op->u.text.font_height <= 0 ||
	    op->x >= LOGICAL_WIDTH || op->y >= LOGICAL_HEIGHT) {
		return;
	}
	uint8_t scale = (uint8_t)(op->u.text.font_height / 7);
	if (scale == 0) {
		scale = 1;
	}
	uint16_t text_y = op->y < 0 ? 0 : (uint16_t)op->y;
	uint16_t cursor_x = op->x < 0 ? 0 : (uint16_t)op->x;

	for (size_t i = 0; op->u.text.text[i] != '\0'; ++i) {
		const uint8_t *glyph = glyph_for(op->u.text.text[i]);

		for (uint8_t glyph_row = 0; glyph_row < 7U; ++glyph_row) {
			for (uint8_t row = 0; row < scale; ++row) {
				uint16_t logical_y = text_y + (uint16_t)(glyph_row * scale) + row;

				for (uint8_t col = 0; col < 5U; ++col) {
					if ((glyph[glyph_row] & (0x10U >> col)) == 0U) {
						continue;
					}
					for (uint8_t dx = 0; dx < scale; ++dx) {
						uint16_t logical_x = cursor_x +
								     (uint16_t)(col * scale) + dx;
						uint16_t physical_x = 0;
						uint16_t physical_y = 0;

						if (logical_to_physical(logical_x, logical_y,
									&physical_x, &physical_y) &&
						    physical_y == y) {
							set_pixel(line, physical_x, text_black);
						}
					}
				}
			}
		}
		cursor_x += (uint16_t)(6U * scale);
		if (cursor_x >= LOGICAL_WIDTH) {
			return;
		}
	}
}

static void draw_rect_row(uint8_t line[ROW_BYTES], uint16_t y,
			  const struct sq_vm_runtime_display_op *op)
{
	bool has_fill = sq_display_color_is_set(op->u.rect.fill_color);
	bool has_stroke = sq_display_color_is_set(op->u.rect.stroke_color);
	bool fill_black = ssd1677_color_is_black(op->u.rect.fill_color);
	bool stroke_black = ssd1677_color_is_black(op->u.rect.stroke_color);
	int32_t left = op->x;
	int32_t top = op->y;
	int32_t right = op->x + op->u.rect.w;
	int32_t bottom = op->y + op->u.rect.h;

	if (op->kind != SQ_VM_RUNTIME_DISPLAY_OP_RECT || op->u.rect.w <= 0 || op->u.rect.h <= 0 ||
	    (!has_fill && !has_stroke)) {
		return;
	}
	if (left < 0) {
		left = 0;
	}
	if (top < 0) {
		top = 0;
	}
	if (right > LOGICAL_WIDTH) {
		right = LOGICAL_WIDTH;
	}
	if (bottom > LOGICAL_HEIGHT) {
		bottom = LOGICAL_HEIGHT;
	}
	if (left >= right || top >= bottom) {
		return;
	}

	uint16_t px0 = 0;
	uint16_t px1 = 0;
	bool is_edge_row = false;
	uint16_t edge_col_a = 0;
	uint16_t edge_col_b = 0;

	switch (LOGICAL_ROTATION) {
	case 0:
		if (y < (uint16_t)top || y >= (uint16_t)bottom) {
			return;
		}
		px0 = (uint16_t)left;
		px1 = (uint16_t)(right - 1);
		is_edge_row = (y == (uint16_t)top || y == (uint16_t)(bottom - 1));
		edge_col_a = (uint16_t)left;
		edge_col_b = (uint16_t)(right - 1);
		break;
	case 90:
		if (y < (uint16_t)(LOGICAL_WIDTH - right) ||
		    y > (uint16_t)(LOGICAL_WIDTH - 1 - left)) {
			return;
		}
		px0 = (uint16_t)top;
		px1 = (uint16_t)(bottom - 1);
		is_edge_row = (y == (uint16_t)(LOGICAL_WIDTH - right) ||
			       y == (uint16_t)(LOGICAL_WIDTH - 1 - left));
		edge_col_a = (uint16_t)top;
		edge_col_b = (uint16_t)(bottom - 1);
		break;
	case 180:
		if (y < (uint16_t)(LOGICAL_HEIGHT - bottom) ||
		    y > (uint16_t)(LOGICAL_HEIGHT - 1 - top)) {
			return;
		}
		px0 = (uint16_t)(LOGICAL_WIDTH - right);
		px1 = (uint16_t)(LOGICAL_WIDTH - 1 - left);
		is_edge_row = (y == (uint16_t)(LOGICAL_HEIGHT - bottom) ||
			       y == (uint16_t)(LOGICAL_HEIGHT - 1 - top));
		edge_col_a = (uint16_t)(LOGICAL_WIDTH - right);
		edge_col_b = (uint16_t)(LOGICAL_WIDTH - 1 - left);
		break;
	case 270:
		if (y < (uint16_t)left || y >= (uint16_t)right) {
			return;
		}
		px0 = (uint16_t)(LOGICAL_HEIGHT - bottom);
		px1 = (uint16_t)(LOGICAL_HEIGHT - 1 - top);
		is_edge_row = (y == (uint16_t)left || y == (uint16_t)(right - 1));
		edge_col_a = (uint16_t)(LOGICAL_HEIGHT - bottom);
		edge_col_b = (uint16_t)(LOGICAL_HEIGHT - 1 - top);
		break;
	default:
		return;
	}

	for (uint16_t px = px0;; ++px) {
		bool is_edge_col = (px == edge_col_a || px == edge_col_b);
		if (has_stroke && (is_edge_row || is_edge_col)) {
			set_pixel(line, px, stroke_black);
		} else if (has_fill) {
			set_pixel(line, px, fill_black);
		}
		if (px == px1) {
			break;
		}
	}
}

static inline void fb_set_pixel(uint16_t x, uint16_t y, bool black)
{
	if (x >= PANEL_WIDTH || y >= PANEL_HEIGHT) {
		return;
	}
	uint16_t ram_x = (uint16_t)(PANEL_WIDTH - 1U - x);
	size_t byte_idx = (size_t)y * ROW_BYTES + ram_x / 8U;
	uint8_t mask = (uint8_t)(0x80U >> (ram_x % 8U));

	if (black) {
		fb_framebuffer[byte_idx] &= (uint8_t)~mask;
	} else {
		fb_framebuffer[byte_idx] |= mask;
	}
}

void sq_display_backend_rasterize_clear(sq_display_color_t color)
{
	if (ssd1677_color_is_black(color)) {
		memset(fb_framebuffer, 0x00, FB_FRAMEBUFFER_SIZE);
	} else {
		memset(fb_framebuffer, 0xFF, FB_FRAMEBUFFER_SIZE);
	}
}

void sq_display_backend_rasterize_text(const char *text, int32_t x, int32_t y,
				       int32_t font_height, sq_display_color_t color)
{
	if (text == NULL || font_height <= 0) {
		return;
	}
	bool text_black = sq_display_color_is_black(color);
	uint8_t scale = (uint8_t)(font_height / 7);

	if (scale == 0) {
		scale = 1;
	}
	uint16_t text_y = y < 0 ? 0 : (uint16_t)y;
	uint16_t cursor_x = x < 0 ? 0 : (uint16_t)x;

	for (size_t i = 0; text[i] != '\0'; ++i) {
		const uint8_t *glyph = glyph_for(text[i]);

		for (uint8_t glyph_row = 0; glyph_row < 7U; ++glyph_row) {
			for (uint8_t row = 0; row < scale; ++row) {
				uint16_t logical_y = text_y + (uint16_t)(glyph_row * scale) + row;

				if (logical_y >= LOGICAL_HEIGHT) {
					continue;
				}
				for (uint8_t col = 0; col < 5U; ++col) {
					if ((glyph[glyph_row] & (0x10U >> col)) == 0U) {
						continue;
					}
					for (uint8_t dx = 0; dx < scale; ++dx) {
						uint16_t logical_x = cursor_x +
								     (uint16_t)(col * scale) + dx;
						uint16_t physical_x = 0;
						uint16_t physical_y = 0;

						if (logical_to_physical(logical_x, logical_y,
									&physical_x, &physical_y)) {
							fb_set_pixel(physical_x, physical_y,
								     text_black);
						}
					}
				}
			}
		}
		cursor_x += (uint16_t)(6U * scale);
		if (cursor_x >= LOGICAL_WIDTH) {
			return;
		}
	}
}

void sq_display_backend_rasterize_rect(int32_t x, int32_t y, int32_t w, int32_t h,
				       sq_display_color_t fill_color, sq_display_color_t stroke_color)
{
	if (w <= 0 || h <= 0) {
		return;
	}
	bool has_fill = sq_display_color_is_set(fill_color);
	bool has_stroke = sq_display_color_is_set(stroke_color);

	if (!has_fill && !has_stroke) {
		return;
	}
	bool fill_black = ssd1677_color_is_black(fill_color);
	bool stroke_black = ssd1677_color_is_black(stroke_color);
	int32_t left = x < 0 ? 0 : x;
	int32_t top = y < 0 ? 0 : y;
	int32_t right = x + w;
	int32_t bottom = y + h;

	if (right > (int32_t)LOGICAL_WIDTH) {
		right = (int32_t)LOGICAL_WIDTH;
	}
	if (bottom > (int32_t)LOGICAL_HEIGHT) {
		bottom = (int32_t)LOGICAL_HEIGHT;
	}
	if (left >= right || top >= bottom) {
		return;
	}
	for (int32_t py = top; py < bottom; ++py) {
		for (int32_t px = left; px < right; ++px) {
			bool is_edge = (py == top || py == bottom - 1 ||
					px == left || px == right - 1);
			bool draw = false;
			bool black = false;

			if (has_stroke && is_edge) {
				draw = true;
				black = stroke_black;
			} else if (has_fill) {
				draw = true;
				black = fill_black;
			}
			if (draw) {
				uint16_t physical_x = 0;
				uint16_t physical_y = 0;

				if (logical_to_physical((uint16_t)px, (uint16_t)py,
							&physical_x, &physical_y)) {
					fb_set_pixel(physical_x, physical_y, black);
				}
			}
		}
	}
}

static int decompress_binbook_gray2_to_fb(const struct sq_vm_runtime_binbook_page *page,
					  bool msb_plane)
{
	struct packbits_reader reader = {0};
	int ret;

	fs_file_t_init(&reader.file);
	ret = fs_open(&reader.file, page->path, FS_O_READ);
	if (ret != 0) {
		return ret;
	}
	ret = fs_seek(&reader.file, (off_t)page->blob_offset, FS_SEEK_SET);
	if (ret != 0) {
		(void)fs_close(&reader.file);
		return ret;
	}
	reader.compressed_left = page->compressed_size;
	for (uint16_t y = 0; y < PANEL_HEIGHT; ++y) {
		size_t row_offset = (size_t)y * ROW_BYTES;

		memset(&fb_framebuffer[row_offset], 0xff, ROW_BYTES);
		for (uint16_t x = 0; x < PANEL_WIDTH; x += 4U) {
			uint8_t packed = 0;
			uint8_t active_mask;

			ret = packbits_next_byte(&reader, &packed);
			if (ret != 0) {
				(void)fs_close(&reader.file);
				return ret;
			}
			active_mask = msb_plane ? sq_ssd1677_gray2_msb_active_mask(packed)
						: sq_ssd1677_gray2_lsb_active_mask(packed);
			for (uint8_t pixel = 0; pixel < 4U; ++pixel) {
				if ((active_mask & (0x80U >> pixel)) != 0U) {
					uint16_t px = x + pixel;

					if (px < PANEL_WIDTH) {
						uint16_t ram_x = (uint16_t)(PANEL_WIDTH - 1U - px);
						size_t byte_idx = row_offset + ram_x / 8U;
						uint8_t mask = (uint8_t)(0x80U >> (ram_x % 8U));

						fb_framebuffer[byte_idx] &= (uint8_t)~mask;
					}
				}
			}
		}
	}
	(void)fs_close(&reader.file);
	return 0;
}

static int decompress_binbook_gray2_bw_to_fb(const struct sq_vm_runtime_binbook_page *page,
					     bool ordered_dither)
{
	struct packbits_reader reader = {0};
	int ret;

	fs_file_t_init(&reader.file);
	ret = fs_open(&reader.file, page->path, FS_O_READ);
	if (ret != 0) {
		return ret;
	}
	ret = fs_seek(&reader.file, (off_t)page->blob_offset, FS_SEEK_SET);
	if (ret != 0) {
		(void)fs_close(&reader.file);
		return ret;
	}
	reader.compressed_left = page->compressed_size;
	for (uint16_t y = 0; y < PANEL_HEIGHT; ++y) {
		size_t row_offset = (size_t)y * ROW_BYTES;

		memset(&fb_framebuffer[row_offset], 0xff, ROW_BYTES);
		for (uint16_t x = 0; x < PANEL_WIDTH; x += 4U) {
			uint8_t packed = 0;
			uint8_t active_mask;

			ret = packbits_next_byte(&reader, &packed);
			if (ret != 0) {
				(void)fs_close(&reader.file);
				return ret;
			}
			active_mask = ordered_dither ?
					      sq_ssd1677_gray2_ordered_dither_bw_active_mask(
						      packed, x, y) :
					      sq_ssd1677_gray2_bw_active_mask(packed);
			for (uint8_t pixel = 0; pixel < 4U; ++pixel) {
				if ((active_mask & (0x80U >> pixel)) != 0U) {
					uint16_t px = x + pixel;

					if (px < PANEL_WIDTH) {
						uint16_t ram_x = (uint16_t)(PANEL_WIDTH - 1U - px);
						size_t byte_idx = row_offset + ram_x / 8U;
						uint8_t mask = (uint8_t)(0x80U >> (ram_x % 8U));

						fb_framebuffer[byte_idx] &= (uint8_t)~mask;
					}
				}
			}
		}
	}
	(void)fs_close(&reader.file);
	return 0;
}

void sq_display_backend_rasterize_binbook(const struct sq_vm_runtime_binbook_page *page)
{
	if (page == NULL || page->path[0] == '\0') {
		return;
	}
	if (page->pixel_format == BINBOOK_PIXEL_FORMAT_GRAY2_PACKED &&
	    page->compression_method == BINBOOK_COMPRESSION_RLE_PACKBITS &&
	    page->stored_width == PANEL_WIDTH && page->stored_height == PANEL_HEIGHT &&
	    page->uncompressed_size == BINBOOK_GRAY2_PAGE_BYTES && page->compressed_size > 0) {
		sq_debug_log_append("%lld:decompress_start:cs=%lu", (long long)k_uptime_get(),
				    (unsigned long)page->compressed_size);
		(void)decompress_binbook_gray2_to_fb(page, true);
		sq_debug_log_append("%lld:decompress_msb_done", (long long)k_uptime_get());
		(void)decompress_binbook_gray2_to_fb(page, false);
		sq_debug_log_append("%lld:decompress_lsb_done", (long long)k_uptime_get());
		(void)decompress_binbook_gray2_bw_to_fb(page, true);
		sq_debug_log_append("%lld:decompress_bw_done", (long long)k_uptime_get());
	}
}

static uint16_t op_y_min(const struct sq_vm_runtime_display_op *op)
{
	int32_t left, top;

	switch (op->kind) {
	case SQ_VM_RUNTIME_DISPLAY_OP_CLEAR:
		return 0;
	case SQ_VM_RUNTIME_DISPLAY_OP_TEXT: {
		if (op->u.text.font_height <= 0) {
			return PANEL_HEIGHT;
		}
		uint8_t scale = (uint8_t)(op->u.text.font_height / 7);

		if (scale == 0) {
			scale = 1;
		}
		switch (LOGICAL_ROTATION) {
		case 0:
			return op->y < 0 ? 0 : (uint16_t)op->y;
		case 90:
			return (uint16_t)(LOGICAL_WIDTH - (op->x + (int32_t)strlen(op->u.text.text) * 6 * scale));
		case 180:
			return (uint16_t)(LOGICAL_HEIGHT - (op->y + 7 * scale));
		case 270:
			return op->x < 0 ? 0 : (uint16_t)op->x;
		default:
			return PANEL_HEIGHT;
		}
	}
	case SQ_VM_RUNTIME_DISPLAY_OP_RECT:
		left = op->x;
		top = op->y;
		switch (LOGICAL_ROTATION) {
		case 0:
			return top < 0 ? 0 : (uint16_t)top;
		case 90:
			return (uint16_t)(LOGICAL_WIDTH - (left + op->u.rect.w));
		case 180:
			return (uint16_t)(LOGICAL_HEIGHT - (top + op->u.rect.h));
		case 270:
			return left < 0 ? 0 : (uint16_t)left;
		default:
			return PANEL_HEIGHT;
		}
	default:
		return PANEL_HEIGHT;
	}
}

static uint16_t op_y_max(const struct sq_vm_runtime_display_op *op)
{
	int32_t left, top, val;

	switch (op->kind) {
	case SQ_VM_RUNTIME_DISPLAY_OP_CLEAR:
		return PANEL_HEIGHT;
	case SQ_VM_RUNTIME_DISPLAY_OP_TEXT: {
		if (op->u.text.font_height <= 0) {
			return 0;
		}
		uint8_t scale = (uint8_t)(op->u.text.font_height / 7);

		if (scale == 0) {
			scale = 1;
		}
		switch (LOGICAL_ROTATION) {
		case 0: {
			uint16_t text_y = op->y < 0 ? 0 : (uint16_t)op->y;
			uint16_t h = (uint16_t)(7U * scale);

			return text_y + h > PANEL_HEIGHT ? PANEL_HEIGHT : text_y + h;
		}
		case 90:
			return (uint16_t)(LOGICAL_WIDTH - op->x);
		case 180:
			return (uint16_t)(LOGICAL_HEIGHT - op->y);
		case 270: {
			int32_t text_w = (int32_t)strlen(op->u.text.text) * 6 * scale;

			val = op->x + text_w;
			return val > PANEL_HEIGHT ? PANEL_HEIGHT : (uint16_t)val;
		}
		default:
			return 0;
		}
	}
	case SQ_VM_RUNTIME_DISPLAY_OP_RECT: {
		left = op->x;
		top = op->y;
		switch (LOGICAL_ROTATION) {
		case 0:
			val = top + op->u.rect.h;
			return val > PANEL_HEIGHT ? PANEL_HEIGHT : (uint16_t)val;
		case 90:
			return (uint16_t)(LOGICAL_WIDTH - left);
		case 180:
			return (uint16_t)(LOGICAL_HEIGHT - top);
		case 270:
			val = left + op->u.rect.w;
			return val > PANEL_HEIGHT ? PANEL_HEIGHT : (uint16_t)val;
		default:
			return 0;
		}
	}
	default:
		return 0;
	}
}

static int refresh_display(bool *observed_busy)
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
	return wait_ready("refresh", observed_busy);
}

static int refresh_partial_display(bool *observed_busy)
{
	const uint8_t display_update[] = {0x00, 0x00};
	const uint8_t update = SSD1677_UPDATE_PARTIAL;
	int ret;

	sq_debug_log_append("%lld:refresh_cmd_start", (long long)k_uptime_get());
	ret = write_command_data(SSD1677_CMD_DISPLAY_UPDATE_CTRL, display_update,
				 sizeof(display_update));
	if (ret != 0) {
		return ret;
	}
	ret = write_command_data(SSD1677_CMD_UPDATE_CTRL2, &update, sizeof(update));
	if (ret != 0) {
		return ret;
	}
	ret = write_command(SSD1677_CMD_MASTER_ACTIVATION);
	if (ret != 0) {
		return ret;
	}
	sq_debug_log_append("%lld:refresh_wait_start", (long long)k_uptime_get());
	return wait_ready("refresh-partial", observed_busy);
}

static int refresh_grayscale_display(bool *observed_busy)
{
	const uint8_t display_update[] = {0x00, 0x00};
	const uint8_t update = SSD1677_UPDATE_GRAYSCALE;
	int ret = write_command_data(SSD1677_CMD_DISPLAY_UPDATE_CTRL, display_update,
				     sizeof(display_update));

	if (ret != 0) {
		return ret;
	}
	ret = write_command_data(SSD1677_CMD_UPDATE_CTRL2, &update, sizeof(update));
	if (ret != 0) {
		return ret;
	}
	ret = write_command(SSD1677_CMD_MASTER_ACTIVATION);
	if (ret != 0) {
		return ret;
	}
	return wait_ready("refresh-grayscale", observed_busy);
}

static int refresh_binbook_bw_partial_display(bool *observed_busy)
{
	const uint8_t display_update[] = {0x00, 0x00};
	const uint8_t update = SSD1677_UPDATE_PARTIAL;
	int ret = write_command_data(SSD1677_CMD_DISPLAY_UPDATE_CTRL, display_update,
				     sizeof(display_update));

	if (ret != 0) {
		return ret;
	}
	ret = write_command_data(SSD1677_CMD_UPDATE_CTRL2, &update, sizeof(update));
	if (ret != 0) {
		return ret;
	}
	ret = write_command(SSD1677_CMD_MASTER_ACTIVATION);
	if (ret != 0) {
		return ret;
	}
	return wait_ready("refresh-partial", observed_busy);
}

struct ssd1677_probe_window {
	uint16_t x;
	uint16_t y;
	uint16_t w;
	uint16_t h;
};

static int stream_solid_window(uint8_t command, const struct ssd1677_probe_window *window,
			       uint8_t value)
{
	size_t window_row_bytes;
	int ret;

	if (window == NULL || window->w == 0U || window->h == 0U ||
	    window->x >= PANEL_WIDTH || window->y >= PANEL_HEIGHT ||
	    window->w > PANEL_WIDTH - window->x || window->h > PANEL_HEIGHT - window->y) {
		return -EINVAL;
	}
	window_row_bytes = (window->w + 7U) / 8U;
	if (window_row_bytes > sizeof(row)) {
		return -EINVAL;
	}
	memset(row, value, window_row_bytes);
	ret = set_window(window->x, window->y, window->x + window->w - 1U,
			 window->y + window->h - 1U);
	if (ret != 0) {
		return ret;
	}
	ret = write_command(command);
	if (ret != 0) {
		return ret;
	}
	for (uint16_t y = 0; y < window->h; ++y) {
		ret = write_data(row, window_row_bytes);
		if (ret != 0) {
			return ret;
		}
	}
	return 0;
}

static int clear_bw_display(void)
{
	static const struct ssd1677_probe_window full_window = {
		.x = 0,
		.y = 0,
		.w = PANEL_WIDTH,
		.h = PANEL_HEIGHT,
	};
	bool observed_busy = false;
	int ret = stream_solid_window(SSD1677_CMD_WRITE_RED_RAM, &full_window, 0xff);

	if (ret == 0) {
		ret = stream_solid_window(SSD1677_CMD_WRITE_RAM, &full_window, 0xff);
	}
	if (ret == 0) {
		ret = refresh_display(&observed_busy);
	}
	return ret;
}

static int stream_probe_windows(const struct ssd1677_probe_window *windows, size_t window_count)
{
	int ret;

	if (windows == NULL || window_count == 0U) {
		return -EINVAL;
	}
	for (size_t i = 0; i < window_count; ++i) {
		ret = stream_solid_window(SSD1677_CMD_WRITE_RED_RAM, &windows[i], 0xff);
		if (ret != 0) {
			return ret;
		}
		ret = stream_solid_window(SSD1677_CMD_WRITE_RAM, &windows[i], 0x00);
		if (ret != 0) {
			return ret;
		}
	}
	return 0;
}

static int full_frame_probe(sq_display_color_t color, bool *observed_busy)
{
	int ret;

	sq_display_backend_rasterize_clear(color);
	ret = sq_display_backend_flush_framebuffer(SQ_VM_RUNTIME_DISPLAY_REFRESH_FULL);
	return ret;
}

int sq_display_backend_window_probe(const char *pattern)
{
	static const struct ssd1677_probe_window top_band[] = {
		{.x = 0, .y = 0, .w = PANEL_WIDTH, .h = 80},
	};
	static const struct ssd1677_probe_window bottom_band[] = {
		{.x = 0, .y = PANEL_HEIGHT - 80U, .w = PANEL_WIDTH, .h = 80},
	};
	static const struct ssd1677_probe_window left_band[] = {
		{.x = 0, .y = 0, .w = 128, .h = PANEL_HEIGHT},
	};
	static const struct ssd1677_probe_window right_band[] = {
		{.x = PANEL_WIDTH - 128U, .y = 0, .w = 128, .h = PANEL_HEIGHT},
	};
	static const struct ssd1677_probe_window corners[] = {
		{.x = 0, .y = 0, .w = 96, .h = 96},
		{.x = PANEL_WIDTH - 96U, .y = 0, .w = 96, .h = 96},
		{.x = 0, .y = PANEL_HEIGHT - 96U, .w = 96, .h = 96},
		{.x = PANEL_WIDTH - 96U, .y = PANEL_HEIGHT - 96U, .w = 96, .h = 96},
	};
	const struct ssd1677_probe_window *windows = NULL;
	size_t window_count = 0;
	bool observed_busy = false;
	int ret;

	if (pattern == NULL) {
		return -EINVAL;
	}
	if (strcmp(pattern, "full-black") == 0) {
		windows = NULL;
		window_count = 0;
	} else if (strcmp(pattern, "full-white") == 0) {
		windows = NULL;
		window_count = 0;
	} else if (strcmp(pattern, "top-band") == 0) {
		windows = top_band;
		window_count = ARRAY_SIZE(top_band);
	} else if (strcmp(pattern, "bottom-band") == 0) {
		windows = bottom_band;
		window_count = ARRAY_SIZE(bottom_band);
	} else if (strcmp(pattern, "left-band") == 0) {
		windows = left_band;
		window_count = ARRAY_SIZE(left_band);
	} else if (strcmp(pattern, "right-band") == 0) {
		windows = right_band;
		window_count = ARRAY_SIZE(right_band);
	} else if (strcmp(pattern, "corners") == 0) {
		windows = corners;
		window_count = ARRAY_SIZE(corners);
	} else {
		return -EINVAL;
	}
	ret = configure_display();
	if (ret != 0) {
		return ret;
	}
	if (display_mode != SSD1677_DISPLAY_MODE_BW) {
		ret = epaper_init();
		if (ret != 0) {
			display_mode = SSD1677_DISPLAY_MODE_NONE;
			return ret;
		}
	}
	if (strcmp(pattern, "full-black") == 0) {
		ret = full_frame_probe(SQ_DISPLAY_COLOR_BLACK, &observed_busy);
	} else if (strcmp(pattern, "full-white") == 0) {
		ret = full_frame_probe(SQ_DISPLAY_COLOR_WHITE, &observed_busy);
	} else {
		ret = clear_bw_display();
		if (ret == 0) {
			ret = stream_probe_windows(windows, window_count);
		}
		if (ret == 0) {
			ret = refresh_partial_display(&observed_busy);
		}
	}
	binbook_remember_previous_page(NULL);
	composed_remember_previous_ops(NULL, 0);
	LOG_INF("display window probe pattern=%s result=%d busy_observed=%d", pattern, ret,
		observed_busy);
	return ret;
}

static int configure_display(void)
{
	if (!device_is_ready(spi_dev) || !gpio_is_ready_dt(&cs_gpio) ||
	    !gpio_is_ready_dt(&dc_gpio) || !gpio_is_ready_dt(&reset_gpio) ||
	    !gpio_is_ready_dt(&busy_gpio)) {
		return -ENODEV;
	}
	int ret = gpio_pin_configure_dt(&dc_gpio, GPIO_OUTPUT_INACTIVE);

	if (ret != 0) {
		return ret;
	}
	ret = gpio_pin_configure_dt(&reset_gpio, GPIO_OUTPUT_INACTIVE);
	if (ret != 0) {
		return ret;
	}
	ret = gpio_pin_configure_dt(&cs_gpio, GPIO_OUTPUT_INACTIVE);
	if (ret != 0) {
		return ret;
	}
	return gpio_pin_configure_dt(&busy_gpio, GPIO_INPUT);
}

void sq_display_backend_reset(void)
{
	memset(&binbook_previous_page, 0, sizeof(binbook_previous_page));
	sq_ssd1677_binbook_refresh_reset(&binbook_refresh_state);
}

const uint8_t *sq_display_backend_framebuffer(void)
{
	return fb_framebuffer;
}

size_t sq_display_backend_framebuffer_size(void)
{
	return FB_FRAMEBUFFER_SIZE;
}

int sq_display_backend_flush_framebuffer(enum sq_vm_runtime_display_refresh_mode mode)
{
	bool observed_busy = false;
	int ret;

	ret = configure_display();
	if (ret != 0) {
		return ret;
	}
	if (display_mode != SSD1677_DISPLAY_MODE_BW) {
		ret = epaper_init();
		if (ret != 0) {
			LOG_ERR("display init failed: %d", ret);
			display_mode = SSD1677_DISPLAY_MODE_NONE;
			return ret;
		}
	}
	ret = set_full_window();
	if (ret != 0) {
		return ret;
	}
	ret = write_command(SSD1677_CMD_WRITE_RAM);
	if (ret != 0) {
		return ret;
	}
	ret = write_data(fb_framebuffer, FB_FRAMEBUFFER_SIZE);
	if (ret != 0) {
		return ret;
	}
	if (mode == SQ_VM_RUNTIME_DISPLAY_REFRESH_FAST_1BPP) {
		ret = refresh_partial_display(&observed_busy);
	} else {
		ret = refresh_display(&observed_busy);
	}
	return ret;
}

int sq_display_backend_flush(const struct sq_vm_runtime_display_op *ops, size_t op_count,
			     enum sq_vm_runtime_display_refresh_mode refresh_request,
			     const struct sq_vm_runtime_binbook_page *binbook_page,
			     bool *needs_phase2)
{
	ARG_UNUSED(ops);
	ARG_UNUSED(op_count);
	ARG_UNUSED(refresh_request);
	ARG_UNUSED(binbook_page);
	if (needs_phase2 != NULL) {
		*needs_phase2 = false;
	}
	return 0;
}

#else

#if !defined(CONFIG_ZTEST)
int sq_display_backend_flush(const struct sq_vm_runtime_display_op *ops, size_t op_count,
			     enum sq_vm_runtime_display_refresh_mode refresh_mode,
			     const struct sq_vm_runtime_binbook_page *binbook_page,
			     bool *needs_phase2)
{
	ARG_UNUSED(ops);
	ARG_UNUSED(op_count);
	ARG_UNUSED(refresh_mode);
	ARG_UNUSED(binbook_page);
	if (needs_phase2 != NULL) {
		*needs_phase2 = false;
	}
	return 0;
}

void sq_display_backend_set_phase2(bool phase2)
{
	ARG_UNUSED(phase2);
}

int sq_display_backend_window_probe(const char *pattern)
{
	ARG_UNUSED(pattern);
	return -ENOTSUP;
}

void sq_display_backend_reset(void)
{
}

void sq_display_backend_rasterize_clear(sq_display_color_t color)
{
	ARG_UNUSED(color);
}

void sq_display_backend_rasterize_text(const char *text, int32_t x, int32_t y,
				       int32_t font_height, sq_display_color_t color)
{
	ARG_UNUSED(text);
	ARG_UNUSED(x);
	ARG_UNUSED(y);
	ARG_UNUSED(font_height);
	ARG_UNUSED(color);
}

void sq_display_backend_rasterize_rect(int32_t x, int32_t y, int32_t w, int32_t h,
				       sq_display_color_t fill, sq_display_color_t stroke)
{
	ARG_UNUSED(x);
	ARG_UNUSED(y);
	ARG_UNUSED(w);
	ARG_UNUSED(h);
	ARG_UNUSED(fill);
	ARG_UNUSED(stroke);
}

void sq_display_backend_rasterize_binbook(const struct sq_vm_runtime_binbook_page *page)
{
	ARG_UNUSED(page);
}

int sq_display_backend_flush_framebuffer(enum sq_vm_runtime_display_refresh_mode mode)
{
	ARG_UNUSED(mode);
	return 0;
}

const uint8_t *sq_display_backend_framebuffer(void)
{
	return NULL;
}

size_t sq_display_backend_framebuffer_size(void)
{
	return 0;
}
#endif

#endif
