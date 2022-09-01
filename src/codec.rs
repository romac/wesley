use bytes::Buf;
use color_eyre::Report;
use deku::{DekuContainerRead, DekuContainerWrite, DekuError};
use tokio_util::codec::{Decoder, Encoder};
use tracing::debug;

use crate::frame::Frame;

#[derive(Debug, Default)]
pub struct FrameCodec {
    read_offset: usize,
}

impl<'a> Encoder<&'a Frame> for FrameCodec {
    type Error = Report;

    fn encode(&mut self, frame: &'a Frame, dst: &mut bytes::BytesMut) -> Result<(), Self::Error> {
        let bytes = frame.to_bytes()?;
        dst.extend_from_slice(bytes.as_slice());

        Ok(())
    }
}

impl Decoder for FrameCodec {
    type Item = Frame;
    type Error = Report;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match Frame::from_bytes((src, self.read_offset)) {
            Ok(((remaining, offset), frame)) => {
                self.read_offset = offset;

                let bytes_read = src.remaining() - remaining.len();
                src.advance(bytes_read);

                Ok(Some(frame))
            }
            Err(DekuError::Incomplete(need_size)) => {
                debug!(
                    "Incomplete frame, need {} more bytes and {} more bits",
                    need_size.byte_size(),
                    need_size.bit_size()
                );

                Ok(None)
            }
            Err(e) => Err(e.into()),
        }
    }
}
