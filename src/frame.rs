use deku::prelude::*;

#[derive(Debug, PartialEq, Eq, DekuRead, DekuWrite)]
#[deku(endian = "big")]
pub struct Frame {
    pub header: FrameHeader,

    #[deku(
        count = "header.payload_len",
        map = "|data: Vec<u8>| -> Result<_, DekuError>  { unmask(data, header.masking_key) }"
    )]
    pub payload_data: Vec<u8>,
}

impl Frame {
    pub fn text(text: &str) -> Self {
        Self {
            header: FrameHeader {
                fin: true,
                rsv1: false,
                rsv2: false,
                rsv3: false,
                opcode: Opcode::TextFrame,
                mask: false,
                payload_len: text.as_bytes().len() as u8,
                extended_payload_len_16: None,
                extended_payload_len_64: None,
                masking_key: None,
            },
            payload_data: text.as_bytes().to_vec(),
        }
    }
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
