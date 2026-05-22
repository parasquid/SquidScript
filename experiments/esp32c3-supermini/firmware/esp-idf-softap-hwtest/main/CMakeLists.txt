if("$ENV{HWTEST_WIFI_MODE}" STREQUAL "scan")
  set(hwtest_sources "scan_hwtest_main.c")
elseif("$ENV{HWTEST_WIFI_MODE}" STREQUAL "station")
  set(hwtest_sources "station_hwtest_main.c")

  if("$ENV{HWTEST_STA_SSID}" STREQUAL "")
    message(FATAL_ERROR "HWTEST_STA_SSID is required for station mode")
  endif()

  if("$ENV{HWTEST_STA_PASSWORD}" STREQUAL "")
    message(FATAL_ERROR "HWTEST_STA_PASSWORD is required for station mode")
  endif()

  set(HWTEST_STA_SSID "$ENV{HWTEST_STA_SSID}")
  set(HWTEST_STA_PASSWORD "$ENV{HWTEST_STA_PASSWORD}")
  string(REPLACE "\\" "\\\\" HWTEST_STA_SSID "${HWTEST_STA_SSID}")
  string(REPLACE "\"" "\\\"" HWTEST_STA_SSID "${HWTEST_STA_SSID}")
  string(REPLACE "\\" "\\\\" HWTEST_STA_PASSWORD "${HWTEST_STA_PASSWORD}")
  string(REPLACE "\"" "\\\"" HWTEST_STA_PASSWORD "${HWTEST_STA_PASSWORD}")

  configure_file(
    "${CMAKE_CURRENT_LIST_DIR}/hwtest_credentials.h.in"
    "${CMAKE_BINARY_DIR}/generated/hwtest_credentials.h"
    @ONLY
  )
else()
  set(hwtest_sources "softap_hwtest_main.c")
endif()

idf_component_register(SRCS "${hwtest_sources}" INCLUDE_DIRS ".")

if("$ENV{HWTEST_WIFI_MODE}" STREQUAL "station")
  target_include_directories(${COMPONENT_LIB} PRIVATE "${CMAKE_BINARY_DIR}/generated")
endif()
