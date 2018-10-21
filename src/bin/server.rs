extern crate pulsar;

#[macro_use] extern crate log;
extern crate env_logger;
extern crate tokio;

use pulsar::server::Server;
use tokio::prelude::*;
use tokio::executor::thread_pool;

use std::path::Path;
use std::process::{Command, Stdio, exit};

fn main() {
    env_logger::init();
    let mut tpb = thread_pool::Builder::new();
    tpb.pool_size(1);
    let mut runtime = tokio::runtime::Builder::new()
        .threadpool_builder(tpb)
        .build().unwrap();

    // (dir must exist)
    let rt_dir = Path::new("target").canonicalize().unwrap();
    info!("using PULSE_RUNTIME_PATH={}", rt_dir.display());
    let server = match Server::new_unix(rt_dir.as_path()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("could not start server: {}", e);
            exit(1);
        }
    };

    let mut pacat = Command::new("pacat")
        .arg("-v")
        .env("PULSE_RUNTIME_PATH", rt_dir.into_os_string())
        .stdin(Stdio::piped())
        .spawn().unwrap();
    info!("pacat spawned as {}", pacat.id());

    if let Ok(Some(status)) = pacat.try_wait() {
        panic!("pacat unexpectedly exited: {}", status);
    }

    runtime.block_on(server.listen().map_err(|err| {
        eprintln!("server encountered error: {}", err);
        exit(1);
    })).unwrap();
    //runtime.run().unwrap();
}
