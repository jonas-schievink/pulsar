extern crate pulsar;
extern crate env_logger;

use pulsar::client::Client;

fn main() {
    env_logger::init();

    let mut client = Client::connect_default().unwrap();
    client.test().unwrap();
}
