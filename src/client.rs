//! Client-Side interface for connecting to a running server.

use pa_proto::paths::runtime_dir;
use pa_proto::error::Error;
use transport::*;

use std::net::SocketAddr;
use std::path::Path;
use std::io::Write;

/// A PulseAudio client connected to a server.
#[derive(Debug)]
pub struct Client {
    stream: Box<Stream>,
}

impl Client {
    /// Attempts to connect to the user instance.
    pub fn connect_default() -> Result<Self, Error> {
        let mut socket_dir = runtime_dir();
        socket_dir.push("native");

        info!("connecting to socket at {}", socket_dir.display());

        Self::connect_unix(socket_dir)
    }

    pub fn connect_network(addr: SocketAddr) -> Result<Self, Error> {
        // TODO: `ToSocketAddrs` instead?
        Self::connect(Transport::Network(addr))
    }

    pub fn connect_unix<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        Self::connect(Transport::Unix(path.as_ref().to_path_buf()))
    }

    fn connect(transport: Transport) -> Result<Self, Error> {
        Ok(Client {
            stream: transport.open()?,
        })
    }

    pub fn test(&mut self) -> Result<(), Error> {
        let packet_buf = Vec::new();

        // payload = tagstruct with:
        // * command: u32
        // * tag: u32
        // * any params follow

        println!("send: {:X?}", packet_buf);

        self.stream.write_all(&packet_buf)?;
        self.stream.flush()?;
        println!("wrote");

        let mut buf = [0; 1];

        while self.stream.read(&mut buf)? > 0 {
            print!(" {:02X}", buf[0]);
        }
        println!();
        println!("RECVd");

        Ok(())
    }
}
