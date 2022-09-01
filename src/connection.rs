use color_eyre::Result;
use futures::{stream::StreamExt, SinkExt};
use tokio::net::TcpStream;
use tokio_util::codec::{Decoder, Framed};

use crate::{frame::Frame, FrameCodec};

pub struct Connection {
    framed: Framed<TcpStream, FrameCodec>,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            framed: FrameCodec::default().framed(stream),
        }
    }

    pub async fn read_frame(&mut self) -> Option<Result<Frame>> {
        self.framed.next().await
    }

    pub async fn write_frame(&mut self, frame: &Frame) -> Result<()> {
        self.framed.send(frame).await?;
        self.framed.flush().await?;

        Ok(())
    }
}
