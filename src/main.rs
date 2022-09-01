use color_eyre::eyre::Result;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{prelude::*, EnvFilter};

use wesley::{message::Message, Connection, Frame};

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
            match wesley::accept(socket, addr).await {
                Ok(conn) => {
                    process(conn).await;
                }
                Err(e) => {
                    tracing::error!("Failed when processing incoming connection: {e:?}");
                }
            }
        });
    }
}

#[tracing::instrument(skip(conn))]
async fn process(mut conn: Connection) {
    while let Some(Ok(frame)) = conn.read_frame().await {
        dbg!(&frame);

        let msg = Message::from_frame(frame).unwrap();
        dbg!(&msg);

        match msg {
            Message::Text(text) => {
                conn.write_frame(&Frame::text(&format!("Hello, {text}!")))
                    .await
                    .unwrap();
            }
            Message::Close(code, reason) => {
                info!("Connection closed ({code}): {reason:?}");
                return;
            }
            _ => {}
        }
    }
}
