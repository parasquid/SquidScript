#![cfg(feature = "alloc")]

use squid_device_protocol::{
    app_install_begin_request, app_install_chunk_request, app_install_commit_request,
    content_install_begin_request, content_install_chunk_request, content_install_commit_request,
    encode_frame, resource_install_begin_request, resource_install_chunk_request,
    resource_install_commit_request, temp_run_begin_request, temp_run_chunk_request,
    temp_run_commit_request, DeviceRequest, HostAction, ProtocolSessions, SessionError,
    MAX_APP_BYTES, MAX_APP_ID_LEN, MAX_CONTENT_NAME_BYTES, MAX_RESOURCE_BYTES,
};

#[test]
fn rust_session_engine_drives_large_max_name_content_begin_chunk_commit() {
    let mut sessions = ProtocolSessions::default();
    let name = format!("{}.binbook", "a".repeat(MAX_CONTENT_NAME_BYTES - 8));
    let bytes = vec![0x5a; 2048];
    let crc = crc32fast::hash(&bytes);

    let begin = encode_frame(&content_install_begin_request(
        40,
        &name,
        bytes.len() as u64,
        crc as u64,
    ));
    let request = DeviceRequest::decode(&begin).unwrap();
    assert_eq!(
        sessions.next_action(&request).unwrap(),
        HostAction::BeginContentInstall {
            name: &name,
            total_len: bytes.len(),
        }
    );
    let path = format!("books/{name}");
    sessions.complete_begin_content_install(&path).unwrap();

    for (index, chunk) in bytes.chunks(512).enumerate() {
        let offset = index * 512;
        let frame = encode_frame(&content_install_chunk_request(
            41 + index as u32,
            offset as u64,
            chunk.to_vec(),
        ));
        let request = DeviceRequest::decode(&frame).unwrap();
        assert_eq!(
            sessions.next_action(&request).unwrap(),
            HostAction::WriteContentChunk {
                path: &path,
                offset,
                bytes: chunk,
            }
        );
        sessions.complete_content_chunk(chunk).unwrap();
    }

    let commit = encode_frame(&content_install_commit_request(45));
    let request = DeviceRequest::decode(&commit).unwrap();
    assert_eq!(
        sessions.next_action(&request).unwrap(),
        HostAction::CommitContentInstall {
            name: &name,
            path: &path,
        }
    );
    sessions.complete_content_commit();
    assert_eq!(sessions.next_action(&request), Err(SessionError::Inactive));
}

#[test]
fn rust_session_engine_drives_installed_app_begin_chunk_commit() {
    let mut sessions = ProtocolSessions::default();
    let bytes = b"hello";
    let crc = crc32fast::hash(bytes);

    let begin = encode_frame(&app_install_begin_request(30, "framed-app", 5, crc as u64));
    let request = DeviceRequest::decode(&begin).expect("request decodes");
    let action = sessions.next_action(&request).expect("begin accepted");
    assert_eq!(
        action,
        HostAction::BeginInstall {
            app_id: "framed-app",
            total_len: 5
        }
    );
    sessions
        .complete_begin_install("/sqtest/tmp/framed-app.staged")
        .expect("staging path accepted");

    let chunk = encode_frame(&app_install_chunk_request(31, 0, bytes[..3].to_vec()));
    let request = DeviceRequest::decode(&chunk).expect("chunk decodes");
    let action = sessions.next_action(&request).expect("chunk accepted");
    assert_eq!(
        action,
        HostAction::WriteInstallChunk {
            staging_path: "/sqtest/tmp/framed-app.staged",
            offset: 0,
            bytes: &bytes[..3]
        }
    );
    sessions.complete_install_chunk(&bytes[..3]).unwrap();

    let chunk = encode_frame(&app_install_chunk_request(32, 3, bytes[3..].to_vec()));
    let request = DeviceRequest::decode(&chunk).expect("chunk decodes");
    let action = sessions.next_action(&request).expect("chunk accepted");
    assert_eq!(
        action,
        HostAction::WriteInstallChunk {
            staging_path: "/sqtest/tmp/framed-app.staged",
            offset: 3,
            bytes: &bytes[3..]
        }
    );
    sessions.complete_install_chunk(&bytes[3..]).unwrap();

    let commit = encode_frame(&app_install_commit_request(33));
    let request = DeviceRequest::decode(&commit).expect("commit decodes");
    assert_eq!(
        sessions.next_action(&request).expect("commit accepted"),
        HostAction::CommitInstall {
            app_id: "framed-app",
            staging_path: "/sqtest/tmp/framed-app.staged"
        }
    );
    sessions.complete_install_commit();
    assert!(matches!(
        sessions.next_action(&request),
        Err(SessionError::Inactive)
    ));
}

#[test]
fn rust_session_engine_rejects_out_of_order_or_bad_crc_chunks() {
    let mut sessions = ProtocolSessions::default();
    let bytes = b"hello";
    let crc = crc32fast::hash(bytes);

    let begin = encode_frame(&app_install_begin_request(1, "bad-order", 5, crc as u64));
    let request = DeviceRequest::decode(&begin).unwrap();
    assert!(matches!(
        sessions.next_action(&request).unwrap(),
        HostAction::BeginInstall { .. }
    ));
    sessions.complete_begin_install("/tmp/staged").unwrap();

    let chunk = encode_frame(&app_install_chunk_request(2, 1, bytes[..2].to_vec()));
    let request = DeviceRequest::decode(&chunk).unwrap();
    assert_eq!(sessions.next_action(&request), Err(SessionError::Offset));

    let chunk = encode_frame(&app_install_chunk_request(3, 0, bytes.to_vec()));
    let request = DeviceRequest::decode(&chunk).unwrap();
    assert!(matches!(
        sessions.next_action(&request).unwrap(),
        HostAction::WriteInstallChunk { .. }
    ));
    sessions.complete_install_chunk(bytes).unwrap();

    let begin = encode_frame(&app_install_begin_request(4, "bad-crc", 5, 0));
    let request = DeviceRequest::decode(&begin).unwrap();
    assert!(matches!(
        sessions.next_action(&request).unwrap(),
        HostAction::BeginInstall { .. }
    ));
    sessions.complete_begin_install("/tmp/staged").unwrap();
    let chunk = encode_frame(&app_install_chunk_request(5, 0, bytes.to_vec()));
    let request = DeviceRequest::decode(&chunk).unwrap();
    assert!(matches!(
        sessions.next_action(&request).unwrap(),
        HostAction::WriteInstallChunk { .. }
    ));
    sessions.complete_install_chunk(bytes).unwrap();
    let commit = encode_frame(&app_install_commit_request(6));
    let request = DeviceRequest::decode(&commit).unwrap();
    assert_eq!(sessions.next_action(&request), Err(SessionError::Crc));
}

#[test]
fn rust_session_engine_covers_temp_run_and_resource_sessions() {
    let mut sessions = ProtocolSessions::default();
    let bytes = b"sqbc";
    let crc = crc32fast::hash(bytes);

    let begin = encode_frame(&temp_run_begin_request(
        10,
        "temp-app",
        bytes.len() as u64,
        crc as u64,
    ));
    let request = DeviceRequest::decode(&begin).unwrap();
    assert_eq!(
        sessions.next_action(&request).unwrap(),
        HostAction::BeginTempRun {
            app_id: "temp-app",
            total_len: bytes.len()
        }
    );
    sessions
        .complete_begin_temp_run("/sqtest/tmp/temp.sqbc")
        .unwrap();
    let chunk = encode_frame(&temp_run_chunk_request(11, 0, bytes.to_vec()));
    let request = DeviceRequest::decode(&chunk).unwrap();
    assert_eq!(
        sessions.next_action(&request).unwrap(),
        HostAction::WriteTempRunChunk {
            staging_path: "/sqtest/tmp/temp.sqbc",
            offset: 0,
            bytes
        }
    );
    sessions.complete_temp_run_chunk(bytes).unwrap();
    let commit = encode_frame(&temp_run_commit_request(12));
    let request = DeviceRequest::decode(&commit).unwrap();
    assert_eq!(
        sessions.next_action(&request).unwrap(),
        HostAction::CommitTempRun {
            app_id: "temp-app",
            staging_path: "/sqtest/tmp/temp.sqbc",
            total_len: bytes.len()
        }
    );

    let resource = b"resource";
    let crc = crc32fast::hash(resource);
    let begin = encode_frame(&resource_install_begin_request(
        20,
        "temp-app",
        "icons/main.bin",
        resource.len() as u64,
        crc as u64,
    ));
    let request = DeviceRequest::decode(&begin).unwrap();
    assert_eq!(
        sessions.next_action(&request).unwrap(),
        HostAction::BeginResourceInstall {
            app_id: "temp-app",
            resource_path: "icons/main.bin",
            total_len: resource.len()
        }
    );
    sessions
        .complete_begin_resource_install("/sqtest/tmp/resource.staged")
        .unwrap();
    let chunk = encode_frame(&resource_install_chunk_request(21, 0, resource.to_vec()));
    let request = DeviceRequest::decode(&chunk).unwrap();
    assert_eq!(
        sessions.next_action(&request).unwrap(),
        HostAction::WriteResourceChunk {
            staging_path: "/sqtest/tmp/resource.staged",
            offset: 0,
            bytes: resource
        }
    );
    sessions.complete_resource_chunk(resource).unwrap();
    let commit = encode_frame(&resource_install_commit_request(22));
    let request = DeviceRequest::decode(&commit).unwrap();
    assert_eq!(
        sessions.next_action(&request).unwrap(),
        HostAction::CommitResourceInstall {
            app_id: "temp-app",
            resource_path: "icons/main.bin",
            staging_path: "/sqtest/tmp/resource.staged"
        }
    );
}

#[test]
fn rust_session_engine_rejects_oversized_install_before_storage_work() {
    let mut sessions = ProtocolSessions::default();
    let begin = encode_frame(&app_install_begin_request(
        1,
        "too-large",
        MAX_APP_BYTES as u64 + 1,
        0,
    ));
    let request = DeviceRequest::decode(&begin).unwrap();

    assert_eq!(sessions.next_action(&request), Err(SessionError::TooLarge));
}

#[test]
fn rust_session_engine_allows_resources_larger_than_app_install_cap() {
    let mut sessions = ProtocolSessions::default();
    let resource_len = MAX_APP_BYTES + 1;
    assert!(resource_len <= MAX_RESOURCE_BYTES);
    let begin = encode_frame(&resource_install_begin_request(
        1,
        "book-reader",
        "books/sample.binbook",
        resource_len as u64,
        0,
    ));
    let request = DeviceRequest::decode(&begin).unwrap();

    assert_eq!(
        sessions.next_action(&request),
        Ok(HostAction::BeginResourceInstall {
            app_id: "book-reader",
            resource_path: "books/sample.binbook",
            total_len: resource_len
        })
    );
}

#[test]
fn rust_session_engine_bounds_app_ids_for_firmware_ram() {
    let mut sessions = ProtocolSessions::default();
    let accepted = "a".repeat(MAX_APP_ID_LEN - 1);
    let rejected = "a".repeat(MAX_APP_ID_LEN);
    let bytes = b"sqbc";
    let crc = crc32fast::hash(bytes);

    let begin = encode_frame(&app_install_begin_request(
        1,
        accepted,
        bytes.len() as u64,
        crc as u64,
    ));
    let request = DeviceRequest::decode(&begin).unwrap();
    assert!(matches!(
        sessions.next_action(&request).unwrap(),
        HostAction::BeginInstall { .. }
    ));

    let begin = encode_frame(&app_install_begin_request(
        2,
        rejected,
        bytes.len() as u64,
        crc as u64,
    ));
    let request = DeviceRequest::decode(&begin).unwrap();
    assert_eq!(
        sessions.next_action(&request),
        Err(SessionError::AppIdTooLong)
    );
    assert_eq!(MAX_APP_ID_LEN, 40);
}
