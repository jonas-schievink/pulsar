use pa_proto::error::{PulseError, Error};
use pa_proto::command::{self, Command, CommandKind, ClientInfo, PROTOCOL_MIN_VERSION};
use pa_proto::packet::{Packet, PacketCodec, Message};
use pa_proto::proplist::{Prop, PropList};
use pa_proto::cookie::AuthCookie;
use pa_proto::paths::cookie_path;
use pa_proto::idxset::{Idx, IdxSet};
use pa_proto;

use tokio;
use tokio::prelude::*;
use tokio_codec::Decoder;
use tokio_uds::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::{fs, io};
use std::ops::{Deref, DerefMut};

// TODO: Limit max. number of connections
#[derive(Debug)]
pub struct Server {
    sock: UnixListener,
    data: Arc<ServerData>,
}

impl Server {
    pub fn new_unix<P: AsRef<Path>>(runtime_dir: P) -> io::Result<Self> {
        let mut socket_file = runtime_dir.as_ref().to_path_buf();
        socket_file.push("native");

        // `socket_file` might already exist. In that case, it's either a left-over from the last
        // server (in which case we can delete the socket) or a server is already running (in which
        // case we bail).
        if socket_file.exists() {
            // If a connection attempt succeeds, there's still someone on the other side.
            match UnixStream::connect(&socket_file).wait() {
                Ok(_) => return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("server socket '{}' already exists and a server is still running", socket_file.display())
                )),
                Err(ref e) if e.kind() == io::ErrorKind::ConnectionRefused => {
                    // This is the expected error when nobody is listening on the socket.
                    // Try to remove the file. We ignore any errors since someone else might've
                    // deleted it while we were checking. If a legit error occurs, the bind below
                    // will fail.
                    if let Err(e) = fs::remove_file(&socket_file) {
                        error!("couldn't remove abandoned socket file '{}': {}", socket_file.display(), e);
                    }
                }
                Err(e) => {
                    // Unexpected error, better bail
                    return Err(e);
                }
            }
        }

        let mut cookie_file = runtime_dir.as_ref().to_path_buf();
        cookie_file.push("cookie");

        Ok(Server {
            sock: UnixListener::bind(socket_file)?,
            data: Arc::new(ServerData::new(AuthCookie::create(cookie_path())?)),
        })
    }

    /// Turn the server instance to a runnable `Future` that will accept clients and process
    /// communication.
    ///
    /// Returns an error if something related to the listening socket goes wrong. This normally
    /// shouldn't happen. In particular, this won't return an error when a client messes up - the
    /// client will simply be disconnected and the error logged.
    pub fn listen(self) -> impl Future<Item=(), Error=io::Error> {
        let data = self.data;
        self.sock.incoming().for_each(move |stream| {
            process(stream, data.clone());
            Ok(())
        })
    }
}

/// Process an incoming connection.
fn process(stream: UnixStream, data: Arc<ServerData>) {
    let (tx, rx) = PacketCodec::new().framed(stream).split();

    let mut handler = ClientHandler::new(data);

    let task = tx.send_all(rx.and_then(move |packet| {
        let reply = handler.handle_packet(&packet)?;
        debug!("reply: {:?}", reply);
        Ok(reply)
    })).then(|result| {
        if let Err(err) = result {
            // FIXME: print which client fucked up
            error!("client handler encountered error: {}", err);
            error!("client will be disconnected");
        }

        Ok(())
    });

    tokio::spawn(task);
}

/// Server data used by server and client handlers (potentially from different threads). Shared via
/// `Arc<RwLock<_>>`.
#[derive(Debug)]
struct ServerData {
    cookie: AuthCookie,
    /// Currently registered clients.
    ///
    /// A client will be added to this list just by opening the control socket, so there might be
    /// bogus clients in here.
    clients: RwLock<IdxSet<Client>>,
    /// Sinks connected to the server.
    ///
    /// Starts out with a dummy sink that ignores all samples, which must never be removed to ensure
    /// that there's always at least a fallback sink to connect to.
    sinks: RwLock<IdxSet<pa_proto::sink::Sink>>,
}

impl ServerData {
    pub fn new(auth_cookie: AuthCookie) -> Self {
        Self {
            cookie: auth_cookie,
            clients: RwLock::new(IdxSet::new()),
            sinks: {
                let mut sinks = IdxSet::new();
                sinks.alloc(|idx| pa_proto::sink::Sink::new_dummy(idx.into()));
                RwLock::new(sinks)
            }
        }
    }
}

impl ServerData {
    fn clients<'a>(&'a self) -> impl Deref<Target=IdxSet<Client>> + 'a {
        self.clients.read().unwrap()
    }

    fn clients_mut<'a>(&'a self) -> impl DerefMut<Target=IdxSet<Client>> + 'a {
        self.clients.write().unwrap()
    }

    fn sinks<'a>(&'a self) -> impl Deref<Target=IdxSet<pa_proto::sink::Sink>> + 'a {
        self.sinks.read().unwrap()
    }

    /*fn sinks_mut<'a>(&'a self) -> impl DerefMut<Target=IdxSet<pa_proto::sink::Sink>> + 'a {
        self.sinks.write().unwrap()
    }*/
}

/// Data associated with every client connected to the server.
#[derive(Debug)]
struct Client {
    id: u32,
    /// Version of the PulseAudio protocol implemented by the client.
    ///
    /// This starts off as `PROTOCOL_MIN_VERSION` and is set to the actual version when the client
    /// informs us. If the client only supports an older version, is it rejected.
    protocol_version: u16,
    /// Whether this client is authenticated to the server.
    authed: bool,
    /// Client properties.
    props: PropList,
}

/// Asynchronous communication processor for a connected client.
#[derive(Debug)]
struct ClientHandler {
    /// Client handle. The managed `Client` structure is stored in the `ServerData` and must not be
    /// removed as long as its `ClientHandler` exists.
    client: Idx<Client>,
    /// Shared handle to the common server state.
    data: Arc<ServerData>,
    /// Buffer for command replies and errors sent to the client.
    reply_buf: Vec<u8>,
}

impl ClientHandler {
    /// Create a new client handler.
    ///
    /// This will create and register a new `Client` with the server automatically.
    fn new(data: Arc<ServerData>) -> Self {
        let client = data.clients_mut().alloc(|idx| {
            info!("new client connected, id {}", idx.value());

            Client {
                id: idx.into(),
                protocol_version: PROTOCOL_MIN_VERSION,
                authed: false,
                props: PropList::new(),
            }
        }).idx();

        Self {
            client,
            data,
            reply_buf: Vec::with_capacity(512),
        }
    }

    /// Process a packet sent to the server and return the response packet to send back to the
    /// client.
    fn handle_packet(&mut self, packet: &Packet) -> Result<Packet, Error> {
        let msg = Message::from_packet(&packet)?;
        debug!("received msg: {:?}", msg);

        match msg {
            Message::Control { tagstruct } => {
                let protocol_version = self.with_client(|c| c.protocol_version);
                let cmd = Command::from_tagstruct(tagstruct, protocol_version)?;

                match self.handle_control(&cmd) {
                    Ok(packet) => {
                        Ok(packet)
                    }
                    Err(e) => {
                        Ok(cmd.error_reply(e)
                            .to_packet(&mut self.reply_buf, protocol_version))
                    }
                }
            }
            _ => unimplemented!(),
        }
    }

    /// Handle a `Command` type message and return the response `Packet` to send back to the client.
    fn handle_control(&mut self, cmd: &Command) -> Result<Packet, PulseError> {
        debug!("handling control command: {:?}", cmd);

        let (authed, protocol_version) = self.with_client(|c| (c.authed, c.protocol_version));

        if cmd.needs_auth() && !authed {
            error!("unauthenticated client tried to execute privileged cmd: {:?}", cmd);
            return Err(PulseError::Access);
        }

        Ok(match cmd.kind() {
            CommandKind::Auth(auth) => {
                let protocol_version = auth.protocol_version();
                if protocol_version < PROTOCOL_MIN_VERSION {
                    error!("client protocol version is {}, minimum supported is {}, rejecting client", protocol_version, PROTOCOL_MIN_VERSION);
                    return Err(PulseError::Version);
                }

                self.with_client_mut(|c| c.protocol_version = protocol_version);

                if self.data.cookie == auth.auth_cookie() {
                    info!("client {} authenticated via cookie", self.client.value());

                    // TODO: memfd and shm negotiation
                    debug!("client support memfd={} shm={}", auth.supports_memfd(), auth.supports_shm());
                    let use_memfd = false;
                    let use_shm = false;

                    self.with_client_mut(|c| c.authed = true);

                    let mut reply = command::AuthReply::new(command::PROTOCOL_VERSION);
                    reply.set_use_memfd(use_memfd);
                    reply.set_use_shm(use_shm);
                    cmd.reply_packet(&mut self.reply_buf, protocol_version, reply)
                } else {
                    error!("auth cookie mismatch");
                    return Err(PulseError::Access);
                }
            },
            CommandKind::SetClientName(params) => {
                self.with_client_mut(|c| c.props.extend(params.props()));
                if let Some(name) = params.props().get_string(Prop::ApplicationName) {
                    info!("client {} is {}", self.client.value(), name);
                }

                cmd.reply_packet(&mut self.reply_buf, protocol_version, command::SetClientNameReply::new(self.client.value()))
            }
            CommandKind::CreatePlaybackStream(_params) => {
                return Err(PulseError::NotImplemented);  // TODO
            }
            CommandKind::GetSinkInfoList => cmd.reply_packet(
                &mut self.reply_buf,
                protocol_version,
                command::GetSinkInfoListReply::new(
                    self.data.sinks().iter()
                )
            ),
            CommandKind::GetSourceInfoList => unimplemented!(),
            CommandKind::GetClientInfoList => cmd.reply_packet(
                &mut self.reply_buf,
                protocol_version,
                command::GetClientInfoListReply::new(
                    self.data.clients().iter()
                        .map(|client| ClientInfo::new(client.id, Default::default(), &client.props))
                ),
            ),
            CommandKind::GetCardInfoList => unimplemented!(),
            CommandKind::GetModuleInfoList => cmd.reply_packet(
                &mut self.reply_buf,
                protocol_version,
                command::GetModuleInfoListReply::new_dummy()
            ),
            CommandKind::GetSinkInputInfoList => unimplemented!(),
            CommandKind::GetSourceOutputInfoList => unimplemented!(),
            CommandKind::GetSampleInfoList => unimplemented!(),
            CommandKind::RegisterMemfdShmid(_) => unimplemented!(),
            CommandKind::Reply { .. } => {
                return Err(PulseError::Protocol);
            }
            CommandKind::Error { .. } => {
                // clients shouldn't send this (right?)
                return Err(PulseError::Protocol);
            }
        })
    }

    // helper functions to make the code more readable
    // could also be implemented with a special guard that derefs to the target type, but this makes
    // the locked area more explicit

    fn with_client<F, R>(&self, f: F) -> R
    where F: FnOnce(&Client) -> R {
        let clients = self.data.clients();
        let client = clients.get(self.client).unwrap();
        f(client)
    }

    /// Obtains write access to the client container and applies a closure to the client managed by
    /// this handler.
    ///
    /// In case the client has been removed from the container, this will panic. This should never
    /// happen as the server is supposed to cancel handler tasks before removing the client.
    fn with_client_mut<F, R>(&mut self, f: F) -> R
    where F: FnOnce(&mut Client) -> R {
        let mut clients = self.data.clients_mut();
        let client = clients.get_mut(self.client).unwrap();
        f(client)
    }
}

impl Drop for ClientHandler {
    fn drop(&mut self) {
        self.data.clients_mut().remove(self.client);
    }
}
