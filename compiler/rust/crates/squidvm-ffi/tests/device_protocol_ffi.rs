use squid_device_protocol::{decode_frame, FrameKind, Opcode, Status};

#[test]
fn ffi_encodes_hello_response_into_caller_buffer() {
    let mut out = [0u8; 128];
    let mut out_len = 0usize;
    let target = b"esp32c3-supermini";
    let firmware = b"squidscript-zephyr";

    let status = unsafe {
        squidvm_ffi::sqdp_encode_hello_response(
            Opcode::Hello as u8,
            42,
            target.as_ptr(),
            target.len(),
            firmware.as_ptr(),
            firmware.len(),
            true,
            out.as_mut_ptr(),
            out.len(),
            &mut out_len,
        )
    };

    assert_eq!(status as i32, 0);
    let frame = decode_frame(&out[..out_len]).unwrap();
    assert_eq!(frame.kind, FrameKind::Response);
    assert_eq!(frame.opcode, Opcode::Hello);
    assert_eq!(frame.status, Status::Ok);
    assert_eq!(frame.sequence, 42);
}

#[test]
fn ffi_rejects_too_small_output_buffer() {
    let mut out = [0u8; 8];
    let mut out_len = usize::MAX;

    let status = unsafe {
        squidvm_ffi::sqdp_encode_empty_response(
            Opcode::Reset as u8,
            Status::Ok as u8,
            80,
            out.as_mut_ptr(),
            out.len(),
            &mut out_len,
        )
    };

    assert_eq!(status as i32, 2);
    assert_eq!(out_len, 0);
}

#[test]
fn ffi_encodes_error_response_into_caller_buffer() {
    let mut out = [0u8; 128];
    let mut out_len = 0usize;
    let message = b"invalid request";

    let status = unsafe {
        squidvm_ffi::sqdp_encode_error_response(
            Opcode::StorageFormat as u8,
            81,
            -22,
            message.as_ptr(),
            message.len(),
            out.as_mut_ptr(),
            out.len(),
            &mut out_len,
        )
    };

    assert_eq!(status as i32, 0);
    let frame = decode_frame(&out[..out_len]).unwrap();
    assert_eq!(frame.kind, FrameKind::Response);
    assert_eq!(frame.opcode, Opcode::StorageFormat);
    assert_eq!(frame.status, Status::Error);
    assert_eq!(frame.sequence, 81);
}
