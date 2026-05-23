use squid_device_protocol::{
    app_install_begin_request, app_install_chunk_request, app_install_commit_request,
    app_launch_request, app_list_entries, decode_frame, encode_frame, event_dispatch_request,
    key_request, lifecycle_lines, output_lines, protocol_error, resource_install_begin_request,
    resource_install_chunk_request, resource_install_commit_request, resource_values, state_bytes,
    state_import_request, wifi_profile_set_request, FrameKind, Opcode, Status,
};
use squidvm_ffi::{
    SqdpAction, SqdpActionKind, SqdpAppLaunch, SqdpAppListEntry, SqdpEventDispatch,
    SqdpLifecycleTimer, SqdpLineSlice, SqdpResourceMetric, SqdpResourceSession, SqdpStateImport,
    SqdpTransferSession, SqdpWifiProfile, SQDP_STAGING_PATH_CAP,
};

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

#[test]
fn ffi_encodes_protocol_error_response_from_status_code() {
    let mut out = [0u8; 128];
    let mut out_len = 0usize;

    let status = unsafe {
        squidvm_ffi::sqdp_encode_error_response_for_code(
            Opcode::AppLaunch as u8,
            82,
            -19,
            out.as_mut_ptr(),
            out.len(),
            &mut out_len,
        )
    };

    assert_eq!(status as i32, 0);
    let frame = decode_frame(&out[..out_len]).unwrap();
    let error = protocol_error(&frame).unwrap();
    assert_eq!(frame.kind, FrameKind::Response);
    assert_eq!(frame.opcode, Opcode::AppLaunch);
    assert_eq!(frame.status, Status::Error);
    assert_eq!(frame.sequence, 82);
    assert_eq!(error.code, -19);
    assert_eq!(error.message, "device unavailable");
}

#[test]
fn ffi_encodes_app_list_response_from_c_registry_entries() {
    let mut out = [0u8; 160];
    let mut out_len = 0usize;
    let mut entries = [SqdpAppListEntry::default(), SqdpAppListEntry::default()];
    entries[0].app_id[..4].copy_from_slice(b"main");
    entries[0].sqbc_len = 123;
    entries[1].app_id[..12].copy_from_slice(b"reader-clock");
    entries[1].sqbc_len = 456;

    let status = unsafe {
        squidvm_ffi::sqdp_encode_app_list_response(
            90,
            entries.as_ptr(),
            entries.len(),
            out.as_mut_ptr(),
            out.len(),
            &mut out_len,
        )
    };

    assert_eq!(status as i32, 0);
    let frame = decode_frame(&out[..out_len]).unwrap();
    assert_eq!(frame.kind, FrameKind::Response);
    assert_eq!(frame.opcode, Opcode::AppList);
    assert_eq!(frame.status, Status::Ok);
    assert_eq!(
        app_list_entries(&frame).unwrap(),
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
fn ffi_encodes_repeated_line_response_without_payload_staging() {
    let mut out = [0u8; 160];
    let mut out_len = 0usize;
    let mut fixed_lines = [[0u8; 16]; 2];
    fixed_lines[0][..7].copy_from_slice(b"count 1");
    fixed_lines[1][..7].copy_from_slice(b"count 2");
    let extra = b"count 3";
    let extra_lines = [SqdpLineSlice {
        bytes: extra.as_ptr(),
        len: extra.len(),
    }];

    let status = unsafe {
        squidvm_ffi::sqdp_encode_line_response(
            Opcode::OutputGet as u8,
            91,
            fixed_lines.as_ptr().cast(),
            fixed_lines.len(),
            fixed_lines[0].len(),
            extra_lines.as_ptr(),
            extra_lines.len(),
            out.as_mut_ptr(),
            out.len(),
            &mut out_len,
        )
    };

    assert_eq!(status as i32, 0);
    let frame = decode_frame(&out[..out_len]).unwrap();
    assert_eq!(frame.kind, FrameKind::Response);
    assert_eq!(frame.opcode, Opcode::OutputGet);
    assert_eq!(frame.status, Status::Ok);
    assert_eq!(
        output_lines(&frame).unwrap(),
        vec![
            "count 1".to_string(),
            "count 2".to_string(),
            "count 3".to_string(),
        ]
    );
}

#[test]
fn ffi_encodes_lifecycle_response_from_structured_runtime_state() {
    let mut out = [0u8; 192];
    let mut out_len = 0usize;
    let active = b"reader-clock";
    let mut process = [[0u8; 16]; 1];
    process[0][..4].copy_from_slice(b"main");
    let mut armed = [SqdpLifecycleTimer::default()];
    armed[0].app_id[..14].copy_from_slice(b"break-reminder");
    armed[0].event[..11].copy_from_slice(b"timer.break");

    let status = unsafe {
        squidvm_ffi::sqdp_encode_lifecycle_response(
            92,
            active.as_ptr(),
            active.len(),
            process.as_ptr().cast(),
            process.len(),
            process[0].len(),
            armed.as_ptr(),
            armed.len(),
            out.as_mut_ptr(),
            out.len(),
            &mut out_len,
        )
    };

    assert_eq!(status as i32, 0);
    let frame = decode_frame(&out[..out_len]).unwrap();
    assert_eq!(frame.kind, FrameKind::Response);
    assert_eq!(frame.opcode, Opcode::LifecycleGet);
    assert_eq!(frame.status, Status::Ok);
    assert_eq!(
        lifecycle_lines(&frame).unwrap(),
        vec![
            "active=reader-clock".to_string(),
            "process_stack[0]=main".to_string(),
            "armed_stack=".to_string(),
            "armed_stack[0]=break-reminder timer.break".to_string(),
        ]
    );
}

#[test]
fn ffi_encodes_resources_response_from_c_metrics() {
    let mut out = [0u8; 192];
    let mut out_len = 0usize;
    let key = b"vm_worker_stack_used_bytes";
    let metrics = [SqdpResourceMetric {
        key: key.as_ptr(),
        key_len: key.len(),
        value: 14_704,
    }];

    let status = unsafe {
        squidvm_ffi::sqdp_encode_resources_response(
            93,
            metrics.as_ptr(),
            metrics.len(),
            out.as_mut_ptr(),
            out.len(),
            &mut out_len,
        )
    };

    assert_eq!(status as i32, 0);
    let frame = decode_frame(&out[..out_len]).unwrap();
    assert_eq!(frame.kind, FrameKind::Response);
    assert_eq!(frame.opcode, Opcode::ResourcesGet);
    assert_eq!(frame.status, Status::Ok);
    assert_eq!(
        resource_values(&frame).unwrap(),
        vec![("vm_worker_stack_used_bytes".to_string(), 14_704)]
    );
}

#[test]
fn ffi_encodes_state_response_from_caller_owned_bytes() {
    let mut out = [0u8; 96];
    let mut out_len = 0usize;
    let state = b"SQST\x01\x02\x03\x04";

    let status = unsafe {
        squidvm_ffi::sqdp_encode_state_response(
            65,
            state.as_ptr(),
            state.len(),
            out.as_mut_ptr(),
            out.len(),
            &mut out_len,
        )
    };

    assert_eq!(status as i32, 0);
    let frame = decode_frame(&out[..out_len]).unwrap();
    assert_eq!(frame.kind, FrameKind::Response);
    assert_eq!(frame.opcode, Opcode::StateGet);
    assert_eq!(frame.status, Status::Ok);
    assert_eq!(frame.sequence, 65);
    assert_eq!(state_bytes(&frame).unwrap(), state);
}

#[test]
fn ffi_prepares_key_event_without_c_payload_staging() {
    let request = encode_frame(&key_request(48, "SELECT"));
    let mut event = [0u8; 16];
    let mut event_len = 0usize;

    let status = unsafe {
        squidvm_ffi::sqdp_prepare_key_event(
            request.as_ptr(),
            request.len(),
            event.as_mut_ptr(),
            event.len(),
            &mut event_len,
        )
    };

    assert_eq!(status as i32, 0);
    assert_eq!(&event[..event_len], b"key.SELECT");
}

#[test]
fn ffi_parses_wifi_profile_request_without_c_tlv_staging() {
    let request = encode_frame(&wifi_profile_set_request(
        76,
        "dev",
        "redacted-network",
        "redacted-password",
    ));
    let mut profile = SqdpWifiProfile::default();

    let status = unsafe {
        squidvm_ffi::sqdp_parse_wifi_profile_set_request(
            request.as_ptr(),
            request.len(),
            &mut profile,
        )
    };

    assert_eq!(status as i32, 0);
    assert_eq!(
        unsafe { core::slice::from_raw_parts(profile.profile, profile.profile_len) },
        b"dev"
    );
    assert_eq!(
        unsafe { core::slice::from_raw_parts(profile.ssid, profile.ssid_len) },
        b"redacted-network"
    );
    assert_eq!(
        unsafe { core::slice::from_raw_parts(profile.password, profile.password_len) },
        b"redacted-password"
    );
}

#[test]
fn ffi_parses_state_import_request_without_c_tlv_staging() {
    let state = vec![0x53, 0x51, 0x53, 0x54, 1, 2, 3, 4];
    let request = encode_frame(&state_import_request(72, state.clone()));
    let mut import = SqdpStateImport::default();

    let status = unsafe {
        squidvm_ffi::sqdp_parse_state_import_request(request.as_ptr(), request.len(), &mut import)
    };

    assert_eq!(status as i32, 0);
    assert_eq!(
        unsafe { core::slice::from_raw_parts(import.bytes, import.bytes_len) },
        state.as_slice()
    );
}

#[test]
fn ffi_parses_app_launch_request_without_c_tlv_staging() {
    let request = encode_frame(&app_launch_request(32, "reader-clock"));
    let mut launch = SqdpAppLaunch::default();

    let status = unsafe {
        squidvm_ffi::sqdp_parse_app_launch_request(request.as_ptr(), request.len(), &mut launch)
    };

    assert_eq!(status as i32, 0);
    assert_eq!(
        unsafe { core::slice::from_raw_parts(launch.app_id, launch.app_id_len) },
        b"reader-clock"
    );
}

#[test]
fn ffi_parses_event_dispatch_request_without_c_tlv_staging() {
    let request = encode_frame(&event_dispatch_request(49, "reader-clock", "timer.clock"));
    let mut event = SqdpEventDispatch::default();

    let status = unsafe {
        squidvm_ffi::sqdp_parse_event_dispatch_request(request.as_ptr(), request.len(), &mut event)
    };

    assert_eq!(status as i32, 0);
    assert_eq!(
        unsafe { core::slice::from_raw_parts(event.app_id, event.app_id_len) },
        b"reader-clock"
    );
    assert_eq!(
        unsafe { core::slice::from_raw_parts(event.event, event.event_len) },
        b"timer.clock"
    );
}

#[test]
fn ffi_validates_install_session_progress_with_caller_owned_storage() {
    let bytes = b"hello";
    let crc = crc32fast::hash(bytes);
    let begin = encode_frame(&app_install_begin_request(
        1,
        "ffi-app",
        bytes.len() as u64,
        crc as u64,
    ));
    let chunk = encode_frame(&app_install_chunk_request(2, 0, bytes.to_vec()));
    let commit = encode_frame(&app_install_commit_request(3));
    let mut session = SqdpTransferSession::default();
    let mut action = SqdpAction::default();

    let status = unsafe {
        squidvm_ffi::sqdp_prepare_transfer_begin(
            begin.as_ptr(),
            begin.len(),
            &mut session,
            &mut action,
        )
    };
    assert_eq!(status as i32, 0);
    assert_eq!(action.kind, SqdpActionKind::BeginInstall);
    assert_eq!(session.app_id_string(), "ffi-app");
    assert!(
        !session.active,
        "storage begin should activate the session after it succeeds"
    );

    assert_eq!(
        session.set_staging_path_for_test("/tmp/ffi-app.staged") as i32,
        0
    );
    session.active = true;

    let status = unsafe {
        squidvm_ffi::sqdp_prepare_transfer_chunk(
            chunk.as_ptr(),
            chunk.len(),
            &mut session,
            &mut action,
        )
    };
    assert_eq!(status as i32, 0);
    assert_eq!(action.kind, SqdpActionKind::WriteInstallChunk);
    assert_eq!(action.offset, 0);
    assert_eq!(
        unsafe { core::slice::from_raw_parts(action.bytes, action.bytes_len) },
        bytes
    );

    let status = unsafe {
        squidvm_ffi::sqdp_complete_transfer_chunk(&mut session, action.bytes, action.bytes_len)
    };
    assert_eq!(status as i32, 0);

    let status = unsafe {
        squidvm_ffi::sqdp_prepare_transfer_commit(
            commit.as_ptr(),
            commit.len(),
            &mut session,
            &mut action,
        )
    };
    assert_eq!(status as i32, 0);
    assert_eq!(action.kind, SqdpActionKind::CommitInstall);
}

#[test]
fn ffi_transfer_staging_path_uses_internal_firmware_capacity() {
    let mut session = SqdpTransferSession::default();
    let max_app_id = "a".repeat(47);
    let longest_install_staging_path = format!("/sq/apps/{max_app_id}/main.sqbc.tmp");

    assert_eq!(SQDP_STAGING_PATH_CAP, 80);
    assert_eq!(longest_install_staging_path.len(), 70);
    assert_eq!(
        session.set_staging_path_for_test(&longest_install_staging_path) as i32,
        0
    );

    let too_long = "x".repeat(SQDP_STAGING_PATH_CAP);
    assert_ne!(session.set_staging_path_for_test(&too_long) as i32, 0);
}

#[test]
fn ffi_validates_resource_session_progress_with_caller_owned_storage() {
    let bytes = b"resource";
    let crc = crc32fast::hash(bytes);
    let begin = encode_frame(&resource_install_begin_request(
        1,
        "ffi-app",
        "assets/main.bin",
        bytes.len() as u64,
        crc as u64,
    ));
    let chunk = encode_frame(&resource_install_chunk_request(2, 0, bytes.to_vec()));
    let commit = encode_frame(&resource_install_commit_request(3));
    let mut session = SqdpResourceSession::default();
    let mut action = SqdpAction::default();

    let status = unsafe {
        squidvm_ffi::sqdp_prepare_resource_begin(
            begin.as_ptr(),
            begin.len(),
            &mut session,
            &mut action,
        )
    };
    assert_eq!(status as i32, 0);
    assert_eq!(action.kind, SqdpActionKind::BeginResourceInstall);
    assert_eq!(session.app_id_string(), "ffi-app");
    assert_eq!(session.resource_path_string(), "assets/main.bin");

    assert_eq!(
        session.set_staging_path_for_test("/tmp/resource.staged") as i32,
        0
    );
    session.active = true;

    let status = unsafe {
        squidvm_ffi::sqdp_prepare_resource_chunk(
            chunk.as_ptr(),
            chunk.len(),
            &mut session,
            &mut action,
        )
    };
    assert_eq!(status as i32, 0);
    assert_eq!(action.kind, SqdpActionKind::WriteResourceChunk);
    assert_eq!(
        unsafe { core::slice::from_raw_parts(action.bytes, action.bytes_len) },
        bytes
    );
    assert_eq!(
        unsafe {
            squidvm_ffi::sqdp_complete_resource_chunk(&mut session, action.bytes, action.bytes_len)
        } as i32,
        0
    );

    let status = unsafe {
        squidvm_ffi::sqdp_prepare_resource_commit(
            commit.as_ptr(),
            commit.len(),
            &mut session,
            &mut action,
        )
    };
    assert_eq!(status as i32, 0);
    assert_eq!(action.kind, SqdpActionKind::CommitResourceInstall);
}
