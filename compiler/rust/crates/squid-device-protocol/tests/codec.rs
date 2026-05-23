#![cfg(feature = "alloc")]

use squid_device_protocol::{
    app_list_entries, decode_frame, encode_app_list_response_into, encode_empty_response_into,
    encode_error_response_into, encode_frame, encode_frame_into, encode_hello_response_into,
    hello_identity, hello_request, AppListEntry, DecodeError, Field, FieldValue, Frame, FrameKind,
    Opcode, Status,
};

#[test]
fn encodes_hello_request_into_caller_buffer() {
    let frame = hello_request(7);
    let expected = encode_frame(&frame);
    let mut out = [0u8; 32];

    let len = encode_frame_into(&frame, &mut out).expect("frame should fit");

    assert_eq!(&out[..len], expected.as_slice());
    assert_eq!(decode_frame(&out[..len]).unwrap(), frame);
}

#[test]
fn reports_output_buffer_capacity_without_partial_success() {
    let frame = Frame::response(
        Opcode::Hello,
        Status::Ok,
        9,
        vec![
            Field::string(1, "esp32c3-supermini"),
            Field::string(2, "zephyr"),
            Field::bool(3, true),
        ],
    );
    let mut out = [0u8; 20];

    assert_eq!(
        encode_frame_into(&frame, &mut out),
        Err(DecodeError::OutputTooSmall {
            needed: encode_frame(&frame).len(),
            capacity: out.len()
        })
    );
}

#[test]
fn decodes_hello_identity_from_shared_codec() {
    let response = Frame::response(
        Opcode::Hello,
        Status::Ok,
        11,
        vec![
            Field::string(1, "esp32c3-supermini"),
            Field::string(2, "zephyr"),
            Field::bool(3, true),
        ],
    );

    assert_eq!(
        hello_identity(&decode_frame(&encode_frame(&response)).unwrap()).unwrap(),
        squid_device_protocol::HelloIdentity {
            target: "esp32c3-supermini".to_string(),
            firmware: "zephyr".to_string(),
            diagnostic: true
        }
    );
}

#[test]
fn preserves_frame_header_shape() {
    let bytes = encode_frame(&Frame::request(
        Opcode::Key,
        0x0102_0304,
        vec![Field::string(1, "SELECT")],
    ));

    assert_eq!(&bytes[0..4], b"SQDP");
    assert_eq!(bytes[4], FrameKind::Request as u8);
    assert_eq!(bytes[5], Opcode::Key as u8);
    assert_eq!(bytes[6], Status::Ok as u8);
    assert_eq!(&bytes[8..12], &[4, 3, 2, 1]);
}

#[test]
fn encodes_heap_free_hello_response() {
    let mut out = [0u8; 128];

    let len = encode_hello_response_into(
        Opcode::Hello,
        42,
        "esp32c3-supermini",
        "squidscript-zephyr",
        true,
        &mut out,
    )
    .unwrap();
    let decoded = decode_frame(&out[..len]).unwrap();

    assert_eq!(decoded.kind, FrameKind::Response);
    assert_eq!(decoded.opcode, Opcode::Hello);
    assert_eq!(decoded.status, Status::Ok);
    assert_eq!(decoded.sequence, 42);
    assert_eq!(
        decoded.fields,
        vec![
            Field::string(1, "esp32c3-supermini"),
            Field::string(2, "squidscript-zephyr"),
            Field::bool(3, true),
        ]
    );
}

#[test]
fn encodes_heap_free_empty_response() {
    let mut out = [0u8; 32];

    let len = encode_empty_response_into(Opcode::Reset, Status::Ok, 80, &mut out).unwrap();
    let decoded = decode_frame(&out[..len]).unwrap();

    assert_eq!(decoded.opcode, Opcode::Reset);
    assert_eq!(decoded.status, Status::Ok);
    assert!(decoded.fields.is_empty());
}

#[test]
fn encodes_heap_free_app_list_response() {
    let mut out = [0u8; 160];
    let entries = [
        AppListEntry {
            app_id: "main",
            sqbc_len: 123,
        },
        AppListEntry {
            app_id: "reader-clock",
            sqbc_len: 456,
        },
    ];

    let len = encode_app_list_response_into(90, entries.iter().copied(), &mut out).unwrap();
    let decoded = decode_frame(&out[..len]).unwrap();

    assert_eq!(decoded.kind, FrameKind::Response);
    assert_eq!(decoded.opcode, Opcode::AppList);
    assert_eq!(decoded.status, Status::Ok);
    assert_eq!(decoded.sequence, 90);
    assert_eq!(
        app_list_entries(&decoded).unwrap(),
        vec![
            squid_device_protocol::AppEntry {
                app_id: "main".to_string(),
                sqbc_len: 123,
            },
            squid_device_protocol::AppEntry {
                app_id: "reader-clock".to_string(),
                sqbc_len: 456,
            },
        ]
    );
}

#[test]
fn encodes_heap_free_error_response() {
    let mut out = [0u8; 128];

    let len =
        encode_error_response_into(Opcode::StorageFormat, 81, -22, "invalid request", &mut out)
            .unwrap();
    let decoded = decode_frame(&out[..len]).unwrap();

    assert_eq!(decoded.status, Status::Error);
    assert_eq!(
        decoded.fields,
        vec![
            Field {
                tag: 250,
                value: FieldValue::I64(-22),
            },
            Field::string(251, "invalid request"),
        ]
    );
}
