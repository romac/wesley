#![allow(unused_imports)]

use std::fmt::Arguments;
use std::fmt::Debug;
use std::future::Future;
use std::net::SocketAddr;

use async_trait::async_trait;
use color_eyre::eyre::eyre;
use color_eyre::eyre::Result;
use http::HeaderValue;
use http::Method;
use http::Request;
use http::Response;
use http::Version;
use sha1::Digest;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::debug;
use tracing::info;
use tracing::warn;
use tracing_subscriber::{prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    info!("Accepting TCP connections on 127.0.0.1:8080");

    loop {
        let (socket, addr) = listener.accept().await?;

        tokio::spawn(async move {
            if let Err(e) = handle_connection(socket, addr).await {
                tracing::error!("Failed when processing incoming connection: {e:?}");
            }
        });
    }
}

#[tracing::instrument(skip(stream))]
async fn handle_connection(mut stream: TcpStream, addr: SocketAddr) -> Result<()> {
    info!("Accepted incoming connection from {addr}");

    let mut incoming = vec![];

    loop {
        let mut buf = vec![0u8; 1024];
        let read = stream.read(&mut buf).await?;
        debug!("Read {read} bytes");
        incoming.extend_from_slice(&buf[..read]);

        if read == 0 || incoming.len() > 4 && &incoming[incoming.len() - 4..] == b"\r\n\r\n" {
            break;
        }
    }

    let incoming = std::str::from_utf8(&incoming)?;
    debug!("Got HTTP request:\n{incoming}");

    let request = parse_http_request(incoming)?;
    debug!("Parsed HTTP request:\n{request:#?}");

    if let Err(e) = upgrade_connection(&mut stream, request).await {
        warn!("Invalid WebSocket opening handshake: {e}");

        let response = Response::builder().status(500).body(e.to_string())?;
        return send_response(&mut stream, response).await;
    }

    handle_websocket(&mut stream).await?;

    info!("Closing connection for {addr}");

    Ok(())
}

#[tracing::instrument(skip(stream))]
async fn handle_websocket(stream: &mut TcpStream) -> Result<()> {
    info!("Handling WebSocket stream");

    let mut incoming = vec![];
    loop {
        let mut buf = vec![0u8; 1024];
        let read = stream.read(&mut buf).await?;
        debug!("Read {read} bytes: {:?}", &buf[..read]);
        incoming.extend_from_slice(&buf[..read]);

        // dbg!(&incoming);
        // let ((remaining, offset), frame) = FrameHeader::from_bytes((incoming.as_slice(), 0))?;
        // dbg!(remaining, offset, frame);

        let frame = Frame::try_from(incoming.as_slice())?;
        dbg!(frame);

        incoming.clear();

        if read == 0 {
            break;
        }
    }

    info!("Finished handling WebSocket");

    Ok(())
}

#[tracing::instrument(skip(stream, request))]
async fn upgrade_connection(stream: &mut TcpStream, request: Request<()>) -> Result<()> {
    if !is_websocket_upgrade(&request) {
        return Err(eyre!("Not a valid incoming WebSocket handshake"));
    }

    let nonce = request
        .headers()
        .get("sec-websocket-key")
        .and_then(|key| key.to_str().ok())
        .map(|key| key.trim())
        .ok_or_else(|| eyre!("Missing header 'Sec-WebSocket-Key'"))?;

    let accept = format!("{nonce}258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let mut hasher = sha1::Sha1::new();
    hasher.update(accept.as_bytes());
    let accept = hasher.finalize();
    let accept = base64::encode(accept);

    let response = Response::builder()
        .status(101)
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header("Sec-WebSocket-Accept", accept)
        .body(())?;

    info!("Upgrading connection");
    send_response(stream, response).await?;
    info!("Connection upgraded");

    Ok(())
}

#[async_trait]
trait Body {
    fn body_length(&self) -> usize;

    async fn write_body(&self, stream: &mut TcpStream) -> Result<()>;
}

#[async_trait]
impl Body for () {
    fn body_length(&self) -> usize {
        0
    }

    async fn write_body(&self, _stream: &mut TcpStream) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl Body for &'_ [u8] {
    fn body_length(&self) -> usize {
        self.len()
    }

    async fn write_body(&self, stream: &mut TcpStream) -> Result<()> {
        stream.write_all(self).await?;
        Ok(())
    }
}

#[async_trait]
impl Body for &'_ str {
    fn body_length(&self) -> usize {
        self.as_bytes().len()
    }

    async fn write_body(&self, stream: &mut TcpStream) -> Result<()> {
        self.as_bytes().write_body(stream).await
    }
}

#[async_trait]
impl Body for String {
    fn body_length(&self) -> usize {
        self.as_bytes().len()
    }

    async fn write_body(&self, stream: &mut TcpStream) -> Result<()> {
        self.as_bytes().write_body(stream).await
    }
}

#[tracing::instrument(skip(stream, response))]
async fn send_response<B>(stream: &mut TcpStream, response: Response<B>) -> Result<()>
where
    B: Body + Debug,
{
    use http::Version;

    debug!("Sending response:\n{response:#?}");

    let status = response.status();

    stream
        .write_all(
            format!(
                "HTTP/1.1 {} {}\r\n",
                status.as_str(),
                status.canonical_reason().unwrap_or_default(),
            )
            .as_bytes(),
        )
        .await?;

    for (name, value) in response.headers() {
        stream
            .write_all(format!("{}: {}\r\n", name.as_str(), value.to_str()?).as_bytes())
            .await?;
    }

    stream
        .write_all(format!("Content-Length: {}\r\n", response.body().body_length()).as_bytes())
        .await?;

    stream.write_all(b"\r\n").await?;

    response.body().write_body(stream).await?;

    stream.flush().await?;

    Ok(())
}

#[tracing::instrument(skip(buf))]
fn parse_http_request(buf: &str) -> Result<Request<()>> {
    let mut headers = [httparse::EMPTY_HEADER; 16];
    let mut raw = httparse::Request::new(&mut headers);
    let status = raw.parse(buf.as_bytes())?;
    assert!(status.is_complete());

    let mut req = Request::builder()
        .method(raw.method.unwrap_or("GET"))
        .uri(raw.path.unwrap_or("/"));

    for header in raw.headers {
        req = req.header(header.name, header.value);
    }

    let req = req.body(())?;

    Ok(req)
}

fn is_websocket_upgrade(req: &Request<()>) -> bool {
    req.method() == Method::GET
        && req.version() == Version::HTTP_11
        && req.headers().get("connection") == Some(&HeaderValue::from_static("Upgrade"))
        && req.headers().get("upgrade") == Some(&HeaderValue::from_static("websocket"))
        && req.headers().get("sec-websocket-version") == Some(&HeaderValue::from_static("13"))
}

use deku::prelude::*;

#[derive(Debug, PartialEq, Eq, DekuRead, DekuWrite)]
#[deku(endian = "big")]
pub struct Frame {
    header: FrameHeader,
    #[deku(
        count = "header.payload_len",
        map = "|data: Vec<u8>| -> Result<_, DekuError>  { unmask(data, header.masking_key) }"
    )]
    pub payload_data: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq, DekuRead, DekuWrite)]
#[deku(endian = "endian", ctx = "endian: deku::ctx::Endian")]
pub struct FrameHeader {
    #[deku(bits = 1)]
    pub fin: bool,
    #[deku(bits = 1)]
    pub rsv1: bool,
    #[deku(bits = 1)]
    pub rsv2: bool,
    #[deku(bits = 1)]
    pub rsv3: bool,
    pub opcode: Opcode,
    #[deku(bits = 1)]
    pub mask: bool,
    #[deku(bits = 7)]
    pub payload_len: u8,
    #[deku(cond = "*payload_len == 126")]
    pub extended_payload_len_16: Option<u16>,
    #[deku(cond = "*payload_len == 127")]
    pub extended_payload_len_64: Option<u64>,
    #[deku(cond = "*mask")]
    pub masking_key: Option<u32>,
}

fn unmask(mut data: Vec<u8>, masking_key: Option<u32>) -> Result<Vec<u8>, DekuError> {
    match masking_key.map(|m| m.to_be_bytes()) {
        None => Ok(data),
        Some(masking_key) => {
            for i in 0..data.len() {
                data[i] ^= masking_key[i % 4];
            }
            Ok(data)
        }
    }
}

#[derive(Debug, PartialEq, Eq, DekuRead, DekuWrite)]
#[deku(
    type = "u8",
    bits = 4,
    endian = "endian",
    ctx = "endian: deku::ctx::Endian"
)]
pub enum Opcode {
    #[deku(id = "0x0")]
    ContinuationFrame,
    #[deku(id = "0x1")]
    TextFrame,
    #[deku(id = "0x2")]
    BinaryFrame,
    #[deku(id_pat = "0x3..=0x7")]
    ReservedNonControl,
    #[deku(id = "0x8")]
    ConnectionClose,
    #[deku(id = "0x9")]
    Ping,
    #[deku(id = "0xA")]
    Pong,
    #[deku(id_pat = "0xB..=0xF")]
    ReservedControl,
}

#[cfg(test)]
mod tests {
    use super::Frame;

    #[test]
    fn hello_world() {
        let data = vec![
            129_u8, 138, 201, 37, 227, 110, 161, 64, 143, 2, 166, 82, 140, 28, 165, 65,
        ];

        let value = Frame::try_from(data.as_ref()).unwrap();
        dbg!(&value);

        assert_eq!(b"helloworld".as_slice(), value.payload_data.as_slice());
    }
}

// trait AsyncWriteFmt {
//     fn write_fmt(
//         &'_ mut self,
//         fmt: Arguments<'_>,
//     ) -> Pin<Box<dyn Future<Output = tokio::io::Result<()>>>>
//     where
//         Self: Unpin;
// }

// impl<T> AsyncWriteFmt for T
// where
//     T: AsyncWriteExt + Send,
// {
//     fn write_fmt(
//         &'_ mut self,
//         fmt: Arguments<'_>,
//     ) -> Pin<Box<dyn Future<Output = tokio::io::Result<()>> + '_>>
//     where
//         Self: Unpin,
//     {
//         let mut string = String::new();
//         let res = std::fmt::write(&mut string, fmt)
//             .map(|_| string.into_bytes())
//             .map_err(|_| tokio::io::Error::new(tokio::io::ErrorKind::Other, "formatter error"));

//         if let Ok(bytes) = res {
//             Box::pin(async move {
//                 self.write_all(&bytes).await?;
//                 Ok(())
//             })
//         } else {
//             Box::pin(async { Ok(()) })
//         }
//     }
// }
