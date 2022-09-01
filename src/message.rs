use color_eyre::Result;

use crate::{Frame, Opcode};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close(u16, Option<String>),
    Frame(Frame),
}

impl Message {
    pub fn from_frame(frame: Frame) -> Result<Self> {
        match frame.header.opcode {
            Opcode::TextFrame => String::from_utf8(frame.payload_data)
                .map(Self::Text)
                .map_err(Into::into),
            Opcode::BinaryFrame => Ok(Self::Binary(frame.payload_data)),
            Opcode::Ping => Ok(Self::Ping(frame.payload_data)),
            Opcode::Pong => Ok(Self::Pong(frame.payload_data)),
            Opcode::ConnectionClose => {
                let mut body = frame.payload_data;
                if body.len() < 2 {
                    Ok(Self::Close(1000, None))
                } else {
                    let code = u16::from_be_bytes(body[0..2].try_into()?);
                    let reason = String::from_utf8(body.split_off(2))?;
                    Ok(Self::Close(code, Some(reason)))
                }
            }
            Opcode::ReservedNonControl => Ok(Self::Frame(frame)),
            Opcode::ReservedControl => Ok(Self::Frame(frame)),
            Opcode::ContinuationFrame => Ok(Self::Frame(frame)),
        }
    }
}
