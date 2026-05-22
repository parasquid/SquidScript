use squidc::protocol::{
    app_install_begin_request, app_install_chunk_request, app_install_commit_request,
    app_launch_request, app_list_entries, app_list_request, decode_frame, encode_frame,
    hello_identity, hello_request, output_get_request, output_lines, temp_run_begin_request,
    temp_run_chunk_request, temp_run_commit_request, AppEntry, DecodeError, Field, FieldValue,
    Frame, FrameKind, Opcode, Status,
};

#[test]
fn encodes_hello_frame_with_little_endian_header_and_tlv_payload() {
    let frame = Frame::request(
        Opcode::Hello,
        7,
        vec![
            Field::string(1, "esp32c3-supermini"),
            Field::bool(2, true),
            Field::u64(3, 4096),
        ],
    );

    let bytes = encode_frame(&frame);

    assert_eq!(
        bytes,
        [
            0x53, 0x51, 0x44, 0x50, // magic SQDP
            0x01, 0x01, 0x00, 0x00, // request, hello, ok, reserved
            0x07, 0x00, 0x00, 0x00, // sequence
            0x26, 0x00, 0x00, 0x00, // payload length
            0x43, 0xa5, 0x05, 0x5c, // payload CRC32
            0x01, 0x01, 0x11, 0x00, 0x65, 0x73, 0x70, 0x33, 0x32, 0x63, 0x33, 0x2d, 0x73, 0x75,
            0x70, 0x65, 0x72, 0x6d, 0x69, 0x6e, 0x69, 0x02, 0x03, 0x01, 0x00, 0x01, 0x03, 0x05,
            0x08, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]
    );
}

#[test]
fn decodes_frame_and_rejects_payload_crc_mismatch() {
    let frame = Frame::response(
        Opcode::ResourcesGet,
        Status::Ok,
        12,
        vec![Field::u64(1, 409600), Field::u64(2, 86016)],
    );
    let mut bytes = encode_frame(&frame);

    assert_eq!(decode_frame(&bytes).unwrap(), frame);

    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    assert_eq!(decode_frame(&bytes).unwrap_err(), DecodeError::PayloadCrc);
}

#[test]
fn supports_repeated_record_fields_without_collapsing_them() {
    let frame = Frame::response(
        Opcode::AppList,
        Status::Ok,
        3,
        vec![
            Field::record(1, vec![Field::string(1, "main"), Field::u64(2, 12)]),
            Field::record(1, vec![Field::string(1, "settings"), Field::u64(2, 24)]),
        ],
    );

    let decoded = decode_frame(&encode_frame(&frame)).unwrap();

    assert_eq!(decoded.kind, FrameKind::Response);
    assert_eq!(decoded.fields, frame.fields);
}

#[test]
fn builds_hello_request_and_extracts_identity_response() {
    let request = hello_request(9);
    assert_eq!(request.kind, FrameKind::Request);
    assert_eq!(request.opcode, Opcode::Hello);
    assert_eq!(request.sequence, 9);

    let response = Frame::response(
        Opcode::Hello,
        Status::Ok,
        9,
        vec![
            Field::string(1, "esp32c3-supermini"),
            Field::string(2, "squidscript-zephyr"),
            Field::bool(3, true),
        ],
    );

    let identity = hello_identity(&decode_frame(&encode_frame(&response)).unwrap()).unwrap();

    assert_eq!(identity.target, "esp32c3-supermini");
    assert_eq!(identity.firmware, "squidscript-zephyr");
    assert!(identity.diagnostic);
}

#[test]
fn builds_app_list_request_and_extracts_repeated_entries() {
    let request = app_list_request(10);
    assert_eq!(request.kind, FrameKind::Request);
    assert_eq!(request.opcode, Opcode::AppList);
    assert_eq!(request.sequence, 10);

    let response = Frame::response(
        Opcode::AppList,
        Status::Ok,
        10,
        vec![
            Field::record(1, vec![Field::string(1, "alpha"), Field::u64(2, 5)]),
            Field::record(1, vec![Field::string(1, "beta"), Field::u64(2, 6)]),
        ],
    );

    assert_eq!(
        app_list_entries(&decode_frame(&encode_frame(&response)).unwrap()).unwrap(),
        vec![
            AppEntry {
                app_id: "alpha".to_string(),
                sqbc_len: 5,
            },
            AppEntry {
                app_id: "beta".to_string(),
                sqbc_len: 6,
            },
        ]
    );
}

#[test]
fn builds_output_get_request_and_extracts_line_fields() {
    let request = output_get_request(3);
    assert_eq!(request.kind, FrameKind::Request);
    assert_eq!(request.opcode, Opcode::OutputGet);
    assert_eq!(request.sequence, 3);
    assert!(request.fields.is_empty());

    let response = Frame::response(
        Opcode::OutputGet,
        Status::Ok,
        3,
        vec![Field::string(1, "ready"), Field::string(1, "tick")],
    );

    assert_eq!(
        output_lines(&decode_frame(&encode_frame(&response)).unwrap()).unwrap(),
        vec!["ready".to_string(), "tick".to_string()]
    );
}

#[test]
fn builds_installed_app_chunked_write_requests() {
    let begin = app_install_begin_request(11, "alpha", 5, 0x8587d865);
    assert_eq!(begin.opcode, Opcode::AppInstallBegin);
    assert_eq!(
        begin.fields,
        vec![
            Field::string(1, "alpha"),
            Field::u64(2, 5),
            Field::u64(3, 0x8587d865),
        ]
    );

    let chunk = app_install_chunk_request(12, 3, b"lo".to_vec());
    assert_eq!(chunk.opcode, Opcode::AppInstallChunk);
    assert_eq!(
        chunk.fields,
        vec![Field::u64(1, 3), Field::bytes(2, b"lo".to_vec())]
    );

    let commit = app_install_commit_request(13);
    assert_eq!(commit.opcode, Opcode::AppInstallCommit);
    assert!(commit.fields.is_empty());

    let decoded = decode_frame(&encode_frame(&chunk)).unwrap();
    assert_eq!(decoded.fields[1].value, FieldValue::Bytes(b"lo".to_vec()));
}

#[test]
fn builds_app_launch_request() {
    let request = app_launch_request(20, "alpha");

    assert_eq!(request.opcode, Opcode::AppLaunch);
    assert_eq!(request.fields, vec![Field::string(1, "alpha")]);
}

#[test]
fn builds_temp_run_chunked_write_requests() {
    assert_eq!(
        temp_run_begin_request(30, "quick", 5, 0x3610a686).opcode,
        Opcode::TempRunBegin
    );
    assert_eq!(
        temp_run_chunk_request(31, 0, b"hel".to_vec()).fields,
        vec![Field::u64(1, 0), Field::bytes(2, b"hel".to_vec())]
    );
    assert_eq!(temp_run_commit_request(32).opcode, Opcode::TempRunCommit);
}
