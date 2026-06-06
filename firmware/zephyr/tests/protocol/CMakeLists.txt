cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(squidscript_protocol_tests)

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

set(SQUID_ZEPHYR_FALLBACK_SOURCE "${CMAKE_CURRENT_LIST_DIR}/../../fallback/esp32c3-supermini-main.squid")
set(SQUID_ZEPHYR_FALLBACK_SQBC "${CMAKE_CURRENT_BINARY_DIR}/generated/squidscript_fallback_app.sqbc")
set(SQUID_ZEPHYR_FALLBACK_H "${CMAKE_CURRENT_BINARY_DIR}/generated/squidscript_fallback_app.h")
set(SQUID_ZEPHYR_FALLBACK_C "${CMAKE_CURRENT_BINARY_DIR}/generated/squidscript_fallback_app.c")
add_custom_command(
  OUTPUT "${SQUID_ZEPHYR_FALLBACK_SQBC}" "${SQUID_ZEPHYR_FALLBACK_H}" "${SQUID_ZEPHYR_FALLBACK_C}"
  COMMAND cargo run -p squidc -- app build "${SQUID_ZEPHYR_FALLBACK_SOURCE}" --out "${SQUID_ZEPHYR_FALLBACK_SQBC}"
  COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_LIST_DIR}/../../../../scripts/generate-zephyr-fallback-app.py" "${SQUID_ZEPHYR_FALLBACK_SQBC}" "main" "${SQUID_ZEPHYR_FALLBACK_H}" "${SQUID_ZEPHYR_FALLBACK_C}"
  DEPENDS "${SQUID_ZEPHYR_FALLBACK_SOURCE}" "${CMAKE_CURRENT_LIST_DIR}/../../../../scripts/generate-zephyr-fallback-app.py"
  WORKING_DIRECTORY "${CMAKE_CURRENT_LIST_DIR}/../../../.."
  VERBATIM
)

file(GLOB SQUID_ZEPHYR_PROTOCOL_FIXTURE_SOURCES CONFIGURE_DEPENDS
  "${CMAKE_CURRENT_LIST_DIR}/fixtures/*.squid"
)
set(SQUID_ZEPHYR_PROTOCOL_FIXTURES_H "${CMAKE_CURRENT_BINARY_DIR}/generated/squidscript_protocol_fixtures.h")
add_custom_command(
  OUTPUT "${SQUID_ZEPHYR_PROTOCOL_FIXTURES_H}"
  COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_LIST_DIR}/../../../../scripts/generate-zephyr-protocol-fixtures.py" "${SQUID_ZEPHYR_PROTOCOL_FIXTURES_H}" ${SQUID_ZEPHYR_PROTOCOL_FIXTURE_SOURCES}
  DEPENDS ${SQUID_ZEPHYR_PROTOCOL_FIXTURE_SOURCES} "${CMAKE_CURRENT_LIST_DIR}/../../../../scripts/generate-zephyr-protocol-fixtures.py"
  WORKING_DIRECTORY "${CMAKE_CURRENT_LIST_DIR}/../../../.."
  VERBATIM
)
add_custom_target(squidscript_protocol_fixtures DEPENDS "${SQUID_ZEPHYR_PROTOCOL_FIXTURES_H}")
add_dependencies(app squidscript_protocol_fixtures)

target_include_directories(app PRIVATE ../../src "${CMAKE_CURRENT_BINARY_DIR}/generated")
target_sources(app PRIVATE
  ../../src/app_lifecycle.c
  ../../src/app_store.c
  ../../src/ble_object_transfer.c
  ../../src/ble_profile_table.c
  ../../src/device_protocol.c
  ../../src/fallback_app.c
  ../../src/protocol.c
  ../../src/serial_transport.c
  ../../src/vm_fs_storage.c
  ../../src/vm_runtime.c
  ../../src/vm_runtime_app_lifecycle.c
  ../../src/vm_runtime_ble.c
  ../../src/vm_runtime_device_config.c
  ../../src/vm_runtime_display.c
  ../../src/vm_runtime_file.c
  ../../src/vm_runtime_indicator_gpio.c
  ../../src/vm_runtime_system.c
  ../../src/vm_runtime_timers.c
  ../../src/vm_runtime_wifi.c
  ../../src/vm_storage.c
  "${SQUID_ZEPHYR_FALLBACK_C}"
  src/main.c
)

file(MAKE_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/test_fs)

target_compile_definitions(app PRIVATE
  TEST_FS_DIR="${CMAKE_CURRENT_BINARY_DIR}/test_fs"
)

squidscript_link_squidvm_ffi(app)
