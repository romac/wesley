use color_eyre::eyre::Result;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{prelude::*, EnvFilter};

use wesley::{Connection, Frame, Opcode};

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
    loop {
        if let Some(Ok(frame)) = conn.read_frame().await {
            dbg!(&frame);

            let _ = conn.write_frame(&Frame::text("Hello, sir!")).await;

            if frame.header.opcode == Opcode::ConnectionClose {
                info!("Connection closed");
                return;
            }
        }
    }
}
