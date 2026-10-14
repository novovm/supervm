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

#[cfg(feature = "iroh-transport")]
pub struct CarrierMultiplexV1<'scope>(crate::iroh_transport::IrohMultiplexV1<'scope>);

#[cfg(feature = "iroh-transport")]
impl<'scope> CarrierMultiplexV1<'scope> {
    pub async fn open(&self) -> anyhow::Result<CarrierStreamV1<'scope>> {
        Ok(self.0.open().await?.into())
    }
    pub async fn accept(&self) -> anyhow::Result<CarrierStreamV1<'scope>> {
        Ok(self.0.accept().await?.into())
    }
}

impl<'scope> CarrierStreamV1<'scope> {
    #[cfg(feature = "iroh-transport")]
    pub(crate) fn multiplex_v1(&mut self) -> anyhow::Result<CarrierMultiplexV1<'scope>> {
        match &mut self.inner {
            Stream::Iroh(stream) => Ok(CarrierMultiplexV1(stream.multiplex_v1()?)),
            #[cfg(feature = "tls-transport")]
            Stream::Tls(_) => anyhow::bail!("carrier has no multiplex service"),
        }
    }

    #[cfg(feature = "iroh-transport")]
    pub(crate) fn is_multiplexed(&self) -> bool {
        match &self.inner {
            Stream::Iroh(stream) => stream.is_multiplexed(),
            #[cfg(feature = "tls-transport")]
            Stream::Tls(_) => false,
        }
    }

    /// Stable node listeners may accept a new, independently authorized QUIC
    /// stream after the previous one finished. TLS-only streams cannot do this.
    pub async fn next_inbound(self) -> anyhow::Result<Self> {
        match self.inner {
            #[cfg(feature = "iroh-transport")]
            Stream::Iroh(stream) => Ok(stream.next_inbound().await?.into()),
            #[cfg(feature = "tls-transport")]
            Stream::Tls(_) => anyhow::bail!("carrier has no independent inbound streams"),
        }
    }
}

enum Stream<'scope> {
    #[cfg(feature = "iroh-transport")]
    Iroh(Box<IrohStreamV1<'scope>>),
    #[cfg(feature = "tls-transport")]
    Tls(Box<TlsStreamV1<'scope>>),
}

#[cfg(feature = "iroh-transport")]
impl<'scope> From<IrohStreamV1<'scope>> for CarrierStreamV1<'scope> {
    fn from(stream: IrohStreamV1<'scope>) -> Self {
        Self {
            inner: Stream::Iroh(Box::new(stream)),
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
    #[cfg(feature = "iroh-transport")]
    pub(crate) fn admit_continuous_service_v4(&mut self) -> anyhow::Result<()> {
        match &mut self.inner {
            Stream::Iroh(stream) => stream.admit_continuous_service_v4(),
            #[cfg(feature = "tls-transport")]
            Stream::Tls(_) => anyhow::bail!("carrier has no continuous service scope"),
        }
    }

    #[cfg(feature = "iroh-transport")]
    pub(crate) fn begin_service_request_v4(&mut self) -> anyhow::Result<()> {
        match &mut self.inner {
            Stream::Iroh(stream) => stream.begin_service_request_v4(),
            #[cfg(feature = "tls-transport")]
            Stream::Tls(_) => anyhow::bail!("carrier has no continuous service scope"),
        }
    }

    #[cfg(feature = "iroh-transport")]
    pub(crate) fn complete_service_request_v6(&mut self) -> anyhow::Result<()> {
        match &mut self.inner {
            Stream::Iroh(stream) => stream.complete_service_request_v6(),
            #[cfg(feature = "tls-transport")]
            Stream::Tls(_) => anyhow::bail!("carrier has no continuous idle service"),
        }
    }

    #[cfg(feature = "iroh-transport")]
    pub(crate) fn service_request_deadline_v6(&mut self) -> anyhow::Result<std::time::Instant> {
        match &mut self.inner {
            Stream::Iroh(stream) => stream.service_request_deadline_v6(),
            #[cfg(feature = "tls-transport")]
            Stream::Tls(_) => anyhow::bail!("carrier has no continuous idle service"),
        }
    }

    #[cfg(feature = "iroh-transport")]
    pub(crate) async fn read_idle_service_frame_v6(&mut self) -> anyhow::Result<Vec<u8>> {
        match &mut self.inner {
            Stream::Iroh(stream) => stream.read_idle_service_frame_v6().await,
            #[cfg(feature = "tls-transport")]
            Stream::Tls(_) => anyhow::bail!("carrier has no continuous idle service"),
        }
    }

    #[cfg(feature = "iroh-transport")]
    pub(crate) fn admit_service_window(
        &mut self,
        deadline: std::time::Instant,
    ) -> anyhow::Result<()> {
        match &mut self.inner {
            Stream::Iroh(stream) => stream.admit_service_window(deadline),
            #[cfg(feature = "tls-transport")]
            Stream::Tls(_) => anyhow::bail!("carrier has no owned service window"),
        }
    }

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

    #[cfg(feature = "iroh-transport")]
    pub(crate) fn selected_rtt(&self) -> Option<(&'static str, std::time::Duration)> {
        match &self.inner {
            #[cfg(feature = "iroh-transport")]
            Stream::Iroh(stream) => stream.selected_rtt(),
            #[cfg(feature = "tls-transport")]
            Stream::Tls(_) => None,
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
