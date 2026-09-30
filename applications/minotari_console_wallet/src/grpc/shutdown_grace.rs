// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Serve gRPC until shutdown, then give in-flight requests a bounded grace period.
//!
//! tonic's graceful shutdown waits for every open HTTP/2 stream, and it serves each accepted connection in its own
//! task holding a clone of the service. For the wallet that clone owns `WalletSqlite` handles, each carrying a
//! `ShutdownSignal`, so a client that stops reading a stream would pin the wallet's shutdown drain. Dropping the
//! serve future does not help: it only stops the accept loop, the connection tasks keep running. Instead every accepted
//! TCP stream is wrapped in [`CuttableIo`], and once the grace period expires all of them start failing their I/O,
//! which ends each connection task and drops its service clone.

use std::{
    future::Future,
    io,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use futures::StreamExt;
use log::*;
use tari_shutdown::ShutdownSignal;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpStream,
    sync::watch,
};
use tonic::transport::server::{Connected, Router, TcpConnectInfo, TcpIncoming};

const LOG_TARGET: &str = "wallet::console_wallet::grpc::shutdown_grace";

/// How long to wait for the connection tasks to wind down once their I/O has been cut.
const CUT_CONNECTIONS_TIMEOUT: Duration = Duration::from_secs(1);

/// Binds the gRPC listener with the same socket options `Server::serve_with_shutdown` uses (`Server::builder()`
/// enables `TCP_NODELAY`).
pub fn bind_grpc_listener(address: SocketAddr) -> io::Result<TcpIncoming> {
    Ok(TcpIncoming::bind(address)?.with_nodelay(Some(true)))
}

/// Serve `router` on `listener` until `shutdown` fires. In-flight requests and streams then get `grace` to finish
/// (tonic's graceful shutdown); after that every open connection is cut, so no connection task outlives this call by
/// more than [`CUT_CONNECTIONS_TIMEOUT`].
pub async fn serve_with_shutdown_grace(
    router: Router,
    listener: TcpIncoming,
    shutdown: ShutdownSignal,
    grace: Duration,
) -> Result<(), tonic::transport::Error> {
    let (cut_tx, cut_rx) = watch::channel(false);
    // The raw TCP stream is wrapped before tonic's optional TLS layer, so cutting works for TLS connections too
    let incoming = listener.map(move |conn| conn.map(|stream| CuttableIo::new(stream, cut_rx.clone())));

    let mut grace_signal = shutdown.clone();
    let serve_fut = router.serve_with_incoming_shutdown(incoming, shutdown);
    tokio::pin!(serve_fut);

    tokio::select! {
        result = &mut serve_fut => return result,
        _ = &mut grace_signal => {},
    }
    // Graceful shutdown has started: tonic stopped accepting and waits for the open connections to finish
    if let Ok(result) = tokio::time::timeout(grace, &mut serve_fut).await {
        return result;
    }
    warn!(
        target: LOG_TARGET,
        "gRPC requests still in flight {grace:.0?} after shutdown; closing their connections"
    );
    // Every connection task now fails its next read/write, ends, and drops its service clone. That also completes
    // tonic's graceful shutdown, which is waiting for exactly those tasks.
    let _ = cut_tx.send(true);
    match tokio::time::timeout(CUT_CONNECTIONS_TIMEOUT, &mut serve_fut).await {
        Ok(result) => result,
        Err(_) => {
            warn!(
                target: LOG_TARGET,
                "gRPC connections still open {CUT_CONNECTIONS_TIMEOUT:.0?} after closing them; giving up waiting"
            );
            Ok(())
        },
    }
}

/// A TCP stream whose I/O fails with [`io::ErrorKind::ConnectionAborted`] once the shared cut flag is set.
pub struct CuttableIo {
    inner: TcpStream,
    cut: Pin<Box<dyn Future<Output = ()> + Send>>,
    is_cut: bool,
}

impl CuttableIo {
    fn new(inner: TcpStream, mut cut_rx: watch::Receiver<bool>) -> Self {
        Self {
            inner,
            // Also resolves if the sender is gone, i.e. the server this connection belonged to has returned
            cut: Box::pin(async move {
                let _cut_or_closed = cut_rx.wait_for(|cut| *cut).await;
            }),
            is_cut: false,
        }
    }

    /// Returns the abort error once cut; otherwise registers `cx` to be woken when the cut happens.
    fn poll_cut(&mut self, cx: &mut Context<'_>) -> Option<io::Error> {
        if !self.is_cut && self.cut.as_mut().poll(cx).is_ready() {
            self.is_cut = true;
        }
        self.is_cut.then(|| {
            io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "gRPC connection closed: shutdown grace period expired",
            )
        })
    }
}

impl Connected for CuttableIo {
    type ConnectInfo = TcpConnectInfo;

    fn connect_info(&self) -> Self::ConnectInfo {
        self.inner.connect_info()
    }
}

impl AsyncRead for CuttableIo {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        if let Some(e) = self.poll_cut(cx) {
            return Poll::Ready(Err(e));
        }
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for CuttableIo {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        if let Some(e) = self.poll_cut(cx) {
            return Poll::Ready(Err(e));
        }
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        if let Some(e) = self.poll_cut(cx) {
            return Poll::Ready(Err(e));
        }
        Pin::new(&mut self.inner).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if let Some(e) = self.poll_cut(cx) {
            return Poll::Ready(Err(e));
        }
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if let Some(e) = self.poll_cut(cx) {
            return Poll::Ready(Err(e));
        }
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod test {
    use std::{convert::Infallible, sync::Arc, time::Instant};

    use minotari_app_grpc::tari_rpc::{GetVersionRequest, wallet_client::WalletClient};
    use tari_shutdown::Shutdown;
    use tokio::sync::mpsc;
    use tonic::{
        body::Body,
        codegen::{BoxFuture, Service, http},
        server::NamedService,
        transport::Server,
    };

    use super::*;

    /// Answers no request, ever, while holding `held` (standing in for the wallet handles)
    #[derive(Clone)]
    struct StallingService {
        held: Arc<()>,
        started: mpsc::UnboundedSender<()>,
    }

    impl NamedService for StallingService {
        const NAME: &'static str = "tari.rpc.Wallet";
    }

    impl Service<http::Request<Body>> for StallingService {
        type Error = Infallible;
        type Future = BoxFuture<Self::Response, Infallible>;
        type Response = http::Response<Body>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: http::Request<Body>) -> Self::Future {
            let held = self.held.clone();
            let _ = self.started.send(());
            Box::pin(async move {
                let _held = held;
                futures::future::pending().await
            })
        }
    }

    #[tokio::test]
    async fn stalled_requests_are_cut_after_the_grace_period() {
        const GRACE: Duration = Duration::from_millis(200);
        let held = Arc::new(());
        let (started_tx, mut started_rx) = mpsc::unbounded_channel();
        let service = StallingService {
            held: held.clone(),
            started: started_tx,
        };
        let listener = bind_grpc_listener(([127, 0, 0, 1], 0).into()).unwrap();
        let address = listener.local_addr().unwrap();
        let shutdown = Shutdown::new();
        let server = tokio::spawn(serve_with_shutdown_grace(
            Server::builder().add_service(service),
            listener,
            shutdown.to_signal(),
            GRACE,
        ));

        let mut client = WalletClient::connect(format!("http://{address}")).await.unwrap();
        let request = tokio::spawn(async move { client.get_version(GetVersionRequest {}).await });
        tokio::time::timeout(Duration::from_secs(5), started_rx.recv())
            .await
            .expect("request reached the service")
            .unwrap();

        let start = Instant::now();
        shutdown.trigger();
        tokio::time::timeout(GRACE + CUT_CONNECTIONS_TIMEOUT, server)
            .await
            .expect("server returns once the grace period expires")
            .unwrap()
            .unwrap();
        assert!(start.elapsed() >= GRACE, "stalled request was given the grace period");

        // The client's stalled call fails instead of hanging, and no connection or request task still holds the
        // service's state
        let result = tokio::time::timeout(Duration::from_secs(1), request)
            .await
            .expect("client call ends once its connection is cut")
            .unwrap();
        assert!(result.is_err());
        let deadline = Instant::now() + Duration::from_secs(1);
        while Arc::strong_count(&held) > 1 && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(Arc::strong_count(&held), 1, "service clones released");
    }
}
