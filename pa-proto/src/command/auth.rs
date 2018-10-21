//! Authentication / Handshake command and reply.

use super::prelude::*;

const VERSION_MASK: u32 = 0x0000ffff;
pub const FLAG_SHM: u32 = 0x80000000;
pub const FLAG_MEMFD: u32 = 0x40000000;

/// Establish connection and authenticate client.
#[derive(Debug)]
pub struct Auth<'a> {
    version: u16,
    supports_shm: bool,
    supports_memfd: bool,
    cookie: &'a [u8],
}

impl<'a> Auth<'a> {
    /// Client's protocol version.
    ///
    /// Protocol versions are backwards-compatible, so a client with a higher version than the
    /// server still works. Current PulseAudio no longer supports client versions `< 8`. We
    /// don't support any version `< 13`, so make sure to reject clients/servers accordingly.
    pub fn protocol_version(&self) -> u16 {
        self.version
    }

    /// Whether the client supports POSIX shared memory.
    pub fn supports_shm(&self) -> bool {
        self.supports_shm
    }

    /// Whether the client supports the `memfd` mechanism for shared memory.
    pub fn supports_memfd(&self) -> bool {
        self.supports_memfd
    }

    /// The authentication cookie.
    pub fn auth_cookie(&self) -> &'a [u8] {
        self.cookie
    }
}

impl<'a> FromTagStruct<'a> for Auth<'a> {
    fn from_tag_struct(ts: &mut TagStructReader<'a>, _version: u16) -> Result<Self, Error> {
        let (flags_and_version, cookie) = (ts.read_u32()?, ts.read_arbitrary()?);

        Ok(Self {
            version: (flags_and_version & VERSION_MASK) as u16,
            supports_shm: flags_and_version & FLAG_SHM != 0,
            supports_memfd: flags_and_version & FLAG_MEMFD != 0,
            cookie,
        })
    }
}

impl<'a> ToTagStruct for Auth<'a> {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _version: u16) -> Result<(), Error> {
        let flags_and_version: u32 =
            (self.version as u32 & VERSION_MASK) |
                if self.supports_shm { FLAG_SHM } else { 0 } |
                if self.supports_memfd { FLAG_MEMFD } else { 0 };

        w.write(flags_and_version);
        w.write(self.cookie);
        Ok(())
    }
}

/// Server reply to `Auth` command.
#[derive(Debug)]
pub struct AuthReply {
    version: u16,
    use_memfd: bool,
    use_shm: bool,
    // TODO: What if both are true? Can that ever happen?
}

impl AuthReply {
    pub fn new(server_protocol_version: u16) -> Self {
        Self {
            version: server_protocol_version,
            use_memfd: false,
            use_shm: false,
        }
    }

    /// Gets the server's implemented protocol version.
    pub fn server_protocol_version(&self) -> u16 {
        self.version
    }

    /// Whether Linux' `memfd` will be used to transfer samples.
    pub fn use_memfd(&self) -> bool {
        self.use_memfd
    }

    /// Sets the flag indicating whether Linux' `memfd` mechanism will be used.
    pub fn set_use_memfd(&mut self, use_memfd: bool) {
        self.use_memfd = use_memfd;
    }

    /// Whether POSIX shm will be used to transfer samples.
    pub fn use_shm(&self) -> bool {
        self.use_shm
    }

    /// Sets the flag indicating whether POSIX shared memory will be used.
    pub fn set_use_shm(&mut self, use_shm: bool) {
        self.use_shm = use_shm;
    }
}

impl ToTagStruct for AuthReply {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        // Auth reply is a tagstruct with just a u32 that looks similar to the "version"
        // field in the auth request. It contains the server's protocol version and the
        // result of the shm and memfd negotiation.
        let reply: u32 = self.version as u32 |
            if self.use_memfd { FLAG_MEMFD } else { 0 } |
            if self.use_shm { FLAG_SHM } else { 0 };
        w.write(reply);
        Ok(())
    }
}
