/*
 * ESP32-C3 SuperMini scan-only hardware test.
 *
 * This uses Espressif's ESP-IDF Wi-Fi station scan path without configured
 * credentials and without attempting to connect. It isolates whether the board
 * can receive nearby AP beacons independently of SquidScript, Rust, and
 * esp-radio.
 */

#include <string.h>

#include "esp_event.h"
#include "esp_log.h"
#include "esp_mac.h"
#include "esp_netif.h"
#include "esp_wifi.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "nvs_flash.h"

#define HWTEST_SCAN_MAX_APS 20

static const char *TAG = "scan_hwtest";
static wifi_ap_record_t scan_records[HWTEST_SCAN_MAX_APS];

static const char *authmode_name(wifi_auth_mode_t authmode) {
  switch (authmode) {
  case WIFI_AUTH_OPEN:
    return "OPEN";
  case WIFI_AUTH_WEP:
    return "WEP";
  case WIFI_AUTH_WPA_PSK:
    return "WPA_PSK";
  case WIFI_AUTH_WPA2_PSK:
    return "WPA2_PSK";
  case WIFI_AUTH_WPA_WPA2_PSK:
    return "WPA_WPA2_PSK";
  case WIFI_AUTH_ENTERPRISE:
    return "ENTERPRISE";
  case WIFI_AUTH_WPA3_PSK:
    return "WPA3_PSK";
  case WIFI_AUTH_WPA2_WPA3_PSK:
    return "WPA2_WPA3_PSK";
  case WIFI_AUTH_WAPI_PSK:
    return "WAPI_PSK";
  case WIFI_AUTH_OWE:
    return "OWE";
  case WIFI_AUTH_WPA3_ENT_192:
    return "WPA3_ENT_192";
  default:
    return "UNKNOWN";
  }
}

static void init_nvs(void) {
  esp_err_t ret = nvs_flash_init();
  if (ret == ESP_ERR_NVS_NO_FREE_PAGES ||
      ret == ESP_ERR_NVS_NEW_VERSION_FOUND) {
    ESP_ERROR_CHECK(nvs_flash_erase());
    ret = nvs_flash_init();
  }
  ESP_ERROR_CHECK(ret);
}

static void init_wifi_scan(void) {
  ESP_ERROR_CHECK(esp_netif_init());
  ESP_ERROR_CHECK(esp_event_loop_create_default());
  esp_netif_t *sta_netif = esp_netif_create_default_wifi_sta();
  ESP_ERROR_CHECK(sta_netif == NULL ? ESP_FAIL : ESP_OK);

  wifi_init_config_t cfg = WIFI_INIT_CONFIG_DEFAULT();
  ESP_ERROR_CHECK(esp_wifi_init(&cfg));
  ESP_ERROR_CHECK(esp_wifi_set_ps(WIFI_PS_NONE));
  ESP_ERROR_CHECK(esp_wifi_set_mode(WIFI_MODE_STA));
  ESP_ERROR_CHECK(esp_wifi_start());

  uint8_t mac[6] = {0};
  ESP_ERROR_CHECK(esp_wifi_get_mac(WIFI_IF_STA, mac));
  ESP_LOGI(TAG, "scan-only hardware test started");
  ESP_LOGI(TAG, "STA MAC:" MACSTR, MAC2STR(mac));
}

static void run_scan(void) {
  wifi_scan_config_t scan_config = {
      .ssid = NULL,
      .bssid = NULL,
      .channel = 0,
      .show_hidden = true,
      .scan_type = WIFI_SCAN_TYPE_ACTIVE,
  };

  ESP_LOGI(TAG, "starting unfiltered scan");
  ESP_ERROR_CHECK(esp_wifi_scan_start(&scan_config, true));

  uint16_t ap_count = HWTEST_SCAN_MAX_APS;
  memset(scan_records, 0, sizeof(scan_records));
  ESP_ERROR_CHECK(esp_wifi_scan_get_ap_records(&ap_count, scan_records));
  ESP_LOGI(TAG, "scan found %u AP record(s)", ap_count);

  for (uint16_t i = 0; i < ap_count; i++) {
    ESP_LOGI(TAG,
             "scan[%u] ssid_len:%u bssid:" MACSTR " channel:%d rssi:%d auth:%s",
             i, (unsigned)strlen((char *)scan_records[i].ssid),
             MAC2STR(scan_records[i].bssid), scan_records[i].primary,
             scan_records[i].rssi, authmode_name(scan_records[i].authmode));
  }
}

void app_main(void) {
  init_nvs();
  ESP_LOGI(TAG, "ESP32-C3 scan-only hardware test using ESP-IDF");
  init_wifi_scan();

  while (true) {
    run_scan();
    vTaskDelay(pdMS_TO_TICKS(10000));
  }
}
