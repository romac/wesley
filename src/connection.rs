use color_eyre::Result;
use futures::{stream::StreamExt, SinkExt};
use tokio::net::TcpStream;
use tokio_util::codec::{Decoder, Framed};

use crate::{frame::Frame, FrameCodec, Message};

pub struct Connection {
    framed: Framed<TcpStream, FrameCodec>,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            framed: FrameCodec::default().framed(stream),
        }
    }

    pub async fn read_message(&mut self) -> Option<Result<Message>> {
        match self.read_frame().await? {
            Err(e) => Some(Err(e)),

            Ok(frame) if frame.header.fin => Some(Message::from_frame(frame)),

            Ok(mut initial) => {
                while let Some(frame) = self.read_frame().await {
                    match frame {
                        Err(e) => return Some(Err(e)),

                        Ok(mut frame) => {
                            // TODO: Ignore control frames

                            initial.payload_data.append(&mut frame.payload_data);

                            if frame.header.fin {
                                break;
                            }
                        }
                    }
                }

                Some(Message::from_frame(initial))
            }
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
