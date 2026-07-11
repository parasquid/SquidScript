#![cfg(feature = "alloc")]

use squid_device_protocol::{
    app_launch_request, app_list_entries, content_install_begin_request,
    content_install_chunk_request, content_install_commit_request, decode_frame,
    encode_app_list_response_into, encode_empty_response_into, encode_error_response_into,
    encode_firmware_info_response_into, encode_firmware_update_status_response_into, encode_frame,
    encode_frame_into, encode_hello_response_into, encode_lifecycle_response_into,
    encode_lifecycle_response_with_details_into, encode_line_response_into,
    encode_resources_response_into, event_dispatch_request, firmware_info, firmware_info_request,
    firmware_update_abort_request, firmware_update_begin_request, firmware_update_chunk_request,
    firmware_update_commit_request, firmware_update_status, firmware_update_status_request,
    hello_identity, hello_request, key_event_from_request_into, key_request, lifecycle_lines,
    output_lines, request_bytes_field, request_string_field, request_u64_field, resource_values,
    resources_get_request_with_heap_reset, runtime_cap_clear_request, runtime_cap_get_request,
    runtime_cap_set_request, AppListEntry, DecodeError, DeviceRequest, Field, FieldValue,
    FirmwareInfoRef, FirmwareUpdateStatusRef, Frame, FrameKind, HostAction, LifecycleTimer, Opcode,
    ProtocolSessions, ResourceMetric, SessionError, Status, TransferCapabilities,
    FIRMWARE_SHA256_BYTES, MAX_CONTENT_NAME_BYTES,
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
            Field::string(2, "native"),
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
            Field::string(2, "native"),
            Field::bool(3, true),
        ],
    );

    assert_eq!(
        hello_identity(&decode_frame(&encode_frame(&response)).unwrap()).unwrap(),
        squid_device_protocol::HelloIdentity {
            target: "esp32c3-supermini".to_string(),
            firmware: "native".to_string(),
            diagnostic: true,
            transfer_capabilities: TransferCapabilities::default_serial(),
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
fn extracts_key_event_from_framed_request_without_allocating_payload() {
    let request = encode_frame(&key_request(48, "SELECT"));
    let mut event = [0u8; 16];

    let len = key_event_from_request_into(&request, &mut event).unwrap();

    assert_eq!(&event[..len], b"key.SELECT");
}

#[test]
fn extracts_string_fields_from_decoded_requests_without_allocating_payload() {
    let launch_bytes = encode_frame(&app_launch_request(20, "reader"));
    let event_bytes = encode_frame(&event_dispatch_request(49, "reader", "app.start"));
    let launch = DeviceRequest::decode(&launch_bytes).unwrap();
    let event = DeviceRequest::decode(&event_bytes).unwrap();

    assert_eq!(request_string_field(&launch, 1).unwrap(), Some("reader"));
    assert_eq!(request_string_field(&event, 1).unwrap(), Some("reader"));
    assert_eq!(request_string_field(&event, 2).unwrap(), Some("app.start"));
    assert_eq!(request_string_field(&event, 3).unwrap(), None);
}

#[test]
fn extracts_byte_fields_from_decoded_requests_without_allocating_payload() {
    let state_bytes = encode_frame(&squid_device_protocol::state_import_request(
        72,
        vec![1, 2, 3, 4],
    ));
    let request = DeviceRequest::decode(&state_bytes).unwrap();

    assert_eq!(
        request_bytes_field(&request, 1).unwrap(),
        Some(&[1, 2, 3, 4][..])
    );
    assert_eq!(request_bytes_field(&request, 2).unwrap(), None);
}

#[test]
fn encodes_heap_free_hello_response() {
    let mut out = [0u8; 128];

    let len = encode_hello_response_into(
        Opcode::Hello,
        42,
        "esp32c3-supermini",
        "squidscript-native",
        true,
        4096,
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
            Field::string(2, "squidscript-native"),
            Field::bool(3, true),
            Field::u64(4, 4096),
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
fn encodes_heap_free_repeated_line_response() {
    let mut out = [0u8; 128];
    let lines = ["count 1", "count 2"];

    let len =
        encode_line_response_into(Opcode::OutputGet, 91, lines.iter().copied(), &mut out).unwrap();
    let decoded = decode_frame(&out[..len]).unwrap();

    assert_eq!(decoded.kind, FrameKind::Response);
    assert_eq!(decoded.opcode, Opcode::OutputGet);
    assert_eq!(decoded.status, Status::Ok);
    assert_eq!(decoded.sequence, 91);
    assert_eq!(
        output_lines(&decoded).unwrap(),
        vec!["count 1".to_string(), "count 2".to_string()]
    );
}

#[test]
fn encodes_heap_free_lifecycle_response_from_structured_inputs() {
    let mut out = [0u8; 192];
    let process = ["main"];
    let armed = [LifecycleTimer {
        app_id: "break-reminder",
        event: "timer.break",
    }];

    let len = encode_lifecycle_response_into(
        92,
        Some("reader-clock"),
        process.iter().copied(),
        armed.iter().copied(),
        &mut out,
    )
    .unwrap();
    let decoded = decode_frame(&out[..len]).unwrap();

    assert_eq!(decoded.kind, FrameKind::Response);
    assert_eq!(decoded.opcode, Opcode::LifecycleGet);
    assert_eq!(decoded.status, Status::Ok);
    assert_eq!(
        lifecycle_lines(&decoded).unwrap(),
        vec![
            "active=reader-clock".to_string(),
            "process_stack[0]=main".to_string(),
            "armed_stack=".to_string(),
            "armed_stack[0]=break-reminder timer.break".to_string(),
        ]
    );
}

#[test]
fn encodes_native_lifecycle_details_after_structured_stacks() {
    let mut out = [0u8; 256];
    let details = [
        "lifecycle=idle",
        "start_reason=return",
        "event_queue=0 overflow=0",
    ];
    let len = encode_lifecycle_response_with_details_into(
        93,
        Some("main"),
        core::iter::empty(),
        core::iter::empty(),
        details.iter().copied(),
        &mut out,
    )
    .unwrap();
    let decoded = decode_frame(&out[..len]).unwrap();

    assert_eq!(
        lifecycle_lines(&decoded).unwrap(),
        vec![
            "active=main".to_string(),
            "armed_stack=".to_string(),
            "lifecycle=idle".to_string(),
            "start_reason=return".to_string(),
            "event_queue=0 overflow=0".to_string(),
        ]
    );
}

#[test]
fn encodes_heap_free_resources_response_from_metrics() {
    let mut out = [0u8; 192];
    let metrics = [
        ResourceMetric {
            key: "ram_total_bytes",
            value: 409_600,
        },
        ResourceMetric {
            key: "proto_stack_used_bytes",
            value: 3_928,
        },
    ];

    let len = encode_resources_response_into(93, metrics.iter().copied(), &mut out).unwrap();
    let decoded = decode_frame(&out[..len]).unwrap();

    assert_eq!(decoded.kind, FrameKind::Response);
    assert_eq!(decoded.opcode, Opcode::ResourcesGet);
    assert_eq!(decoded.status, Status::Ok);
    let FieldValue::Record(first_fields) = &decoded.fields[0].value else {
        panic!("first resource metric should be a record");
    };
    assert!(matches!(first_fields[0].value, FieldValue::U32(1)));
    assert!(matches!(first_fields[1].value, FieldValue::U32(409_600)));
    assert_eq!(
        resource_values(&decoded).unwrap(),
        vec![
            ("ram_total_bytes".to_string(), 409_600),
            ("proto_stack_used_bytes".to_string(), 3_928),
        ]
    );
}

#[test]
fn encodes_radio_and_upload_resources_response_from_metrics() {
    let mut out = [0u8; 512];
    let metrics = [
        ResourceMetric {
            key: "radio_active_leases",
            value: 2,
        },
        ResourceMetric {
            key: "radio_wifi_active",
            value: 1,
        },
        ResourceMetric {
            key: "radio_ble_active",
            value: 1,
        },
        ResourceMetric {
            key: "upload_profile_active",
            value: 1,
        },
        ResourceMetric {
            key: "upload_profile_id_len",
            value: 2,
        },
        ResourceMetric {
            key: "upload_profile_start_events",
            value: 3,
        },
        ResourceMetric {
            key: "upload_profile_stop_events",
            value: 2,
        },
        ResourceMetric {
            key: "upload_transport_http_active",
            value: 0,
        },
        ResourceMetric {
            key: "upload_transport_ble_active",
            value: 1,
        },
        ResourceMetric {
            key: "serial_buffer_bytes",
            value: 5_176,
        },
        ResourceMetric {
            key: "known_static_bytes",
            value: 32_652,
        },
        ResourceMetric {
            key: "heap_pool_bytes",
            value: 102_400,
        },
        ResourceMetric {
            key: "known_used_bytes",
            value: 114_980,
        },
        ResourceMetric {
            key: "nonheap_remainder_bytes",
            value: 274_548,
        },
    ];

    let len = encode_resources_response_into(94, metrics.iter().copied(), &mut out).unwrap();
    let decoded = decode_frame(&out[..len]).unwrap();

    assert_eq!(
        resource_values(&decoded).unwrap(),
        vec![
            ("radio_active_leases".to_string(), 2),
            ("radio_wifi_active".to_string(), 1),
            ("radio_ble_active".to_string(), 1),
            ("upload_profile_active".to_string(), 1),
            ("upload_profile_id_len".to_string(), 2),
            ("upload_profile_start_events".to_string(), 3),
            ("upload_profile_stop_events".to_string(), 2),
            ("upload_transport_http_active".to_string(), 0),
            ("upload_transport_ble_active".to_string(), 1),
            ("serial_buffer_bytes".to_string(), 5_176),
            ("known_static_bytes".to_string(), 32_652),
            ("heap_pool_bytes".to_string(), 102_400),
            ("known_used_bytes".to_string(), 114_980),
            ("nonheap_remainder_bytes".to_string(), 274_548),
        ]
    );
}

#[test]
fn encodes_display_pending_refresh_resources_response_from_metrics() {
    let mut out = [0u8; 128];
    let metrics = [
        ResourceMetric {
            key: "display_pending_refreshes",
            value: 3,
        },
        ResourceMetric {
            key: "display_recorded_draws",
            value: 5,
        },
        ResourceMetric {
            key: "display_dropped_draws",
            value: 1,
        },
    ];

    let len = encode_resources_response_into(95, metrics.iter().copied(), &mut out).unwrap();
    let decoded = decode_frame(&out[..len]).unwrap();

    assert_eq!(
        resource_values(&decoded).unwrap(),
        vec![
            ("display_pending_refreshes".to_string(), 3),
            ("display_recorded_draws".to_string(), 5),
            ("display_dropped_draws".to_string(), 1),
        ]
    );
}

#[test]
fn encodes_capability_demand_resources_response_from_metrics() {
    let mut out = [0u8; 256];
    let metrics = [
        ResourceMetric {
            key: "demand_wifi",
            value: 1,
        },
        ResourceMetric {
            key: "demand_ble",
            value: 1,
        },
        ResourceMetric {
            key: "demand_http",
            value: 0,
        },
        ResourceMetric {
            key: "demand_display",
            value: 1,
        },
        ResourceMetric {
            key: "demand_storage",
            value: 1,
        },
        ResourceMetric {
            key: "demand_binbook",
            value: 0,
        },
    ];

    let len = encode_resources_response_into(96, metrics.iter().copied(), &mut out).unwrap();
    let decoded = decode_frame(&out[..len]).unwrap();

    assert_eq!(
        resource_values(&decoded).unwrap(),
        vec![
            ("demand_wifi".to_string(), 1),
            ("demand_ble".to_string(), 1),
            ("demand_http".to_string(), 0),
            ("demand_display".to_string(), 1),
            ("demand_storage".to_string(), 1),
            ("demand_binbook".to_string(), 0),
        ]
    );
}

#[test]
fn encodes_resources_get_request_with_heap_max_reset() {
    let request = resources_get_request_with_heap_reset(17);

    assert_eq!(request.kind, FrameKind::Request);
    assert_eq!(request.opcode, Opcode::ResourcesGet);
    assert_eq!(request.sequence, 17);
    assert_eq!(request.fields, vec![Field::bool(1, true)]);
}

#[test]
fn encodes_runtime_cap_requests() {
    let get = runtime_cap_get_request(82, Some("vm_runtime.timer_max"));
    assert_eq!(get.opcode, Opcode::RuntimeCapGet);
    assert_eq!(get.fields, vec![Field::string(1, "vm_runtime.timer_max")]);

    let get_all = runtime_cap_get_request(83, None);
    assert_eq!(get_all.opcode, Opcode::RuntimeCapGet);
    assert!(get_all.fields.is_empty());

    let set = runtime_cap_set_request(84, "vm_runtime.timer_max", 2);
    assert_eq!(set.opcode, Opcode::RuntimeCapSet);
    assert_eq!(
        set.fields,
        vec![Field::string(1, "vm_runtime.timer_max"), Field::u32(2, 2)]
    );

    let clear = runtime_cap_clear_request(85, Some("vm_runtime.timer_max"));
    assert_eq!(clear.opcode, Opcode::RuntimeCapClear);
    assert_eq!(clear.fields, vec![Field::string(1, "vm_runtime.timer_max")]);
}

#[test]
fn encodes_content_install_requests() {
    let begin = content_install_begin_request(88, "weakest-tamer-v01.binbook", 12_460_884, 0x1234);
    assert_eq!(begin.opcode, Opcode::ContentInstallBegin);
    assert_eq!(
        begin.fields,
        vec![
            Field::string(1, "weakest-tamer-v01.binbook"),
            Field::u64(2, 12_460_884),
            Field::u64(3, 0x1234),
        ]
    );

    let chunk = content_install_chunk_request(89, 4096, b"page".to_vec());
    assert_eq!(chunk.opcode, Opcode::ContentInstallChunk);
    assert_eq!(
        chunk.fields,
        vec![
            Field::u64(1, 4096),
            Field::bytes(2, b"page".to_vec()),
            Field::bool(3, true),
        ]
    );

    let commit = content_install_commit_request(90);
    assert_eq!(commit.opcode, Opcode::ContentInstallCommit);
    assert!(commit.fields.is_empty());
}

#[test]
fn content_install_session_produces_host_actions() {
    let payload = b"current-binbook";
    let crc = crc32fast::hash(payload);
    let mut sessions = ProtocolSessions::default();
    let begin = encode_frame(&content_install_begin_request(
        88,
        "book.binbook",
        payload.len() as u64,
        crc as u64,
    ));
    let begin = DeviceRequest::decode(&begin).unwrap();

    assert_eq!(
        sessions.next_action(&begin).unwrap(),
        HostAction::BeginContentInstall {
            name: "book.binbook",
            total_len: payload.len(),
        }
    );
    sessions
        .complete_begin_content_install("books/book.binbook")
        .unwrap();

    let chunk = encode_frame(&content_install_chunk_request(89, 0, payload.to_vec()));
    let chunk = DeviceRequest::decode(&chunk).unwrap();
    assert_eq!(
        sessions.next_action(&chunk).unwrap(),
        HostAction::WriteContentChunk {
            path: "books/book.binbook",
            offset: 0,
            bytes: payload,
        }
    );
    sessions.complete_content_chunk(payload).unwrap();

    let commit = encode_frame(&content_install_commit_request(90));
    let commit = DeviceRequest::decode(&commit).unwrap();
    assert_eq!(
        sessions.next_action(&commit).unwrap(),
        HostAction::CommitContentInstall {
            name: "book.binbook",
            path: "books/book.binbook",
        }
    );
}

#[test]
fn content_install_session_accepts_full_portable_name_budget() {
    let longest = format!("{}.binbook", "a".repeat(113));
    let too_long = format!("{}.binbook", "a".repeat(114));
    assert_eq!(longest.len(), MAX_CONTENT_NAME_BYTES);
    let mut sessions = ProtocolSessions::default();

    let begin = encode_frame(&content_install_begin_request(88, &longest, 1, 0));
    let begin = DeviceRequest::decode(&begin).unwrap();
    assert_eq!(
        sessions.next_action(&begin).unwrap(),
        HostAction::BeginContentInstall {
            name: longest.as_str(),
            total_len: 1,
        }
    );

    let mut sessions = ProtocolSessions::default();
    let begin = encode_frame(&content_install_begin_request(88, &too_long, 1, 0));
    let begin = DeviceRequest::decode(&begin).unwrap();
    assert_eq!(sessions.next_action(&begin), Err(SessionError::PathTooLong));
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

#[test]
fn firmware_update_requests_round_trip_with_typed_fields() {
    let hash = vec![0x5a; FIRMWARE_SHA256_BYTES];
    let requests = [
        firmware_info_request(1),
        firmware_update_commit_request(4),
        firmware_update_status_request(5),
        firmware_update_abort_request(6),
    ];
    assert_eq!(
        requests.map(|request| request.opcode),
        [
            Opcode::FirmwareInfo,
            Opcode::FirmwareUpdateCommit,
            Opcode::FirmwareUpdateStatus,
            Opcode::FirmwareUpdateAbort,
        ]
    );

    let begin = encode_frame(&firmware_update_begin_request(
        2,
        123_456,
        hash.clone(),
        "build-a",
    ));
    let begin = DeviceRequest::decode(&begin).unwrap();
    assert_eq!(begin.opcode, Opcode::FirmwareUpdateBegin);
    assert_eq!(request_u64_field(&begin, 1).unwrap(), Some(123_456));
    assert_eq!(
        request_bytes_field(&begin, 2).unwrap(),
        Some(hash.as_slice())
    );
    assert_eq!(request_string_field(&begin, 3).unwrap(), Some("build-a"));

    let chunk = encode_frame(&firmware_update_chunk_request(3, 4096, vec![1, 2, 3]));
    let chunk = DeviceRequest::decode(&chunk).unwrap();
    assert_eq!(chunk.opcode, Opcode::FirmwareUpdateChunk);
    assert_eq!(request_u64_field(&chunk, 1).unwrap(), Some(4096));
    assert_eq!(
        request_bytes_field(&chunk, 2).unwrap(),
        Some(&[1, 2, 3][..])
    );
}

#[test]
fn firmware_info_and_status_responses_round_trip() {
    let mut out = [0u8; 384];
    let len = encode_firmware_info_response_into(
        10,
        FirmwareInfoRef {
            active_slot: "app0",
            active_slot_size: 0x280000,
            inactive_slot: "app1",
            inactive_slot_size: 0x280000,
            build_id: "build-a",
            boot_state: "valid",
        },
        &mut out,
    )
    .unwrap();
    let info = firmware_info(&decode_frame(&out[..len]).unwrap()).unwrap();
    assert_eq!(info.active_slot, "app0");
    assert_eq!(info.inactive_slot, "app1");
    assert_eq!(info.inactive_slot_size, 0x280000);
    assert_eq!(info.build_id, "build-a");
    assert_eq!(info.boot_state, "valid");

    let hash = [0xa5; FIRMWARE_SHA256_BYTES];
    let len = encode_firmware_update_status_response_into(
        11,
        Status::Pending,
        FirmwareUpdateStatusRef {
            state: "receiving",
            candidate_slot: "app1",
            expected_len: 123_456,
            durable_offset: 8192,
            build_id: "build-b",
            expected_sha256: &hash,
        },
        &mut out,
    )
    .unwrap();
    let status = firmware_update_status(&decode_frame(&out[..len]).unwrap()).unwrap();
    assert_eq!(status.state, "receiving");
    assert_eq!(status.candidate_slot, "app1");
    assert_eq!(status.expected_len, 123_456);
    assert_eq!(status.durable_offset, 8192);
    assert_eq!(status.build_id, "build-b");
    assert_eq!(status.expected_sha256, hash);
}

#[test]
fn firmware_response_encoders_enforce_capacity_and_hash_shape() {
    let hash = [0u8; FIRMWARE_SHA256_BYTES];
    let status = FirmwareUpdateStatusRef {
        state: "idle",
        candidate_slot: "app1",
        expected_len: 0,
        durable_offset: 0,
        build_id: "",
        expected_sha256: &hash,
    };
    assert!(
        encode_firmware_update_status_response_into(12, Status::Ok, status, &mut [0u8; 16])
            .is_err()
    );
    assert!(encode_firmware_update_status_response_into(
        12,
        Status::Ok,
        FirmwareUpdateStatusRef {
            expected_sha256: &hash[..31],
            ..status
        },
        &mut [0u8; 256]
    )
    .is_err());
}
