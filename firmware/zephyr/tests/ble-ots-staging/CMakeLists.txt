cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(squidscript_ble_ots_staging_tests)

include(../../cmake/squidvm_ffi.cmake)

set(SQUID_ZEPHYR_TARGET_JSON "$ENV{SQUID_ZEPHYR_TARGET_JSON}" CACHE FILEPATH "SquidScript target metadata JSON")
if(SQUID_ZEPHYR_TARGET_JSON STREQUAL "")
  set(SQUID_ZEPHYR_TARGET_JSON "${CMAKE_CURRENT_LIST_DIR}/../../../../targets/esp32c3-super-mini.target.json")
endif()
set(SQUID_ZEPHYR_TARGET_OVERLAY "$ENV{SQUID_ZEPHYR_TARGET_OVERLAY}" CACHE FILEPATH "SquidScript Zephyr target overlay")
if(SQUID_ZEPHYR_TARGET_OVERLAY STREQUAL "")
  set(SQUID_ZEPHYR_TARGET_OVERLAY "${CMAKE_CURRENT_LIST_DIR}/../../boards/esp32c3_supermini.overlay")
endif()
set(SQUID_ZEPHYR_TARGET_DEFAULTS_H "${CMAKE_CURRENT_BINARY_DIR}/generated/squidscript_target_defaults.h")
execute_process(
  COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_LIST_DIR}/../../../../scripts/generate-zephyr-target-defaults.py" "${SQUID_ZEPHYR_TARGET_JSON}" "${SQUID_ZEPHYR_TARGET_DEFAULTS_H}" --zephyr-overlay "${SQUID_ZEPHYR_TARGET_OVERLAY}"
  RESULT_VARIABLE SQUID_ZEPHYR_TARGET_DEFAULTS_RESULT
)
if(NOT SQUID_ZEPHYR_TARGET_DEFAULTS_RESULT EQUAL 0)
  message(FATAL_ERROR "Failed to generate SquidScript Zephyr target defaults")
endif()

target_include_directories(app PRIVATE ../../src "${CMAKE_CURRENT_BINARY_DIR}/generated")
target_sources(app PRIVATE
  ../../src/app_store.c
  ../../src/ble_ots.c
  src/main.c
)

file(MAKE_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/test_fs)

target_compile_definitions(app PRIVATE
  TEST_FS_DIR="${CMAKE_CURRENT_BINARY_DIR}/test_fs"
  SQ_BLE_OTS_STAGING_DIR="/sqtest/tmp"
)

squidscript_link_squidvm_ffi(app)
