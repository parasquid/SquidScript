use squid_device_protocol::{
    encode_empty_response_into, encode_error_response_into, encode_hello_response_into, Opcode,
    Status, HEADER_LEN, MAGIC,
};

#[test]
fn heap_free_encoders_write_framed_headers_without_alloc_feature() {
    let mut out = [0u8; 128];

    let len = encode_hello_response_into(
        Opcode::Hello,
        42,
        "esp32c3-supermini",
        "squidscript-zephyr",
        true,
        4096,
        &mut out,
    )
    .unwrap();

    assert!(len > HEADER_LEN);
    assert_eq!(&out[0..4], &MAGIC);
    assert_eq!(out[4], 2);
    assert_eq!(out[5], Opcode::Hello as u8);
    assert_eq!(out[6], Status::Ok as u8);
    assert_eq!(&out[8..12], &42u32.to_le_bytes());
}

#[test]
fn heap_free_encoders_report_capacity_errors() {
    let mut out = [0u8; 8];

    assert!(encode_empty_response_into(Opcode::Reset, Status::Ok, 80, &mut out).is_err());
    assert!(encode_error_response_into(
        Opcode::StorageFormat,
        81,
        -22,
        "invalid request",
        &mut out,
    )
    .is_err());
}
