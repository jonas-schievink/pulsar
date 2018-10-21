use std::net::{SocketAddr, TcpStream};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::fmt::Debug;
use std::io::prelude::*;
use std::io;

/// A bidirectional data stream.
pub trait Stream: Read + Write + Debug {}

impl<RW: Read + Write + Debug> Stream for RW {}

#[derive(Debug)]
pub enum Transport {
    Network(SocketAddr),
    Unix(PathBuf),
}

impl Transport {
    pub fn open(self) -> io::Result<Box<Stream>> {
        Ok(match self {
            Transport::Network(addr) => Box::new(TcpStream::connect(addr)?) as Box<Stream>,
            Transport::Unix(path) => Box::new(UnixStream::connect(path)?) as Box<Stream>,
        })
    }
}
