cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(squidscript_zephyr_firmware)

include(cmake/squidvm_ffi.cmake)

set(SQUID_ZEPHYR_TARGET_JSON "$ENV{SQUID_ZEPHYR_TARGET_JSON}" CACHE FILEPATH "SquidScript target metadata JSON")
if(SQUID_ZEPHYR_TARGET_JSON STREQUAL "")
  set(SQUID_ZEPHYR_TARGET_JSON "${CMAKE_CURRENT_LIST_DIR}/../../targets/esp32c3-super-mini.target.json")
endif()
set(SQUID_ZEPHYR_TARGET_OVERLAY "$ENV{SQUID_ZEPHYR_TARGET_OVERLAY}" CACHE FILEPATH "SquidScript Zephyr target overlay")
if(SQUID_ZEPHYR_TARGET_OVERLAY STREQUAL "")
  set(SQUID_ZEPHYR_TARGET_OVERLAY "${CMAKE_CURRENT_LIST_DIR}/boards/esp32c3_supermini.overlay")
endif()
set(SQUID_ZEPHYR_TARGET_DEFAULTS_H "${CMAKE_CURRENT_BINARY_DIR}/generated/squidscript_target_defaults.h")
execute_process(
  COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_LIST_DIR}/../../scripts/generate-zephyr-target-defaults.py" "${SQUID_ZEPHYR_TARGET_JSON}" "${SQUID_ZEPHYR_TARGET_DEFAULTS_H}" --zephyr-overlay "${SQUID_ZEPHYR_TARGET_OVERLAY}"
  RESULT_VARIABLE SQUID_ZEPHYR_TARGET_DEFAULTS_RESULT
)
if(NOT SQUID_ZEPHYR_TARGET_DEFAULTS_RESULT EQUAL 0)
  message(FATAL_ERROR "Failed to generate SquidScript Zephyr target defaults")
endif()

set(SQUID_ZEPHYR_FALLBACK_SOURCE "${CMAKE_CURRENT_LIST_DIR}/fallback/esp32c3-supermini-main.squid" CACHE FILEPATH "SquidScript firmware fallback source")
set(SQUID_ZEPHYR_FALLBACK_APP_ID "main" CACHE STRING "SquidScript firmware fallback logical app id")
set(SQUID_ZEPHYR_FALLBACK_SQBC "${CMAKE_CURRENT_BINARY_DIR}/generated/squidscript_fallback_app.sqbc")
set(SQUID_ZEPHYR_FALLBACK_H "${CMAKE_CURRENT_BINARY_DIR}/generated/squidscript_fallback_app.h")
set(SQUID_ZEPHYR_FALLBACK_C "${CMAKE_CURRENT_BINARY_DIR}/generated/squidscript_fallback_app.c")
add_custom_command(
  OUTPUT "${SQUID_ZEPHYR_FALLBACK_SQBC}" "${SQUID_ZEPHYR_FALLBACK_H}" "${SQUID_ZEPHYR_FALLBACK_C}"
  COMMAND cargo run -p squidc -- build "${SQUID_ZEPHYR_FALLBACK_SOURCE}" --out "${SQUID_ZEPHYR_FALLBACK_SQBC}"
  COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_LIST_DIR}/../../scripts/generate-zephyr-fallback-app.py" "${SQUID_ZEPHYR_FALLBACK_SQBC}" "${SQUID_ZEPHYR_FALLBACK_APP_ID}" "${SQUID_ZEPHYR_FALLBACK_H}" "${SQUID_ZEPHYR_FALLBACK_C}"
  DEPENDS "${SQUID_ZEPHYR_FALLBACK_SOURCE}" "${CMAKE_CURRENT_LIST_DIR}/../../scripts/generate-zephyr-fallback-app.py"
  WORKING_DIRECTORY "${CMAKE_CURRENT_LIST_DIR}/../.."
  VERBATIM
)

target_include_directories(app PRIVATE src "${CMAKE_CURRENT_BINARY_DIR}/generated")

target_sources(app PRIVATE
  src/app_lifecycle.c
  src/app_store.c
  src/device_protocol.c
  src/fallback_app.c
  src/main.c
  src/protocol.c
  src/serial_transport.c
  src/vm_fs_storage.c
  src/vm_runtime.c
  src/vm_storage.c
  "${SQUID_ZEPHYR_FALLBACK_C}"
)

option(SQUID_ZEPHYR_STACK_USAGE "Emit GCC .su stack-usage files for Zephyr app C sources" OFF)
if(SQUID_ZEPHYR_STACK_USAGE)
  target_compile_options(app PRIVATE $<$<COMPILE_LANGUAGE:C>:-fstack-usage>)
endif()

squidscript_link_squidvm_ffi(app)
