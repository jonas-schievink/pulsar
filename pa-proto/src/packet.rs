//! Packet parsing and serialization.
//!
//! As it turns out, the extremely simple format used by `bincode` is perfect for reading and
//! writing simple, fixed structures, so we make use of it here.


/* We piggyback information if audio data blocks are stored in SHM on the seek mode */
/*
#define PA_FLAG_SHMDATA     0x80000000LU
#define PA_FLAG_SHMDATA_MEMFD_BLOCK         0x20000000LU
#define PA_FLAG_SHMRELEASE  0x40000000LU
#define PA_FLAG_SHMREVOKE   0xC0000000LU
#define PA_FLAG_SHMMASK     0xFF000000LU
#define PA_FLAG_SEEKMASK    0x000000FFLU
#define PA_FLAG_SHMWRITABLE 0x00800000LU
*/



// Types of packets/items:
// * "Item packet" - control packet - channel=-1 - payload is tagstruct
// * "SHM release" - set in flags
// * "SHM revoke"  - set in flags
// * "MEMBLOCK" - chunk of audio data - channel=actual channel lol

use types::tagstruct::TagStructReader;
use error::Error;

use bincode;
use tokio_codec::{Encoder, Decoder};
use bytes::{Bytes, BytesMut};
use std::io::prelude::*;
use std::{fmt, mem};

const FLAG_SHMRELEASE: u32 = 0x40000000;    // 0b0100
const FLAG_SHMREVOKE: u32 = 0xC0000000;     // 0b1100 FIXME 2 bits set?

// Not sure why PA doesn't just call this "Header" tbh...
/// Packet descriptor / header.
#[derive(Serialize, Deserialize, Debug, Clone)]
struct Descriptor {
    /// Payload length in Bytes.
    length: u32,
    /// -1 = control packet
    channel: i32,
    offset_hi: u32,
    offset_lo: u32,
    /// SHMRELEASE or SHMREVOKE to mark packet as such, or:
    ///
    /// For memblock packets:
    /// * Lowest byte: Seek mode
    flags: u32,
}

/// A packet transferred via IPC (Unix Socket).
///
/// Packets can be further broken down into 4 kinds of messages.
pub struct Packet {
    /// Packet header / Descriptor.
    desc: Descriptor,
    payload: Bytes,  // TODO: dont serialize length
}

impl Packet {
    /// Creates a packet carrying a command in its payload.
    pub fn new_command(payload: &[u8]) -> Self {
        assert!(payload.len() <= u32::max_value() as usize, "payload larger than 4 GB");

        Packet {
            desc: Descriptor {
                length: payload.len() as u32,
                channel: -1,
                offset_hi: 0,
                offset_lo: 0,
                flags: 0x00,
            },
            payload: payload.into(),
        }
    }

    /// Decodes a `Packet` from a byte slice.
    ///
    /// Also see `PacketCodec` for another way of decoding packets.
    pub fn decode<B: AsRef<[u8]>>(bytes: &B) -> Result<Self, Error> {
        match PacketCodec::new().decode(&mut BytesMut::from(bytes.as_ref())) {
            Ok(None) => Err(Error::string(format!("packet truncated"))),
            Ok(Some(packet)) => Ok(packet),
            Err(e) => Err(e),
        }
    }
}

/// Debug-format the payload as the encoded `Message` instead of raw bytes.
impl fmt::Debug for Packet {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Packet")
            .field("desc", &self.desc)
            .field("payload", &Message::from_packet(self))
            .finish()
    }
}

/// A tokio-compatible decoder and encoder for packets.
#[derive(Debug)]
pub struct PacketCodec {
    /// Caches the parsed packet header.
    desc: Option<Descriptor>,
}

impl PacketCodec {
    /// Creates a new `PacketCodec`.
    pub fn new() -> Self {
        Self {
            desc: None,
        }
    }
}

impl Decoder for PacketCodec {
    type Item = Packet;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<<Self as Decoder>::Item>, <Self as Decoder>::Error> {
        if src.len() < mem::size_of::<Descriptor>() {
            Ok(None)  // not enough data
        } else {
            // at least enough data for the header
            if self.desc.is_none() {
                self.desc = Some(bincode::config().big_endian().deserialize_from(&mut &src[..])?);
            }
            let payload_len = self.desc.as_mut().unwrap().length as usize;

            if src.len() < mem::size_of::<Descriptor>() + payload_len {
                Ok(None)
            } else {
                // enough payload data too
                let mut payload = BytesMut::with_capacity(payload_len);
                payload.resize(payload_len, 0);
                payload.copy_from_slice(&src[mem::size_of::<Descriptor>()..][..payload_len]);

                // remember to reset `self` and clear `src` when yielding an item
                src.clear();
                Ok(Some(Packet {
                    desc: self.desc.take().unwrap(),
                    payload: payload.freeze(),
                }))
            }
        }
    }
}

impl Encoder for PacketCodec {
    type Item = Packet;
    type Error = Error;

    fn encode(&mut self, item: <Self as Encoder>::Item, dst: &mut BytesMut) -> Result<(), <Self as Encoder>::Error> {
        assert_eq!(item.desc.length as usize, item.payload.len());
        dst.resize(mem::size_of::<Descriptor>() + item.desc.length as usize, 0);
        let mut buf = &mut dst[..];
        bincode::config().big_endian().serialize_into(&mut buf, &item.desc)?;
        buf.write_all(&item.payload)?;
        Ok(())
    }
}

/// Any packet contains one of these message types.
#[derive(Debug)]
pub enum Message<'a> {
    /// Control message carrying a tagstruct.
    Control {
        tagstruct: TagStructReader<'a>,
    },
    Memblock {

    },
    ShmRelease {

    },
    ShmRevoke {

    },
}

impl<'a> Message<'a> {
    /// Try to create a message from a raw packet.
    pub fn from_packet(packet: &'a Packet) -> Result<Self, Error> {
        if packet.desc.channel == -1 {
            // Control message containing tagstruct
            Ok(Message::Control {
                tagstruct: TagStructReader::from_raw(&packet.payload),
            })
        } else if packet.desc.flags == FLAG_SHMRELEASE {
            unimplemented!("shmrelease messages");
        } else if packet.desc.flags == FLAG_SHMREVOKE {
            unimplemented!("shmrevoke messages");
        } else {
            unimplemented!("memblock messages");
        }
    }
}
