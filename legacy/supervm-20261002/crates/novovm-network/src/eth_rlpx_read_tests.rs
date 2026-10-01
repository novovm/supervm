enum ReadEvent {
    Bytes(Vec<u8>),
    Failure(std::io::Error),
}

struct ScriptedFrameReader {
    events: std::collections::VecDeque<ReadEvent>,
}

impl std::io::Read for ScriptedFrameReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match self.events.pop_front() {
            Some(ReadEvent::Bytes(bytes)) => {
                let count = buffer.len().min(bytes.len());
                buffer[..count].copy_from_slice(&bytes[..count]);
                if count < bytes.len() {
                    self.events
                        .push_front(ReadEvent::Bytes(bytes[count..].to_vec()));
                }
                Ok(count)
            }
            Some(ReadEvent::Failure(error)) => Err(error),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "暂时无数据",
            )),
        }
    }
}

fn scripted_frame_reader(events: Vec<ReadEvent>) -> ScriptedFrameReader {
    ScriptedFrameReader {
        events: events.into(),
    }
}

#[test]
fn frame_read_idle_timeout_uses_error_kind_and_preserves_next_frame() {
    for kind in [std::io::ErrorKind::WouldBlock, std::io::ErrorKind::TimedOut] {
        let (mut writer, mut reader) = build_test_session_pair();
        let mut wire = Vec::new();
        eth_rlpx_write_wire_frame_v1(&mut wire, &mut writer, ETH_RLPX_P2P_PING_MSG, b"ping")
            .unwrap();
        let mut stream = scripted_frame_reader(vec![
            ReadEvent::Failure(std::io::Error::new(kind, "暂时无数据")),
            ReadEvent::Bytes(wire),
        ]);
        let error = eth_rlpx_read_wire_frame_v1(&mut stream, &mut reader).unwrap_err();
        assert!(
            error.starts_with("rlpx_frame_header_read_failed:read_timeout read=0/16 "),
            "{error}"
        );
        assert_eq!(
            eth_rlpx_read_wire_frame_v1(&mut stream, &mut reader).unwrap(),
            (ETH_RLPX_P2P_PING_MSG, b"ping".to_vec())
        );
    }
}

#[test]
fn frame_read_partial_timeout_kinds_retry_without_losing_bytes() {
    for kind in [std::io::ErrorKind::WouldBlock, std::io::ErrorKind::TimedOut] {
        let mut stream = scripted_frame_reader(vec![
            ReadEvent::Bytes(vec![1, 2]),
            ReadEvent::Failure(std::io::Error::new(kind, "暂时无数据")),
            ReadEvent::Bytes(vec![3, 4]),
        ]);
        let mut buffer = [0; 4];
        eth_rlpx_read_exact_with_partial_deadline_v1(
            &mut stream,
            &mut buffer,
            "rlpx_frame_body_read_failed",
            Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(buffer, [1, 2, 3, 4]);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn frame_read_linux_eagain_is_retryable_after_partial_data() {
    let error = std::io::Error::from_raw_os_error(11);
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
    let mut stream = scripted_frame_reader(vec![
        ReadEvent::Bytes(vec![1]),
        ReadEvent::Failure(error),
        ReadEvent::Bytes(vec![2]),
    ]);
    let mut buffer = [0; 2];
    eth_rlpx_read_exact_with_partial_deadline_v1(
        &mut stream,
        &mut buffer,
        "rlpx_frame_header_read_failed",
        Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(buffer, [1, 2]);
}

#[test]
fn frame_read_interrupted_retries_before_and_after_partial_data() {
    let mut stream = scripted_frame_reader(vec![
        ReadEvent::Failure(std::io::Error::from(std::io::ErrorKind::Interrupted)),
        ReadEvent::Bytes(vec![1]),
        ReadEvent::Failure(std::io::Error::from(std::io::ErrorKind::Interrupted)),
        ReadEvent::Bytes(vec![2]),
    ]);
    let mut buffer = [0; 2];
    eth_rlpx_read_exact_with_partial_deadline_v1(
        &mut stream,
        &mut buffer,
        "rlpx_frame_header_read_failed",
        Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(buffer, [1, 2]);
}

#[test]
fn frame_read_error_text_does_not_override_fatal_kind() {
    let mut stream = scripted_frame_reader(vec![
        ReadEvent::Bytes(vec![1]),
        ReadEvent::Failure(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "timed out",
        )),
        ReadEvent::Bytes(vec![2]),
    ]);
    let mut buffer = [0; 2];
    let error = eth_rlpx_read_exact_with_partial_deadline_v1(
        &mut stream,
        &mut buffer,
        "rlpx_frame_header_read_failed",
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(error.contains("read=1/2"));
    assert_eq!(stream.events.len(), 1);
}

#[test]
fn frame_read_timeout_after_consumed_frame_sections_is_not_idle() {
    for prefix_size in [16, 32, 48] {
        let (mut writer, mut reader) = build_test_session_pair();
        let mut wire = Vec::new();
        eth_rlpx_write_wire_frame_v1(&mut wire, &mut writer, ETH_RLPX_P2P_PING_MSG, b"ping")
            .unwrap();
        let mut stream =
            scripted_frame_reader(vec![ReadEvent::Bytes(wire[..prefix_size].to_vec())]);
        let error = eth_rlpx_read_wire_frame_v1(&mut stream, &mut reader).unwrap_err();
        assert!(error.contains(":read_timeout read=0/16 "), "{error}");
        assert!(
            !error.starts_with("rlpx_frame_header_read_failed:"),
            "{error}"
        );
    }
}
