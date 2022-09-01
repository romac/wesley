use async_trait::async_trait;
use color_eyre::Result;
use tokio::{io::AsyncWriteExt, net::TcpStream};

#[async_trait]
pub trait Body {
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
