//! Closed, authenticated framed carriers. Construction requires a real stream
//! established by one of this crate's transport owners; callers cannot supply
//! an exporter, peer identity, or an alleged verified binding.

#[cfg(feature = "iroh-transport")]
use crate::iroh_transport::{IrohReadHalfV1, IrohStreamIoFailureV1, IrohStreamV1, IrohWriteHalfV1};
#[cfg(feature = "tls-transport")]
use crate::tls_transport::{TlsReadHalfV1, TlsStreamIoFailureV1, TlsStreamV1, TlsWriteHalfV1};
use crate::transport_binding::VerifiedTransportBinding;

pub struct CarrierStreamV1<'scope> {
    inner: Stream<'scope>,
}

enum Stream<'scope> {
    #[cfg(feature = "iroh-transport")]
    Iroh(IrohStreamV1<'scope>),
    #[cfg(feature = "tls-transport")]
    Tls(Box<TlsStreamV1<'scope>>),
}

#[cfg(feature = "iroh-transport")]
impl<'scope> From<IrohStreamV1<'scope>> for CarrierStreamV1<'scope> {
    fn from(stream: IrohStreamV1<'scope>) -> Self {
        Self {
            inner: Stream::Iroh(stream),
        }
    }
}

#[cfg(feature = "tls-transport")]
impl<'scope> From<TlsStreamV1<'scope>> for CarrierStreamV1<'scope> {
    fn from(stream: TlsStreamV1<'scope>) -> Self {
        Self {
            inner: Stream::Tls(Box::new(stream)),
        }
    }
}

pub struct CarrierWriteHalfV1<'stream> {
    inner: WriteHalf<'stream>,
}

enum WriteHalf<'stream> {
    #[cfg(feature = "iroh-transport")]
    Iroh(IrohWriteHalfV1<'stream>),
    #[cfg(feature = "tls-transport")]
    Tls(TlsWriteHalfV1<'stream>),
}

pub struct CarrierReadHalfV1<'stream> {
    inner: ReadHalf<'stream>,
}

enum ReadHalf<'stream> {
    #[cfg(feature = "iroh-transport")]
    Iroh(IrohReadHalfV1<'stream>),
    #[cfg(feature = "tls-transport")]
    Tls(TlsReadHalfV1<'stream>),
}

impl CarrierStreamV1<'_> {
    pub fn binding(&self) -> anyhow::Result<VerifiedTransportBinding> {
        match &self.inner {
            #[cfg(feature = "iroh-transport")]
            Stream::Iroh(stream) => stream.binding(),
            #[cfg(feature = "tls-transport")]
            Stream::Tls(stream) => stream.binding(),
        }
    }

    pub fn remote_endpoint_key(&self) -> anyhow::Result<[u8; 32]> {
        match &self.inner {
            #[cfg(feature = "iroh-transport")]
            Stream::Iroh(stream) => stream.remote_endpoint_key(),
            #[cfg(feature = "tls-transport")]
            Stream::Tls(stream) => stream.remote_endpoint_key(),
        }
    }

    /// This is the underlying carrier's observation, not a privacy credential.
    pub fn selected_path(&self) -> anyhow::Result<&'static str> {
        match &self.inner {
            #[cfg(feature = "iroh-transport")]
            Stream::Iroh(stream) => stream.selected_path(),
            #[cfg(feature = "tls-transport")]
            Stream::Tls(stream) => stream.selected_path(),
        }
    }

    pub async fn write_frame(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        match &mut self.inner {
            #[cfg(feature = "iroh-transport")]
            Stream::Iroh(stream) => stream.write_frame(bytes).await,
            #[cfg(feature = "tls-transport")]
            Stream::Tls(stream) => stream.write_frame(bytes).await,
        }
    }

    pub async fn read_frame(&mut self) -> anyhow::Result<Vec<u8>> {
        match &mut self.inner {
            #[cfg(feature = "iroh-transport")]
            Stream::Iroh(stream) => stream.read_frame().await,
            #[cfg(feature = "tls-transport")]
            Stream::Tls(stream) => stream.read_frame().await,
        }
    }

    pub fn split_io(&mut self) -> anyhow::Result<(CarrierWriteHalfV1<'_>, CarrierReadHalfV1<'_>)> {
        match &mut self.inner {
            #[cfg(feature = "iroh-transport")]
            Stream::Iroh(stream) => {
                let (write, read) = stream.split_io()?;
                Ok((
                    CarrierWriteHalfV1 {
                        inner: WriteHalf::Iroh(write),
                    },
                    CarrierReadHalfV1 {
                        inner: ReadHalf::Iroh(read),
                    },
                ))
            }
            #[cfg(feature = "tls-transport")]
            Stream::Tls(stream) => {
                let (write, read) = stream.split_io()?;
                Ok((
                    CarrierWriteHalfV1 {
                        inner: WriteHalf::Tls(write),
                    },
                    CarrierReadHalfV1 {
                        inner: ReadHalf::Tls(read),
                    },
                ))
            }
        }
    }

    pub async fn finish(&mut self) -> anyhow::Result<()> {
        match &mut self.inner {
            #[cfg(feature = "iroh-transport")]
            Stream::Iroh(stream) => stream.finish().await,
            #[cfg(feature = "tls-transport")]
            Stream::Tls(stream) => stream.finish().await,
        }
    }
}

impl CarrierWriteHalfV1<'_> {
    pub async fn write_frame(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        match &mut self.inner {
            #[cfg(feature = "iroh-transport")]
            WriteHalf::Iroh(write) => write.write_frame(bytes).await,
            #[cfg(feature = "tls-transport")]
            WriteHalf::Tls(write) => write.write_frame(bytes).await,
        }
    }
}

impl CarrierReadHalfV1<'_> {
    pub async fn read_frame(&mut self) -> anyhow::Result<Vec<u8>> {
        match &mut self.inner {
            #[cfg(feature = "iroh-transport")]
            ReadHalf::Iroh(read) => read.read_frame().await,
            #[cfg(feature = "tls-transport")]
            ReadHalf::Tls(read) => read.read_frame().await,
        }
    }
}

/// Recognizes only the sealed markers issued at actual transport IO failures.
/// It deliberately does not search arbitrary source chains: a local authority
/// or observer failure may contain an older transport error as its source.
pub fn is_carrier_io_failure(error: &anyhow::Error) -> bool {
    #[cfg(feature = "iroh-transport")]
    if error.is::<IrohStreamIoFailureV1>() {
        return true;
    }
    #[cfg(feature = "tls-transport")]
    if error.is::<TlsStreamIoFailureV1>() {
        return true;
    }
    false
}
