#![deny(unsafe_op_in_unsafe_fn)]

use std::io;
use std::net::{SocketAddr, UdpSocket};

#[cfg(target_os = "linux")]
use std::mem;
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
#[cfg(target_os = "linux")]
use std::ptr;

pub fn sendmmsg_batch(
    socket: &UdpSocket,
    target: SocketAddr,
    datagrams: &[&[u8]],
) -> io::Result<usize> {
    if datagrams.is_empty() {
        return Ok(0);
    }
    sendmmsg_batch_platform(socket, target, datagrams)
}

#[cfg(not(target_os = "linux"))]
fn sendmmsg_batch_platform(
    _socket: &UdpSocket,
    _target: SocketAddr,
    _datagrams: &[&[u8]],
) -> io::Result<usize> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "sendmmsg is not available on this platform",
    ))
}

#[cfg(target_os = "linux")]
fn sendmmsg_batch_platform(
    socket: &UdpSocket,
    target: SocketAddr,
    datagrams: &[&[u8]],
) -> io::Result<usize> {
    let fd = socket.as_raw_fd();
    let (mut storage, storage_len) = raw_sockaddr(target);
    let mut iovecs = datagrams
        .iter()
        .map(|datagram| libc::iovec {
            iov_base: datagram.as_ptr() as *mut libc::c_void,
            iov_len: datagram.len(),
        })
        .collect::<Vec<_>>();
    let mut headers = (0..datagrams.len())
        .map(|index| libc::mmsghdr {
            msg_hdr: libc::msghdr {
                msg_name: (&mut storage as *mut libc::sockaddr_storage).cast::<libc::c_void>(),
                msg_namelen: storage_len,
                msg_iov: (&mut iovecs[index]) as *mut libc::iovec,
                msg_iovlen: 1,
                msg_control: ptr::null_mut(),
                msg_controllen: 0,
                msg_flags: 0,
            },
            msg_len: 0,
        })
        .collect::<Vec<_>>();
    let sent = unsafe {
        libc::sendmmsg(
            fd,
            headers.as_mut_ptr(),
            headers.len().try_into().unwrap_or(u32::MAX),
            0,
        )
    };
    if sent < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(sent as usize)
    }
}

#[cfg(target_os = "linux")]
fn raw_sockaddr(addr: SocketAddr) -> (libc::sockaddr_storage, libc::socklen_t) {
    let mut storage = unsafe { mem::zeroed::<libc::sockaddr_storage>() };
    match addr {
        SocketAddr::V4(addr) => {
            let raw = libc::sockaddr_in {
                sin_family: libc::AF_INET as libc::sa_family_t,
                sin_port: addr.port().to_be(),
                sin_addr: libc::in_addr {
                    s_addr: u32::from_ne_bytes(addr.ip().octets()),
                },
                sin_zero: [0; 8],
            };
            unsafe {
                ptr::write(
                    (&mut storage as *mut libc::sockaddr_storage).cast::<libc::sockaddr_in>(),
                    raw,
                );
            }
            (
                storage,
                mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
            )
        }
        SocketAddr::V6(addr) => {
            let raw = libc::sockaddr_in6 {
                sin6_family: libc::AF_INET6 as libc::sa_family_t,
                sin6_port: addr.port().to_be(),
                sin6_flowinfo: addr.flowinfo(),
                sin6_addr: libc::in6_addr {
                    s6_addr: addr.ip().octets(),
                },
                sin6_scope_id: addr.scope_id(),
            };
            unsafe {
                ptr::write(
                    (&mut storage as *mut libc::sockaddr_storage).cast::<libc::sockaddr_in6>(),
                    raw,
                );
            }
            (
                storage,
                mem::size_of::<libc::sockaddr_in6>() as libc::socklen_t,
            )
        }
    }
}
