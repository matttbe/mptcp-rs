use std::{future::Future, io, net::SocketAddr};

use async_std::net::{TcpListener, TcpStream, ToSocketAddrs};

use crate::{sys, MptcpExt, MptcpOpt, MptcpSocket};

#[async_trait::async_trait(?Send)]
pub trait MptcpStreamExt {
    type Output;

    async fn connect_mptcp_opt<A: ToSocketAddrs>(
        addr: A,
        opt: MptcpOpt,
    ) -> io::Result<MptcpSocket<Self::Output>>;

    async fn connect_mptcp<A: ToSocketAddrs>(addr: A) -> io::Result<MptcpSocket<Self::Output>> {
        Self::connect_mptcp_opt(addr, MptcpOpt::Fallack).await
    }

    async fn connect_mptcp_force<A: ToSocketAddrs>(addr: A) -> io::Result<Self::Output> {
        Ok(Self::connect_mptcp_opt(addr, MptcpOpt::NoFallback)
            .await?
            .into_socket())
    }
}

#[async_trait::async_trait(?Send)]
pub trait MptcpListenerExt {
    type Output;

    async fn bind_mptcp_opt<A: ToSocketAddrs>(
        addr: A,
        opt: MptcpOpt,
    ) -> io::Result<MptcpSocket<Self::Output>>;

    async fn bind_mptcp<A: ToSocketAddrs>(addr: A) -> io::Result<MptcpSocket<Self::Output>> {
        Self::bind_mptcp_opt(addr, MptcpOpt::Fallack).await
    }

    async fn bind_mptcp_force<A: ToSocketAddrs>(addr: A) -> io::Result<Self::Output> {
        Ok(Self::bind_mptcp_opt(addr, MptcpOpt::NoFallback)
            .await?
            .into_socket())
    }
}

async fn resolve_each_addr<A: ToSocketAddrs, F, Fut, T>(addr: &A, mut f: F) -> io::Result<T>
where
    F: FnMut(SocketAddr) -> Fut,
    Fut: Future<Output = io::Result<T>>,
{
    let addrs = addr.to_socket_addrs().await?;
    let mut last_err = None;
    for addr in addrs {
        match f(addr).await {
            Ok(l) => return Ok(l),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "could not resolve to any address",
        )
    }))
}

async fn connect_mptcp(addr: SocketAddr) -> io::Result<TcpStream> {
    let socket = sys::mptcp_socket_for_addr(addr)?;
    socket.set_nonblocking(true)?;
    let r = socket.connect(&addr.into());
    match r.map_err(|e| (e.raw_os_error(), e)) {
        Err((Some(errno), err)) if errno != libc::EINPROGRESS => return Err(err),
        _ => {}
    }
    let socket: std::net::TcpStream = socket.into();
    Ok(socket.into())
}

#[async_trait::async_trait(?Send)]
impl MptcpStreamExt for TcpStream {
    type Output = Self;

    async fn connect_mptcp_opt<A: ToSocketAddrs>(
        addr: A,
        opt: MptcpOpt,
    ) -> io::Result<MptcpSocket<Self::Output>> {
        match resolve_each_addr(&addr, connect_mptcp).await {
            Ok(sock) => Ok(MptcpSocket::Mptcp(sock)),
            Err(_) if matches!(opt, MptcpOpt::Fallack) => {
                Ok(MptcpSocket::Tcp(Self::connect(addr).await?))
            }
            Err(err) => Err(err),
        }
    }
}

impl MptcpExt for TcpStream {}

async fn bind_mptcp(addr: SocketAddr) -> io::Result<TcpListener> {
    let socket = sys::mptcp_socket_for_addr(addr)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(0)?;
    let socket: std::net::TcpListener = socket.into();
    Ok(socket.into())
}

#[async_trait::async_trait(?Send)]
impl MptcpListenerExt for TcpListener {
    type Output = Self;

    async fn bind_mptcp_opt<A: ToSocketAddrs>(
        addr: A,
        opt: MptcpOpt,
    ) -> io::Result<MptcpSocket<Self::Output>> {
        match resolve_each_addr(&addr, bind_mptcp).await {
            Ok(sock) => Ok(MptcpSocket::Mptcp(sock)),
            Err(_) if matches!(opt, MptcpOpt::Fallack) => {
                Ok(MptcpSocket::Tcp(Self::bind(addr).await?))
            }
            Err(err) => Err(err),
        }
    }
}
