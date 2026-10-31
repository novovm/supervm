use std::io;
use std::net::{SocketAddr, UdpSocket};

pub const MAX_UDP_SEGMENTED_DATAGRAMS: usize = 64;
pub const MAX_UDP_SEGMENTED_BATCH_BYTES: usize = 65_507;

/// Sets Windows' UDP_SEND_MSG_SIZE; zero restores ordinary datagram sends.
///
/// The socket owner must serialize this setting with every send, including
/// control messages. Shared sockets must not opt in without that ownership.
pub fn set_udp_send_segment_size(socket: &UdpSocket, segment_size: usize) -> io::Result<()> {
    if segment_size > MAX_UDP_SEGMENTED_BATCH_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "UDP segment size exceeds the bounded batch limit",
        ));
    }
    set_segment_size_platform(socket, segment_size as u32)
}

/// Submits complete datagrams in one Windows UDP segmentation send.
///
/// The caller must already have successfully set `segment_size` on this exact
/// socket and serialize all writers until this call completes. Every datagram
/// except the optional short final one must have that size. Datagram bytes are
/// concatenated unchanged; their boundaries are restored by the socket option.
///
/// Success returns the complete datagram count, not peer receipt. Errors are
/// propagated without retry or fallback; a short write is never counted as a
/// successful prefix. This function does not alter the socket setting.
pub fn send_udp_segmented_batch(
    socket: &UdpSocket,
    target: SocketAddr,
    datagrams: &[&[u8]],
    segment_size: usize,
) -> io::Result<usize> {
    if datagrams.is_empty() {
        return Ok(0);
    }
    let total = validate_batch(datagrams, segment_size)?;
    send_segmented_platform(socket, target, datagrams, total)
}

fn validate_batch(datagrams: &[&[u8]], segment_size: usize) -> io::Result<usize> {
    if segment_size == 0 || segment_size > MAX_UDP_SEGMENTED_BATCH_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "segmented UDP sends require a nonzero bounded segment size",
        ));
    }
    if datagrams.len() > MAX_UDP_SEGMENTED_DATAGRAMS {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "segmented UDP batch has too many datagrams",
        ));
    }
    let mut total = 0;
    for (index, datagram) in datagrams.iter().enumerate() {
        if datagram.is_empty()
            || datagram.len() > segment_size
            || (index + 1 < datagrams.len() && datagram.len() != segment_size)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "only the final UDP segment may be shorter than the segment size",
            ));
        }
        total += datagram.len();
        if total > MAX_UDP_SEGMENTED_BATCH_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "segmented UDP batch exceeds its byte limit",
            ));
        }
    }
    Ok(total)
}

#[cfg(windows)]
fn set_segment_size_platform(socket: &UdpSocket, segment_size: u32) -> io::Result<()> {
    use std::os::windows::io::AsRawSocket;

    const IPPROTO_UDP: i32 = 17;
    const UDP_SEND_MSG_SIZE: i32 = 2;
    #[link(name = "Ws2_32")]
    extern "system" {
        fn setsockopt(
            socket: usize,
            level: i32,
            option: i32,
            value: *const u8,
            value_len: i32,
        ) -> i32;
        fn WSAGetLastError() -> i32;
    }

    // WSASetUdpSendMessageSize is the SDK's inline wrapper for this DWORD option.
    let result = unsafe {
        setsockopt(
            socket.as_raw_socket() as usize,
            IPPROTO_UDP,
            UDP_SEND_MSG_SIZE,
            (&segment_size as *const u32).cast(),
            std::mem::size_of::<u32>() as i32,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(unsafe { WSAGetLastError() }))
    }
}

#[cfg(not(windows))]
fn set_segment_size_platform(_socket: &UdpSocket, _segment_size: u32) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Windows UDP segmentation is not available on this platform",
    ))
}

#[cfg(windows)]
fn send_segmented_platform(
    socket: &UdpSocket,
    target: SocketAddr,
    datagrams: &[&[u8]],
    total: usize,
) -> io::Result<usize> {
    let mut payload = Vec::with_capacity(total);
    for datagram in datagrams {
        payload.extend_from_slice(datagram);
    }
    let written = socket.send_to(&payload, target)?;
    if written != total {
        return Err(io::Error::new(
            io::ErrorKind::WriteZero,
            "segmented UDP send did not accept the complete batch",
        ));
    }
    Ok(datagrams.len())
}

#[cfg(not(windows))]
fn send_segmented_platform(
    _socket: &UdpSocket,
    _target: SocketAddr,
    _datagrams: &[&[u8]],
    _total: usize,
) -> io::Result<usize> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Windows UDP segmentation is not available on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segmented_batch_accepts_equal_segments_and_only_a_short_final_segment() {
        let full = [1; 768];
        let tail = [2; 123];
        assert_eq!(validate_batch(&[&full, &full], 768).unwrap(), 1_536);
        assert_eq!(validate_batch(&[&full, &full, &tail], 768).unwrap(), 1_659);
        for datagrams in [vec![&tail[..], &full[..]], vec![&full[..], &[][..]]] {
            assert_eq!(
                validate_batch(&datagrams, 768).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
        assert_eq!(
            validate_batch(&[&full, &full], 767).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            validate_batch(&[&tail], 0).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn segmented_batch_preserves_original_quantum_and_bounds_total_scratch_bytes() {
        let fragment = [3; 1_177];
        let datagrams = [&fragment[..]; 32];
        assert_eq!(validate_batch(&datagrams, 1_177).unwrap(), 37_664);
        let byte_boundary = vec![4; MAX_UDP_SEGMENTED_BATCH_BYTES];
        assert_eq!(
            validate_batch(&[&byte_boundary], byte_boundary.len()).unwrap(),
            MAX_UDP_SEGMENTED_BATCH_BYTES
        );
        assert_eq!(
            validate_batch(&[&byte_boundary, &[5]], byte_boundary.len())
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        let tiny = [6];
        assert_eq!(
            validate_batch(&[&tiny[..]; MAX_UDP_SEGMENTED_DATAGRAMS + 1], 1)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            validate_batch(&[&tiny[..]; MAX_UDP_SEGMENTED_DATAGRAMS], 1).unwrap(),
            MAX_UDP_SEGMENTED_DATAGRAMS
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_segmented_loopback_restores_exact_datagrams_and_option_zero_single_send(
    ) -> io::Result<()> {
        use std::time::Duration;

        let sender = UdpSocket::bind("127.0.0.1:0")?;
        let receiver = UdpSocket::bind("127.0.0.1:0")?;
        receiver.set_read_timeout(Some(Duration::from_secs(1)))?;
        let target = receiver.local_addr()?;
        let first = [0x11; 768];
        let second = [0x22; 768];
        let tail = [0x33; 123];
        set_udp_send_segment_size(&sender, 768)?;
        assert_eq!(
            send_udp_segmented_batch(&sender, target, &[&first, &second, &tail], 768)?,
            3
        );
        let mut received = [0; 2_048];
        for expected in [&first[..], &second[..], &tail[..]] {
            let (length, source) = receiver.recv_from(&mut received)?;
            assert_eq!(source, sender.local_addr()?);
            assert_eq!(&received[..length], expected);
        }

        set_udp_send_segment_size(&sender, 0)?;
        let single = [0x44; 1_024];
        assert_eq!(sender.send_to(&single, target)?, single.len());
        let (length, source) = receiver.recv_from(&mut received)?;
        assert_eq!(source, sender.local_addr()?);
        assert_eq!(&received[..length], single);
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn windows_invalid_segmented_batches_send_nothing() -> io::Result<()> {
        use std::time::Duration;

        let sender = UdpSocket::bind("127.0.0.1:0")?;
        let receiver = UdpSocket::bind("127.0.0.1:0")?;
        receiver.set_read_timeout(Some(Duration::from_millis(30)))?;
        let target = receiver.local_addr()?;
        set_udp_send_segment_size(&sender, 768)?;
        let full = [1; 768];
        let short = [2; 123];
        let too_large = [3; 769];
        let over_count = vec![&short[..]; MAX_UDP_SEGMENTED_DATAGRAMS + 1];
        for datagrams in [
            vec![&short[..], &full[..]],
            vec![&full[..], &too_large[..]],
            vec![&full[..], &[][..]],
            over_count,
        ] {
            assert_eq!(
                send_udp_segmented_batch(&sender, target, &datagrams, 768)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidInput
            );
        }
        let mut received = [0; 1_024];
        let large = [4; 1_177];
        assert_eq!(
            send_udp_segmented_batch(&sender, target, &[&large[..]; 64], 1_177)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        let error = receiver.recv_from(&mut received).unwrap_err();
        assert!(matches!(
            error.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
        ));
        Ok(())
    }
}
