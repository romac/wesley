use std::fmt::Debug;
use std::net::SocketAddr;

use color_eyre::eyre::{eyre, Result};
use http::{HeaderValue, Method, Request, Response, Version};
use sha1::Digest;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, info, warn};

pub mod body;
pub mod codec;
pub mod connection;
pub mod frame;
pub mod message;

pub use body::Body;
pub use codec::FrameCodec;
pub use connection::Connection;
pub use frame::{Frame, FrameHeader, Opcode};
pub use message::Message;

#[tracing::instrument(skip(stream))]
pub async fn accept(mut stream: TcpStream, addr: SocketAddr) -> Result<Connection> {
    info!("Accepted incoming connection from {addr}");

    let incoming = read_http_request(&mut stream).await?;
    debug!("Got HTTP request:\n{incoming}");

    let request = parse_http_request(&incoming)?;
    debug!("Parsed HTTP request:\n{request:#?}");

    if let Err(e) = upgrade_connection(&mut stream, request).await {
        warn!("Invalid WebSocket opening handshake: {e}");

        let response = Response::builder().status(500).body(e.to_string())?;
        send_response(&mut stream, response).await?;
        return Err(eyre!("Not a WebSocket connection"));
    }

    Ok(Connection::new(stream))
}

async fn read_http_request(stream: &mut TcpStream) -> Result<String> {
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

    let text = String::from_utf8(incoming)?;
    Ok(text)
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

#[tracing::instrument(skip(stream, response))]
async fn send_response<B>(stream: &mut TcpStream, response: Response<B>) -> Result<()>
where
    B: Body + Debug,
{
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
