//! The `pa-trace` utility hooks itself between a PulseAudio server and client and decodes and dumps
//! their communication in a human-readable format.

extern crate pa_proto;

#[macro_use] extern crate log;
extern crate env_logger;
extern crate tempfile;
extern crate nix;

use pa_proto::paths;
use pa_proto::packet::{Packet, Message};
use pa_proto::command::{Command, CommandKind, PROTOCOL_MIN_VERSION};

use tempfile::Builder;
use nix::sys::socket::{recvmsg, sendmsg, MsgFlags, CmsgSpace, ControlMessage};
use nix::sys::uio::IoVec;
use nix::unistd::close;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::io::{AsRawFd, RawFd};
use std::error::Error;
use std::process;
use std::{env, thread};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

fn main() -> Result<(), Box<Error + Send + Sync>> {
    env_logger::init();

    let real_rt_dir = paths::runtime_dir();
    let faux_rt_dir = Builder::new().prefix("pa-trace").tempdir()?;
    let faux_rt_dir = faux_rt_dir.path();
    debug!("real PULSE_RUNTIME_PATH={}", real_rt_dir.display());
    debug!("faux PULSE_RUNTIME_PATH={}", faux_rt_dir.display());

    let mut args = env::args_os().skip(1);
    let program = if let Some(program) = args.next() {
        program
    } else {
        return Err(format!("missing arguments - please provide the program to be run").into());
    };
    let args: Vec<_> = args.collect();

    info!("connecting to real server");
    let mut sock_path = real_rt_dir.to_path_buf();
    sock_path.push("native");
    let real_stream = Arc::new(UnixStream::connect(sock_path)?);

    // create faux server socket
    let mut sock_path = faux_rt_dir.to_path_buf();
    sock_path.push("native");
    let listener = UnixListener::bind(sock_path)?;

    info!("running client and waiting for connection");
    let mut client = process::Command::new(program)
        .args(&args)
        .env("PULSE_RUNTIME_PATH", faux_rt_dir)
        .spawn()?;

    let faux_stream = Arc::new(listener.accept()?.0);
    info!("forwarding to real server");

    let mut threads = Vec::new();

    let client2server = (faux_stream.clone(), real_stream.clone());
    threads.push(thread::Builder::new().name("client -> server".into()).spawn(move || {
        forward(Direction::ClientToServer, &*client2server.0, &*client2server.1)
    })?);

    let server2client = (faux_stream.clone(), real_stream.clone());
    threads.push(thread::Builder::new().name("client <- server".into()).spawn(move || {
        forward(Direction::ServerToClient, &*server2client.1, &*server2client.0)
    })?);

    for thread in threads {
        thread.join().unwrap()?;    // forward panics and errors as-is
    }

    client.wait()?;

    Ok(())
}

#[derive(Debug)]
enum Direction {
    ClientToServer,
    ServerToClient,
}

impl Direction {
    fn as_str(&self) -> &'static str {
        match *self {
            Direction::ClientToServer => "client -> server",
            Direction::ServerToClient => "client <- server",
        }
    }
}

static MSG_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn msg_counter() -> usize {
    MSG_COUNTER.load(Ordering::SeqCst)
}

fn inc_msg_counter() {
    MSG_COUNTER.fetch_add(1, Ordering::SeqCst);
}

/// Forwards data and ancillary messages from Unix socket `reader` to unix socket `writer`.
fn forward(dir: Direction, reader: &UnixStream, writer: &UnixStream) -> Result<(), Box<Error + Send + Sync>> {
    let dir = dir.as_str();
    let mut buf = vec![0; 1024 * 64];
    let mut cmsgspace = CmsgSpace::<[RawFd; 2]>::new();

    let (rfd, wfd) = (reader.as_raw_fd(), writer.as_raw_fd());

    // for printing, we need to keep track of the protocol version, since it influences cmd contents
    let mut protocol_version = PROTOCOL_MIN_VERSION;
    let mut print_data = |bytes| -> Result<(), pa_proto::Error> {
        let packet = Packet::decode(&bytes)?;
        let msg = Message::from_packet(&packet)?;
        match msg {
            Message::Control { tagstruct } => {
                let cmd = Command::from_tagstruct(tagstruct, protocol_version)?;
                println!("{} [{:03}]: {:?}", dir, msg_counter(), cmd);

                if let CommandKind::Auth(auth) = cmd.kind() {
                    protocol_version = auth.protocol_version();
                    info!("{} protocol version updated to {}", dir, protocol_version);
                }
            }
            _ => unimplemented!(),
        }
        Ok(())
    };

    loop {
        let msg = {
            let iovec = IoVec::from_mut_slice(&mut buf);

            recvmsg(rfd, &[iovec], Some(&mut cmsgspace), MsgFlags::empty())?
        };

        let cmsgs: Vec<_> = msg.cmsgs().collect();
        debug!("{}: {} cmsgs, {} bytes", dir, cmsgs.len(), msg.bytes);
        for cmsg in &cmsgs {
            println!("{} [{:03}]: {}", dir, msg_counter(), debug_cmsg(cmsg));
        }

        let data = &buf[..msg.bytes];
        match print_data(data.to_owned()) {  // weird borrow issue when passing `data` as-is
            Ok(()) => {},
            Err(e) => error!("{}: {}", dir, e),
        }

        let iovec = IoVec::from_slice(data);
        let sent = sendmsg(wfd, &[iovec], &cmsgs, MsgFlags::empty(), None)?;
        assert_eq!(msg.bytes, sent, "couldn't forward entire message");

        for cmsg in &cmsgs {
            match cmsg {
                ControlMessage::ScmRights(fds) => {
                    for fd in *fds {
                        close(*fd).expect("failed to close received fd");
                    }
                }
                _ => {}
            }
        }

        inc_msg_counter();
    }
}

fn debug_cmsg(cmsg: &ControlMessage) -> String {
    match cmsg {
        ControlMessage::ScmRights(rawfds) => format!("ScmRights({:?})", rawfds),
        ControlMessage::ScmTimestamp(time) => format!("ScmTimestamp({:?})", time),
        _ => format!("<unknown>"),
    }
}
