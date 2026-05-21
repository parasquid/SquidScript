/*
 * ESP32-C3 SuperMini SoftAP hardware test.
 *
 * This intentionally follows Espressif's ESP-IDF SoftAP example shape so it can
 * isolate board/RF behavior from SquidScript's Rust firmware and esp-radio path.
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

#define HWTEST_WIFI_SSID "ESP32C3-HWTEST"
#define HWTEST_WIFI_PASS ""
#define HWTEST_WIFI_CHANNEL 6
#define HWTEST_MAX_STA_CONN 4

static const char *TAG = "softap_hwtest";

static void wifi_event_handler(
    void *arg,
    esp_event_base_t event_base,
    int32_t event_id,
    void *event_data) {
  (void)arg;
  (void)event_base;

  if (event_id == WIFI_EVENT_AP_STACONNECTED) {
    wifi_event_ap_staconnected_t *event =
        (wifi_event_ap_staconnected_t *)event_data;
    ESP_LOGI(TAG, "station " MACSTR " join, AID=%d", MAC2STR(event->mac),
             event->aid);
  } else if (event_id == WIFI_EVENT_AP_STADISCONNECTED) {
    wifi_event_ap_stadisconnected_t *event =
        (wifi_event_ap_stadisconnected_t *)event_data;
    ESP_LOGI(TAG, "station " MACSTR " leave, AID=%d, reason=%d",
             MAC2STR(event->mac), event->aid, event->reason);
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

static void wifi_init_softap(void) {
  ESP_ERROR_CHECK(esp_netif_init());
  ESP_ERROR_CHECK(esp_event_loop_create_default());
  esp_netif_t *ap_netif = esp_netif_create_default_wifi_ap();
  ESP_ERROR_CHECK(ap_netif == NULL ? ESP_FAIL : ESP_OK);

  wifi_init_config_t cfg = WIFI_INIT_CONFIG_DEFAULT();
  ESP_ERROR_CHECK(esp_wifi_init(&cfg));

  ESP_ERROR_CHECK(esp_event_handler_instance_register(
      WIFI_EVENT, ESP_EVENT_ANY_ID, &wifi_event_handler, NULL, NULL));

  wifi_config_t wifi_config = {
      .ap =
          {
              .ssid = HWTEST_WIFI_SSID,
              .ssid_len = strlen(HWTEST_WIFI_SSID),
              .channel = HWTEST_WIFI_CHANNEL,
              .password = HWTEST_WIFI_PASS,
              .max_connection = HWTEST_MAX_STA_CONN,
              .authmode = WIFI_AUTH_OPEN,
              .pmf_cfg =
                  {
                      .required = false,
                  },
          },
  };

  ESP_ERROR_CHECK(esp_wifi_set_mode(WIFI_MODE_AP));
  ESP_ERROR_CHECK(esp_wifi_set_config(WIFI_IF_AP, &wifi_config));
  ESP_ERROR_CHECK(esp_wifi_start());

  uint8_t mac[6] = {0};
  ESP_ERROR_CHECK(esp_wifi_get_mac(WIFI_IF_AP, mac));

  ESP_LOGI(TAG, "SoftAP hardware test started");
  ESP_LOGI(TAG, "SSID:%s password:<open> channel:%d", HWTEST_WIFI_SSID,
           HWTEST_WIFI_CHANNEL);
  ESP_LOGI(TAG, "AP MAC:" MACSTR, MAC2STR(mac));
  ESP_LOGI(TAG, "Expected AP IP: 192.168.4.1");
}

void app_main(void) {
  init_nvs();
  ESP_LOGI(TAG, "ESP32-C3 SoftAP hardware test using ESP-IDF");
  wifi_init_softap();

  while (true) {
    wifi_sta_list_t stations;
    if (esp_wifi_ap_get_sta_list(&stations) == ESP_OK) {
      ESP_LOGI(TAG, "connected stations:%d", stations.num);
    }
    vTaskDelay(pdMS_TO_TICKS(5000));
  }
}
