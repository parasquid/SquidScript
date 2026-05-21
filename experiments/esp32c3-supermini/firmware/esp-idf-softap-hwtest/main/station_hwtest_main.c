/*
 * ESP32-C3 SuperMini station-mode hardware test.
 *
 * This intentionally uses Espressif's ESP-IDF Wi-Fi station path to check
 * whether the board can receive and join an existing AP independently of
 * SquidScript's Rust firmware and esp-radio path.
 */

#include <string.h>

#include "esp_event.h"
#include "esp_log.h"
#include "esp_mac.h"
#include "esp_netif.h"
#include "esp_wifi.h"
#include "freertos/FreeRTOS.h"
#include "freertos/event_groups.h"
#include "freertos/task.h"
#include "hwtest_credentials.h"
#include "nvs_flash.h"

#define HWTEST_MAXIMUM_RETRY 10
#define HWTEST_SCAN_MAX_APS 20

static const char *TAG = "station_hwtest";
static EventGroupHandle_t wifi_event_group;
static int retry_count;
static wifi_ap_record_t scan_records[HWTEST_SCAN_MAX_APS];

static const int WIFI_CONNECTED_BIT = BIT0;
static const int WIFI_FAIL_BIT = BIT1;

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

static const char *disconnect_reason_name(uint8_t reason) {
  switch (reason) {
  case WIFI_REASON_AUTH_EXPIRE:
    return "AUTH_EXPIRE";
  case WIFI_REASON_AUTH_LEAVE:
    return "AUTH_LEAVE";
  case WIFI_REASON_ASSOC_EXPIRE:
    return "ASSOC_EXPIRE";
  case WIFI_REASON_ASSOC_TOOMANY:
    return "ASSOC_TOOMANY";
  case WIFI_REASON_NOT_AUTHED:
    return "NOT_AUTHED";
  case WIFI_REASON_NOT_ASSOCED:
    return "NOT_ASSOCED";
  case WIFI_REASON_ASSOC_LEAVE:
    return "ASSOC_LEAVE";
  case WIFI_REASON_ASSOC_NOT_AUTHED:
    return "ASSOC_NOT_AUTHED";
  case WIFI_REASON_4WAY_HANDSHAKE_TIMEOUT:
    return "4WAY_HANDSHAKE_TIMEOUT";
  case WIFI_REASON_NO_AP_FOUND:
    return "NO_AP_FOUND";
  case WIFI_REASON_HANDSHAKE_TIMEOUT:
    return "HANDSHAKE_TIMEOUT";
  case WIFI_REASON_CONNECTION_FAIL:
    return "CONNECTION_FAIL";
  default:
    return "UNKNOWN";
  }
}

static void wifi_event_handler(
    void *arg,
    esp_event_base_t event_base,
    int32_t event_id,
    void *event_data) {
  (void)arg;

  if (event_base == WIFI_EVENT && event_id == WIFI_EVENT_STA_START) {
    ESP_LOGI(TAG, "station start");
  } else if (event_base == WIFI_EVENT &&
             event_id == WIFI_EVENT_STA_DISCONNECTED) {
    wifi_event_sta_disconnected_t *event =
        (wifi_event_sta_disconnected_t *)event_data;
    ESP_LOGI(TAG, "station disconnected; reason=%d (%s)", event->reason,
             disconnect_reason_name(event->reason));
    if (retry_count < HWTEST_MAXIMUM_RETRY) {
      retry_count++;
      ESP_LOGI(TAG, "retrying connection attempt %d/%d", retry_count,
               HWTEST_MAXIMUM_RETRY);
      ESP_ERROR_CHECK(esp_wifi_connect());
    } else {
      xEventGroupSetBits(wifi_event_group, WIFI_FAIL_BIT);
    }
  } else if (event_base == IP_EVENT && event_id == IP_EVENT_STA_GOT_IP) {
    ip_event_got_ip_t *event = (ip_event_got_ip_t *)event_data;
    retry_count = 0;
    ESP_LOGI(TAG, "got ip:" IPSTR, IP2STR(&event->ip_info.ip));
    ESP_LOGI(TAG, "gateway:" IPSTR, IP2STR(&event->ip_info.gw));
    ESP_LOGI(TAG, "netmask:" IPSTR, IP2STR(&event->ip_info.netmask));
    xEventGroupSetBits(wifi_event_group, WIFI_CONNECTED_BIT);
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

static void log_station_scan(void) {
  wifi_scan_config_t scan_config = {
      .ssid = (uint8_t *)HWTEST_STA_SSID,
      .bssid = NULL,
      .channel = 0,
      .show_hidden = true,
      .scan_type = WIFI_SCAN_TYPE_ACTIVE,
  };

  ESP_LOGI(TAG, "scanning for configured target SSID");
  ESP_ERROR_CHECK(esp_wifi_scan_start(&scan_config, true));

  uint16_t ap_count = HWTEST_SCAN_MAX_APS;
  memset(scan_records, 0, sizeof(scan_records));
  ESP_ERROR_CHECK(esp_wifi_scan_get_ap_records(&ap_count, scan_records));
  ESP_LOGI(TAG, "scan found %u matching AP record(s)", ap_count);

  for (uint16_t i = 0; i < ap_count; i++) {
    ESP_LOGI(TAG,
             "scan[%u] ssid_len:%u bssid:" MACSTR " channel:%d rssi:%d auth:%s",
             i, (unsigned)strlen((char *)scan_records[i].ssid),
             MAC2STR(scan_records[i].bssid), scan_records[i].primary,
             scan_records[i].rssi,
             authmode_name(scan_records[i].authmode));
  }
}

static void wifi_init_station(void) {
  wifi_event_group = xEventGroupCreate();

  ESP_ERROR_CHECK(esp_netif_init());
  ESP_ERROR_CHECK(esp_event_loop_create_default());
  esp_netif_t *sta_netif = esp_netif_create_default_wifi_sta();
  ESP_ERROR_CHECK(sta_netif == NULL ? ESP_FAIL : ESP_OK);

  wifi_init_config_t cfg = WIFI_INIT_CONFIG_DEFAULT();
  ESP_ERROR_CHECK(esp_wifi_init(&cfg));
  ESP_ERROR_CHECK(esp_wifi_set_ps(WIFI_PS_NONE));

  ESP_ERROR_CHECK(esp_event_handler_instance_register(
      WIFI_EVENT, ESP_EVENT_ANY_ID, &wifi_event_handler, NULL, NULL));
  ESP_ERROR_CHECK(esp_event_handler_instance_register(
      IP_EVENT, IP_EVENT_STA_GOT_IP, &wifi_event_handler, NULL, NULL));

  wifi_config_t wifi_config = {
      .sta =
          {
              .ssid = HWTEST_STA_SSID,
              .password = HWTEST_STA_PASSWORD,
              .threshold.authmode = WIFI_AUTH_WPA_PSK,
              .sae_pwe_h2e = WPA3_SAE_PWE_BOTH,
          },
  };

  ESP_ERROR_CHECK(esp_wifi_set_mode(WIFI_MODE_STA));
  ESP_ERROR_CHECK(esp_wifi_set_config(WIFI_IF_STA, &wifi_config));
  ESP_ERROR_CHECK(esp_wifi_start());
  log_station_scan();
  ESP_ERROR_CHECK(esp_wifi_connect());

  uint8_t mac[6] = {0};
  ESP_ERROR_CHECK(esp_wifi_get_mac(WIFI_IF_STA, mac));
  ESP_LOGI(TAG, "Station hardware test started");
  ESP_LOGI(TAG, "target SSID length:%u", (unsigned)strlen(HWTEST_STA_SSID));
  ESP_LOGI(TAG, "password length:%u", (unsigned)strlen(HWTEST_STA_PASSWORD));
  ESP_LOGI(TAG, "auth threshold:%s power_save:off",
           authmode_name(WIFI_AUTH_WPA_PSK));
  ESP_LOGI(TAG, "STA MAC:" MACSTR, MAC2STR(mac));
}

void app_main(void) {
  init_nvs();
  ESP_LOGI(TAG, "ESP32-C3 station hardware test using ESP-IDF");
  wifi_init_station();

  EventBits_t bits = xEventGroupWaitBits(
      wifi_event_group, WIFI_CONNECTED_BIT | WIFI_FAIL_BIT, pdFALSE, pdFALSE,
      pdMS_TO_TICKS(30000));

  if ((bits & WIFI_CONNECTED_BIT) != 0) {
    ESP_LOGI(TAG, "connected to configured target SSID");
  } else if ((bits & WIFI_FAIL_BIT) != 0) {
    ESP_LOGE(TAG, "failed to connect to configured target SSID");
  } else {
    ESP_LOGE(TAG, "timed out connecting to configured target SSID");
  }

  while (true) {
    wifi_ap_record_t ap_info;
    if (esp_wifi_sta_get_ap_info(&ap_info) == ESP_OK) {
      ESP_LOGI(TAG, "connected rssi:%d channel:%d authmode:%d", ap_info.rssi,
               ap_info.primary, ap_info.authmode);
    } else {
      ESP_LOGI(TAG, "station not connected");
    }
    vTaskDelay(pdMS_TO_TICKS(5000));
  }
}
