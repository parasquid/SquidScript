cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(squidscript_protocol_tests)

include(../../cmake/squidvm_ffi.cmake)

target_include_directories(app PRIVATE ../../src)
target_sources(app PRIVATE
  ../../src/app_store.c
  ../../src/device_protocol.c
  ../../src/protocol.c
  ../../src/serial_transport.c
  ../../src/vm_fs_storage.c
  ../../src/vm_runtime.c
  ../../src/vm_storage.c
  src/main.c
)

file(MAKE_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/test_fs)

target_compile_definitions(app PRIVATE
  TEST_FS_DIR="${CMAKE_CURRENT_BINARY_DIR}/test_fs"
)

squidscript_link_squidvm_ffi(app)
