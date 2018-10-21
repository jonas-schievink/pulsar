//! This module contains parsing and serialization code for the different commands that can be sent
//! to a PulseAudio server (along with the corresponding replies to the client).
//!
//! For every command, type safe wrappers are defined that handle the (de)serialization of that
//! command. Differences between PulseAudio protocol versions are handled via a `protocol_version`
//! parameter that many functions accept.
//!
//! `Command` is probably the type of interest for any users of this module. It can (de)serialize
//! arbitrary commands and provides access to the type-safe information within each command via the
//! `CommandKind` enum.

mod auth;
mod create_playback_stream;
mod get_info;
mod register_memfd_shmid;
mod set_client_name;

pub use self::auth::{Auth, AuthReply};
pub use self::create_playback_stream::{CreatePlaybackStream, CreatePlaybackStreamReply};
pub use self::get_info::*;
pub use self::register_memfd_shmid::*;
pub use self::set_client_name::{SetClientName, SetClientNameReply};

use self::PaCommand::*;

use tagstruct::{TagStructReader, TagStructWriter, ToTagStruct, FromTagStruct};
use error::{PulseError, Error};
use packet::Packet;

use num_traits::cast::FromPrimitive;
use std::u32;
use std::fmt::Debug;

/// Prelude module containing various types frequently used in commands or replies.
///
/// Do `use super::prelude::*;` in command files.
pub(crate) mod prelude {
    pub use tagstruct::{TagStructReader, TagStructWriter, ToTagStruct, FromTagStruct};
    pub use proplist::{Prop, PropList};
    pub use sample_spec::SampleSpec;
    pub use channel_map::ChannelMap;
    pub use cvolume::{CVolume, Volume};
    pub use error::Error;
    pub use string::PaStr;
}

/// Minimum protocol version understood by the library.
pub const PROTOCOL_MIN_VERSION: u16 = 13;

/// PulseAudio protocol version implemented by this library.
///
/// This library can still work with clients and servers down to `PROTOCOL_MIN_VERSION` and up to
/// any higher version, but features added by versions higher than this are not supported.
pub const PROTOCOL_VERSION: u16 = 32;

#[allow(bad_style)]
#[repr(u8)]
#[derive(Debug, Copy, Clone, FromPrimitive)]
enum PaCommand {
    /* Generic commands */
    PA_COMMAND_ERROR = 0,
    PA_COMMAND_TIMEOUT = 1, /* pseudo command */
    PA_COMMAND_REPLY,

    /* CLIENT->SERVER */
    PA_COMMAND_CREATE_PLAYBACK_STREAM,        /* Payload changed in v9, v12 (0.9.0, 0.9.8) */
    PA_COMMAND_DELETE_PLAYBACK_STREAM,
    PA_COMMAND_CREATE_RECORD_STREAM,          /* Payload changed in v9, v12 (0.9.0, 0.9.8) */
    PA_COMMAND_DELETE_RECORD_STREAM,
    PA_COMMAND_EXIT,
    PA_COMMAND_AUTH,
    PA_COMMAND_SET_CLIENT_NAME,
    PA_COMMAND_LOOKUP_SINK,
    PA_COMMAND_LOOKUP_SOURCE,
    PA_COMMAND_DRAIN_PLAYBACK_STREAM,
    PA_COMMAND_STAT,
    PA_COMMAND_GET_PLAYBACK_LATENCY,
    PA_COMMAND_CREATE_UPLOAD_STREAM,
    PA_COMMAND_DELETE_UPLOAD_STREAM,
    PA_COMMAND_FINISH_UPLOAD_STREAM,
    PA_COMMAND_PLAY_SAMPLE,
    PA_COMMAND_REMOVE_SAMPLE,

    PA_COMMAND_GET_SERVER_INFO,
    PA_COMMAND_GET_SINK_INFO,
    PA_COMMAND_GET_SINK_INFO_LIST,
    PA_COMMAND_GET_SOURCE_INFO,
    PA_COMMAND_GET_SOURCE_INFO_LIST,
    PA_COMMAND_GET_MODULE_INFO,
    PA_COMMAND_GET_MODULE_INFO_LIST,
    PA_COMMAND_GET_CLIENT_INFO,
    PA_COMMAND_GET_CLIENT_INFO_LIST,
    PA_COMMAND_GET_SINK_INPUT_INFO,          /* Payload changed in v11 (0.9.7) */
    PA_COMMAND_GET_SINK_INPUT_INFO_LIST,     /* Payload changed in v11 (0.9.7) */
    PA_COMMAND_GET_SOURCE_OUTPUT_INFO,
    PA_COMMAND_GET_SOURCE_OUTPUT_INFO_LIST,
    PA_COMMAND_GET_SAMPLE_INFO,
    PA_COMMAND_GET_SAMPLE_INFO_LIST,
    PA_COMMAND_SUBSCRIBE,

    PA_COMMAND_SET_SINK_VOLUME,
    PA_COMMAND_SET_SINK_INPUT_VOLUME,
    PA_COMMAND_SET_SOURCE_VOLUME,

    PA_COMMAND_SET_SINK_MUTE,
    PA_COMMAND_SET_SOURCE_MUTE,

    PA_COMMAND_CORK_PLAYBACK_STREAM,
    PA_COMMAND_FLUSH_PLAYBACK_STREAM,
    PA_COMMAND_TRIGGER_PLAYBACK_STREAM,

    PA_COMMAND_SET_DEFAULT_SINK,
    PA_COMMAND_SET_DEFAULT_SOURCE,

    PA_COMMAND_SET_PLAYBACK_STREAM_NAME,
    PA_COMMAND_SET_RECORD_STREAM_NAME,

    PA_COMMAND_KILL_CLIENT,
    PA_COMMAND_KILL_SINK_INPUT,
    PA_COMMAND_KILL_SOURCE_OUTPUT,

    PA_COMMAND_LOAD_MODULE,
    PA_COMMAND_UNLOAD_MODULE,

    /* Obsolete */
    PA_COMMAND_ADD_AUTOLOAD___OBSOLETE,
    PA_COMMAND_REMOVE_AUTOLOAD___OBSOLETE,
    PA_COMMAND_GET_AUTOLOAD_INFO___OBSOLETE,
    PA_COMMAND_GET_AUTOLOAD_INFO_LIST___OBSOLETE,

    PA_COMMAND_GET_RECORD_LATENCY,
    PA_COMMAND_CORK_RECORD_STREAM,
    PA_COMMAND_FLUSH_RECORD_STREAM,
    PA_COMMAND_PREBUF_PLAYBACK_STREAM,

    /* SERVER->CLIENT */
    PA_COMMAND_REQUEST,
    PA_COMMAND_OVERFLOW,
    PA_COMMAND_UNDERFLOW,
    PA_COMMAND_PLAYBACK_STREAM_KILLED,
    PA_COMMAND_RECORD_STREAM_KILLED,
    PA_COMMAND_SUBSCRIBE_EVENT,

    /* A few more client->server commands */

    /* Supported since protocol v10 (0.9.5) */
    PA_COMMAND_MOVE_SINK_INPUT,
    PA_COMMAND_MOVE_SOURCE_OUTPUT,

    /* Supported since protocol v11 (0.9.7) */
    PA_COMMAND_SET_SINK_INPUT_MUTE,

    PA_COMMAND_SUSPEND_SINK,
    PA_COMMAND_SUSPEND_SOURCE,

    /* Supported since protocol v12 (0.9.8) */
    PA_COMMAND_SET_PLAYBACK_STREAM_BUFFER_ATTR,
    PA_COMMAND_SET_RECORD_STREAM_BUFFER_ATTR,

    PA_COMMAND_UPDATE_PLAYBACK_STREAM_SAMPLE_RATE,
    PA_COMMAND_UPDATE_RECORD_STREAM_SAMPLE_RATE,

    /* SERVER->CLIENT */
    PA_COMMAND_PLAYBACK_STREAM_SUSPENDED,
    PA_COMMAND_RECORD_STREAM_SUSPENDED,
    PA_COMMAND_PLAYBACK_STREAM_MOVED,
    PA_COMMAND_RECORD_STREAM_MOVED,

    /* Supported since protocol v13 (0.9.11) */
    PA_COMMAND_UPDATE_RECORD_STREAM_PROPLIST,
    PA_COMMAND_UPDATE_PLAYBACK_STREAM_PROPLIST,
    PA_COMMAND_UPDATE_CLIENT_PROPLIST,
    PA_COMMAND_REMOVE_RECORD_STREAM_PROPLIST,
    PA_COMMAND_REMOVE_PLAYBACK_STREAM_PROPLIST,
    PA_COMMAND_REMOVE_CLIENT_PROPLIST,

    /* SERVER->CLIENT */
    PA_COMMAND_STARTED,

    /* Supported since protocol v14 (0.9.12) */
    PA_COMMAND_EXTENSION,

    /* Supported since protocol v15 (0.9.15) */
    PA_COMMAND_GET_CARD_INFO,
    PA_COMMAND_GET_CARD_INFO_LIST,
    PA_COMMAND_SET_CARD_PROFILE,

    PA_COMMAND_CLIENT_EVENT,
    PA_COMMAND_PLAYBACK_STREAM_EVENT,
    PA_COMMAND_RECORD_STREAM_EVENT,

    /* SERVER->CLIENT */
    PA_COMMAND_PLAYBACK_BUFFER_ATTR_CHANGED,
    PA_COMMAND_RECORD_BUFFER_ATTR_CHANGED,

    /* Supported since protocol v16 (0.9.16) */
    PA_COMMAND_SET_SINK_PORT,
    PA_COMMAND_SET_SOURCE_PORT,

    /* Supported since protocol v22 (1.0) */
    PA_COMMAND_SET_SOURCE_OUTPUT_VOLUME,
    PA_COMMAND_SET_SOURCE_OUTPUT_MUTE,

    /* Supported since protocol v27 (3.0) */
    PA_COMMAND_SET_PORT_LATENCY_OFFSET,

    /* Supported since protocol v30 (6.0) */
    /* BOTH DIRECTIONS */
    PA_COMMAND_ENABLE_SRBCHANNEL,
    PA_COMMAND_DISABLE_SRBCHANNEL,

    /* Supported since protocol v31 (9.0)
     * BOTH DIRECTIONS */
    PA_COMMAND_REGISTER_MEMFD_SHMID,

    //PA_COMMAND_MAX,
}

#[derive(Debug)]
pub enum CommandKind<'a> {
    /// Authentication request (and protocol handshake).
    Auth(Auth<'a>),

    /// Updates client properties (not just the name).
    SetClientName(SetClientName),

    /// Create a new playback stream.
    CreatePlaybackStream(CreatePlaybackStream<'a>),

    // TODO: Payload for forwards-compatibility
    GetSinkInfoList,
    GetSourceInfoList,
    GetClientInfoList,
    GetCardInfoList,
    GetModuleInfoList,
    GetSinkInputInfoList,
    GetSourceOutputInfoList,
    GetSampleInfoList,

    /// Register `memfd`-based shared memory.
    ///
    /// This command can be sent from client to server and from server to
    /// client. It can only be sent over a Unix domain socket and *must* be
    /// accompanied by the `memfd` file descriptor to share (see [`unix(7)`]
    /// and the `SCM_RIGHTS` ancillary message).
    ///
    /// [`unix(7)`]: https://linux.die.net/man/7/unix
    // TODO: Better docs
    RegisterMemfdShmid(RegisterMemfdShmid),

    /// Reply from server to client.
    Reply {
        params: TagStructReader<'a>,
    },

    /// Command error, sent to client when a command fails.
    Error {
        code: PulseError,
    },
}

/// A command from client to server (or a reply/error sent to the client).
///
/// A command is transmitted as a `TagStruct` in the payload of a control message.
#[derive(Debug)]
pub struct Command<'a> {
    tag: u32,
    kind: CommandKind<'a>,
}

impl<'a> Command<'a> {
    /// Parses a command encoded as a tagstruct.
    ///
    /// # Parameters
    ///
    /// * `ts`: The tagstruct to parse the command from.
    /// * `protocol_version`: PulseAudio protocol version used by the sender of the Command. If the
    ///   version isn't yet known, you can set this to `PROTOCOL_MIN_VERSION`.
    pub fn from_tagstruct(mut ts: TagStructReader<'a>, protocol_version: u16) -> Result<Self, Error> {
        if protocol_version < PROTOCOL_MIN_VERSION {
            return Err(Error::string(format!("protocol version {} unsupported (minimum supported version is {})", protocol_version, PROTOCOL_MIN_VERSION)));
        }

        let (command, tag) = (ts.read_u32()?, ts.read_u32()?);

        let command = PaCommand::from_u32(command)
            .ok_or_else(|| Error::string(format!("invalid command opcode {}", command)))?;
        let kind = match command {
            /* SERVER->CLIENT */
            /*PA_COMMAND_ERROR |
            PA_COMMAND_TIMEOUT |*/
            PA_COMMAND_REPLY => {
                let content = ts;
                ts = TagStructReader::from_raw(&[]);
                CommandKind::Reply { params: content }
            }

            /* CLIENT->SERVER */
            PA_COMMAND_CREATE_PLAYBACK_STREAM => {
                CommandKind::CreatePlaybackStream(CreatePlaybackStream::from_tag_struct(&mut ts, protocol_version)?)
            }
            /*
            PA_COMMAND_DELETE_PLAYBACK_STREAM |
            PA_COMMAND_CREATE_RECORD_STREAM |          /* Payload changed in v9, v12 (0.9.0, 0.9.8) */
            PA_COMMAND_DELETE_RECORD_STREAM |
            PA_COMMAND_EXIT |*/
            PA_COMMAND_AUTH => {
                CommandKind::Auth(Auth::from_tag_struct(&mut ts, protocol_version)?)
            }
            PA_COMMAND_SET_CLIENT_NAME => {
                CommandKind::SetClientName(SetClientName::from_tag_struct(&mut ts, protocol_version)?)
            }
            /*PA_COMMAND_LOOKUP_SINK |
            PA_COMMAND_LOOKUP_SOURCE |
            PA_COMMAND_DRAIN_PLAYBACK_STREAM |
            PA_COMMAND_STAT |
            PA_COMMAND_GET_PLAYBACK_LATENCY |
            PA_COMMAND_CREATE_UPLOAD_STREAM |
            PA_COMMAND_DELETE_UPLOAD_STREAM |
            PA_COMMAND_FINISH_UPLOAD_STREAM |
            PA_COMMAND_PLAY_SAMPLE |
            PA_COMMAND_REMOVE_SAMPLE |

            PA_COMMAND_GET_SERVER_INFO |*/
            PA_COMMAND_GET_SINK_INFO => unimplemented!(),
            PA_COMMAND_GET_SINK_INFO_LIST => CommandKind::GetSinkInfoList,
            PA_COMMAND_GET_SOURCE_INFO => unimplemented!(),
            PA_COMMAND_GET_SOURCE_INFO_LIST => CommandKind::GetSourceInfoList,
            PA_COMMAND_GET_MODULE_INFO => unimplemented!(),
            PA_COMMAND_GET_MODULE_INFO_LIST => CommandKind::GetModuleInfoList,
            PA_COMMAND_GET_CLIENT_INFO => unimplemented!(),
            PA_COMMAND_GET_CLIENT_INFO_LIST => CommandKind::GetClientInfoList,
            PA_COMMAND_GET_SINK_INPUT_INFO => unimplemented!(),
            PA_COMMAND_GET_SINK_INPUT_INFO_LIST => CommandKind::GetSinkInputInfoList,
            PA_COMMAND_GET_SOURCE_OUTPUT_INFO => unimplemented!(),
            PA_COMMAND_GET_SOURCE_OUTPUT_INFO_LIST => CommandKind::GetSourceOutputInfoList,
            PA_COMMAND_GET_SAMPLE_INFO => unimplemented!(),
            PA_COMMAND_GET_SAMPLE_INFO_LIST => CommandKind::GetSampleInfoList,
            /*PA_COMMAND_SUBSCRIBE |

            PA_COMMAND_SET_SINK_VOLUME |
            PA_COMMAND_SET_SINK_INPUT_VOLUME |
            PA_COMMAND_SET_SOURCE_VOLUME |

            PA_COMMAND_SET_SINK_MUTE |
            PA_COMMAND_SET_SOURCE_MUTE |

            PA_COMMAND_CORK_PLAYBACK_STREAM |
            PA_COMMAND_FLUSH_PLAYBACK_STREAM |
            PA_COMMAND_TRIGGER_PLAYBACK_STREAM |

            PA_COMMAND_SET_DEFAULT_SINK |
            PA_COMMAND_SET_DEFAULT_SOURCE |

            PA_COMMAND_SET_PLAYBACK_STREAM_NAME |
            PA_COMMAND_SET_RECORD_STREAM_NAME |

            PA_COMMAND_KILL_CLIENT |
            PA_COMMAND_KILL_SINK_INPUT |
            PA_COMMAND_KILL_SOURCE_OUTPUT |

            PA_COMMAND_LOAD_MODULE |
            PA_COMMAND_UNLOAD_MODULE |

            /* Obsolete */
            PA_COMMAND_ADD_AUTOLOAD___OBSOLETE |
            PA_COMMAND_REMOVE_AUTOLOAD___OBSOLETE |
            PA_COMMAND_GET_AUTOLOAD_INFO___OBSOLETE |
            PA_COMMAND_GET_AUTOLOAD_INFO_LIST___OBSOLETE |

            PA_COMMAND_GET_RECORD_LATENCY |
            PA_COMMAND_CORK_RECORD_STREAM |
            PA_COMMAND_FLUSH_RECORD_STREAM |
            PA_COMMAND_PREBUF_PLAYBACK_STREAM |

            /* SERVER->CLIENT */
            PA_COMMAND_REQUEST |
            PA_COMMAND_OVERFLOW |
            PA_COMMAND_UNDERFLOW |
            PA_COMMAND_PLAYBACK_STREAM_KILLED |
            PA_COMMAND_RECORD_STREAM_KILLED |
            PA_COMMAND_SUBSCRIBE_EVENT |

            /* A few more client->server commands */

            /* Supported since protocol v10 (0.9.5) */
            PA_COMMAND_MOVE_SINK_INPUT |
            PA_COMMAND_MOVE_SOURCE_OUTPUT |

            /* Supported since protocol v11 (0.9.7) */
            PA_COMMAND_SET_SINK_INPUT_MUTE |

            PA_COMMAND_SUSPEND_SINK |
            PA_COMMAND_SUSPEND_SOURCE |

            /* Supported since protocol v12 (0.9.8) */
            PA_COMMAND_SET_PLAYBACK_STREAM_BUFFER_ATTR |
            PA_COMMAND_SET_RECORD_STREAM_BUFFER_ATTR |

            PA_COMMAND_UPDATE_PLAYBACK_STREAM_SAMPLE_RATE |
            PA_COMMAND_UPDATE_RECORD_STREAM_SAMPLE_RATE |

            /* SERVER->CLIENT */
            PA_COMMAND_PLAYBACK_STREAM_SUSPENDED |
            PA_COMMAND_RECORD_STREAM_SUSPENDED |
            PA_COMMAND_PLAYBACK_STREAM_MOVED |
            PA_COMMAND_RECORD_STREAM_MOVED |

            /* Supported since protocol v13 (0.9.11) */
            PA_COMMAND_UPDATE_RECORD_STREAM_PROPLIST |
            PA_COMMAND_UPDATE_PLAYBACK_STREAM_PROPLIST |
            PA_COMMAND_UPDATE_CLIENT_PROPLIST |
            PA_COMMAND_REMOVE_RECORD_STREAM_PROPLIST |
            PA_COMMAND_REMOVE_PLAYBACK_STREAM_PROPLIST |
            PA_COMMAND_REMOVE_CLIENT_PROPLIST |

            /* SERVER->CLIENT */
            PA_COMMAND_STARTED |

            /* Supported since protocol v14 (0.9.12) */
            PA_COMMAND_EXTENSION |

            /* Supported since protocol v15 (0.9.15) */
            PA_COMMAND_GET_CARD_INFO |
            PA_COMMAND_GET_CARD_INFO_LIST |
            PA_COMMAND_SET_CARD_PROFILE |

            PA_COMMAND_CLIENT_EVENT |
            PA_COMMAND_PLAYBACK_STREAM_EVENT |
            PA_COMMAND_RECORD_STREAM_EVENT |

            /* SERVER->CLIENT */
            PA_COMMAND_PLAYBACK_BUFFER_ATTR_CHANGED |
            PA_COMMAND_RECORD_BUFFER_ATTR_CHANGED |

            /* Supported since protocol v16 (0.9.16) */
            PA_COMMAND_SET_SINK_PORT |
            PA_COMMAND_SET_SOURCE_PORT |

            /* Supported since protocol v22 (1.0) */
            PA_COMMAND_SET_SOURCE_OUTPUT_VOLUME |
            PA_COMMAND_SET_SOURCE_OUTPUT_MUTE |

            /* Supported since protocol v27 (3.0) */
            PA_COMMAND_SET_PORT_LATENCY_OFFSET |*/

            /* Supported since protocol v30 (6.0) */
            /* BOTH DIRECTIONS */
            //PA_COMMAND_ENABLE_SRBCHANNEL
            //PA_COMMAND_DISABLE_SRBCHANNEL

            /* Supported since protocol v31 (9.0)
             * BOTH DIRECTIONS */
            PA_COMMAND_REGISTER_MEMFD_SHMID => {
                CommandKind::RegisterMemfdShmid(RegisterMemfdShmid::from_tag_struct(&mut ts, protocol_version)?)
            }
            _ => unimplemented!("command {:?}", command),
        };

        // ensure that all parameters are consumed
        if let Some(val) = ts.read()? {
            return Err(Error::string(format!("extra command parameter: {:?}", val)));
        }

        Ok(Command { tag, kind })
    }

    /// Serialize this command as a control packet that can be sent over the wire.
    ///
    /// # Parameters
    ///
    /// * `payload_buf`: Reusable payload buffer to use for serialization.
    /// * `protocol_version`: Negotiated protocol version to use for the serialization format.
    pub fn to_packet(&self, payload_buf: &mut Vec<u8>, protocol_version: u16) -> Packet {
        self.to_tag_struct(&mut TagStructWriter::new(payload_buf), protocol_version)
            .expect("serializing into tagstruct failed");
        Packet::new_command(payload_buf)
    }

    pub fn kind(&self) -> &CommandKind { &self.kind }

    /// Creates a reply command containing a tagstruct.
    pub fn reply_packet<'c, T>(&self, buffer: &'c mut Vec<u8>, protocol_version: u16, reply: T) -> Packet
    where
        T: ToTagStruct + Debug {

        debug!("reply: {:?}", reply);

        {
            // Skip the `Command` middleman and serialize the reply directly into the buffer:
            let mut w = TagStructWriter::new(buffer);
            w.write(PA_COMMAND_REPLY as u32);
            w.write(self.tag);
            reply.to_tag_struct(&mut w, protocol_version).expect("serializing into tagstruct failed");
            debug!("reply tagstruct: {:?}", w);
        }
        Packet::new_command(buffer)
    }

    /// Creates an empty reply packet.
    pub fn empty_reply_packet<'c>(&self, buffer: &'c mut Vec<u8>) -> Packet {
        {
            // Skip the `Command` middleman and serialize the reply directly into the buffer:
            let mut w = TagStructWriter::new(buffer);
            w.write(PA_COMMAND_REPLY as u32);
            w.write(self.tag);
            debug!("reply tagstruct: {:?}", w);
        }
        Packet::new_command(buffer)
    }

    /// Creates an error reply.
    pub fn error_reply(&self, code: PulseError) -> Command<'static> {
        Command {
            tag: self.tag,
            kind: CommandKind::Error { code },
        }
    }

    /// Whether the command requires an authenticated client to be executed.
    ///
    /// This is an extremely conservative implementation (every command needs auth, except
    /// `CommandKind::Auth` itself).
    pub fn needs_auth(&self) -> bool {
        match self.kind() {
            CommandKind::Auth { .. } => false,
            _ => true,
        }
    }
}

impl<'a> ToTagStruct for Command<'a> {
    fn to_tag_struct(&self, w: &mut TagStructWriter, protocol_version: u16) -> Result<(), Error> {
        use self::CommandKind::*;

        match self.kind {
            Auth(ref auth) => {
                w.write(PA_COMMAND_AUTH as u32);
                w.write(self.tag);
                auth.to_tag_struct(w, protocol_version)?;
            }
            SetClientName(ref params) => {
                w.write(PA_COMMAND_SET_CLIENT_NAME as u32);
                w.write(self.tag);
                params.to_tag_struct(w, protocol_version)?;
            }
            CreatePlaybackStream(ref params) => {
                w.write(PA_COMMAND_CREATE_PLAYBACK_STREAM as u32);
                w.write(self.tag);
                params.to_tag_struct(w, protocol_version)?;
            }
            GetSinkInfoList => {
                w.write(PA_COMMAND_GET_SINK_INFO_LIST as u32);
                w.write(self.tag);
            }
            GetSourceInfoList => {
                w.write(PA_COMMAND_GET_SOURCE_INFO_LIST as u32);
                w.write(self.tag);
            }
            GetClientInfoList => {
                w.write(PA_COMMAND_GET_CLIENT_INFO_LIST as u32);
                w.write(self.tag);
            }
            GetCardInfoList => {
                w.write(PA_COMMAND_GET_CARD_INFO_LIST as u32);
                w.write(self.tag);
            }
            GetModuleInfoList => {
                w.write(PA_COMMAND_GET_MODULE_INFO_LIST as u32);
                w.write(self.tag);
            }
            GetSinkInputInfoList => {
                w.write(PA_COMMAND_GET_SINK_INPUT_INFO_LIST as u32);
                w.write(self.tag);
            }
            GetSourceOutputInfoList => {
                w.write(PA_COMMAND_GET_SOURCE_OUTPUT_INFO_LIST as u32);
                w.write(self.tag);
            }
            GetSampleInfoList => {
                w.write(PA_COMMAND_GET_SAMPLE_INFO_LIST as u32);
                w.write(self.tag);
            }
            RegisterMemfdShmid(ref params) => {
                // XXX This needs to pass the memfd too!
                w.write(PA_COMMAND_REGISTER_MEMFD_SHMID as u32);
                w.write(self.tag);
                params.to_tag_struct(w, protocol_version)?;
            }

            Reply { ref params } => {
                w.write(PA_COMMAND_REPLY as u32);
                w.write(self.tag);
                w.extend(params.checked_iter()?);
            }
            Error { code } => {
                w.write(PA_COMMAND_ERROR as u32);
                w.write(self.tag);
                w.write(code as u32);
            }
        }

        Ok(())
    }
}
