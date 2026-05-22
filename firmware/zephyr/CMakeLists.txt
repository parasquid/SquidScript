cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(squidscript_zephyr_firmware)

include(cmake/squidvm_ffi.cmake)

target_sources(app PRIVATE
  src/app_store.c
  src/device_protocol.c
  src/main.c
  src/protocol.c
  src/serial_transport.c
  src/vm_fs_storage.c
  src/vm_runtime.c
  src/vm_storage.c
)

squidscript_link_squidvm_ffi(app)
