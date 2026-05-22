function(squidscript_link_squidvm_ffi target)
  set(SQUIDSCRIPT_REPO_ROOT "${CMAKE_CURRENT_FUNCTION_LIST_DIR}/../../..")
  set(SQUIDVM_CARGO_TARGET_DIR "${CMAKE_BINARY_DIR}/squidvm-ffi-cargo-target")
  set(SQUIDVM_CARGO_MANIFEST "${SQUIDSCRIPT_REPO_ROOT}/Cargo.toml")

  find_program(SQUIDVM_RUSTUP rustup REQUIRED)

  execute_process(
    COMMAND "${SQUIDVM_RUSTUP}" which cargo --toolchain stable
    OUTPUT_VARIABLE SQUIDVM_CARGO
    OUTPUT_STRIP_TRAILING_WHITESPACE
    COMMAND_ERROR_IS_FATAL ANY
  )
  execute_process(
    COMMAND "${SQUIDVM_RUSTUP}" which rustc --toolchain stable
    OUTPUT_VARIABLE SQUIDVM_RUSTC
    OUTPUT_STRIP_TRAILING_WHITESPACE
    COMMAND_ERROR_IS_FATAL ANY
  )

  set(SQUIDVM_CARGO_TARGET_ARGS "")
  set(SQUIDVM_CARGO_FEATURE_ARGS "")
  set(SQUIDVM_STATICLIB_DIR "${SQUIDVM_CARGO_TARGET_DIR}/debug")

  if(CONFIG_SOC_ESP32C3 OR BOARD MATCHES "esp32c3")
    set(SQUIDVM_RUST_TARGET "riscv32imc-unknown-none-elf")
    set(SQUIDVM_CARGO_TARGET_ARGS --target "${SQUIDVM_RUST_TARGET}")
    set(SQUIDVM_CARGO_FEATURE_ARGS --no-default-features --features zephyr)
    set(SQUIDVM_STATICLIB_DIR "${SQUIDVM_CARGO_TARGET_DIR}/${SQUIDVM_RUST_TARGET}/debug")
  endif()

  set(SQUIDVM_STATICLIB "${SQUIDVM_STATICLIB_DIR}/libsquidvm_ffi.a")

  add_custom_target(squidvm_ffi_staticlib
    COMMAND "${CMAKE_COMMAND}" -E env
      "RUSTC=${SQUIDVM_RUSTC}"
      "CARGO_TARGET_DIR=${SQUIDVM_CARGO_TARGET_DIR}"
      "${SQUIDVM_CARGO}" build
        -p squidvm-ffi
        --manifest-path "${SQUIDVM_CARGO_MANIFEST}"
        ${SQUIDVM_CARGO_TARGET_ARGS}
        ${SQUIDVM_CARGO_FEATURE_ARGS}
    WORKING_DIRECTORY "${SQUIDSCRIPT_REPO_ROOT}"
    BYPRODUCTS "${SQUIDVM_STATICLIB}"
    VERBATIM
  )

  add_library(squidvm_ffi STATIC IMPORTED GLOBAL)
  set_target_properties(squidvm_ffi PROPERTIES IMPORTED_LOCATION "${SQUIDVM_STATICLIB}")
  add_dependencies(squidvm_ffi squidvm_ffi_staticlib)
  add_dependencies(${target} squidvm_ffi_staticlib)
  target_link_libraries(${target} PUBLIC squidvm_ffi)
endfunction()
