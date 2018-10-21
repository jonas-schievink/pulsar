//! The `GET_*_INFO` and `GET_*_INFO_LIST` commands.

use super::prelude::*;
use sink::Sink;

use std::u32;
use string::PaString;

#[derive(Debug)]
pub struct GetSinkInfoListReply<'a, S>
where
    S: IntoIterator<Item=&'a Sink> + Clone
{
    pub sinks: S,
    _priv: (),
}

impl<'a, S> GetSinkInfoListReply<'a, S>
where
    S: IntoIterator<Item=&'a Sink> + Clone
{
    pub fn new(sinks: S) -> Self {
        Self {
            sinks,
            _priv: (),
        }
    }
}

impl<'a, S> ToTagStruct for GetSinkInfoListReply<'a, S>
where
    S: IntoIterator<Item=&'a Sink> + Clone
{
    fn to_tag_struct(&self, w: &mut TagStructWriter, protocol_version: u16) -> Result<(), Error> {
        // sink info is simply concatenated onto the tagstruct, with separator or length or anything
        for sink in self.sinks.clone() {
            w.write(sink.index());
            w.write(sink.name());
            // sink description (which may not be a null string)
            w.write(sink.props()
                .get(Prop::DeviceDescription)
                .map(|bytes| PaStr::from_bytes_with_nul(bytes).unwrap())
                .unwrap_or_else(|| PaStr::from_bytes_with_nul(b"(null)\0").unwrap()));
            w.write(sink.sample_spec().protocol_downgrade(protocol_version));
            w.write(sink.channel_map());
            w.write(u32::MAX);  // sink module (we don't have modules)
            w.write(sink.cvolume());
            w.write(sink.muted());
            w.write(u32::MAX);  // sink's monitor source
            w.write(None);  // sink's monitor source name
            w.write(sink.actual_latency());
            w.write(PaString::new("Unknown Driver").unwrap());   // TODO: driver name
            w.write(sink.flags().bits());
            // proto>=13
            w.write(sink.props());
            w.write(sink.requested_latency());
            if protocol_version >= 15 {
                w.write(sink.base_volume());
                w.write(sink.state() as u32);
                w.write(sink.volume_steps());
                w.write(u32::MAX);  // TODO: card index (invalid dummy value)
            }
            if protocol_version >= 16 {
                // send sink port info
                w.write(sink.ports().len() as u32);
                for port in sink.ports() {
                    w.write(port.name());
                    w.write(port.description());
                    w.write(port.priority());
                    if protocol_version >= 24 {
                        w.write(port.available() as u32);
                    }
                }

                // active port name
                w.write(sink.active_port().name());
            }
            if protocol_version >= 21 {
                // send supported sample formats
                w.write(sink.formats().len() as u8);
                for format in sink.formats() {
                    w.write(format);
                }
            }
        }

        Ok(())
    }
}

// FIXME: `pactl list` hangs after receiving this - maybe it expects at least one module?
// (doesn't seem like it - it still hangs)
#[derive(Debug)]
pub struct GetModuleInfoListReply {
    _priv: (),
}

impl GetModuleInfoListReply {
    /// Creates a dummy reply for servers that do not support modules.
    pub fn new_dummy() -> Self {
        Self { _priv: () }
    }
}

impl ToTagStruct for GetModuleInfoListReply {
    fn to_tag_struct(&self, w: &mut TagStructWriter, protocol_version: u16) -> Result<(), Error> {
        w.write(0u32);  // ID
        w.write(PaStr::from_bytes_with_nul(b"Default Module\0").unwrap());
        w.write(<&PaStr>::default());   // "argument"
        w.write(1u32);  // "get_n_used" users of module?

        if protocol_version < 15 {
            w.write(false); // autoload
        } else {
            w.write(PropList::new());   // module props
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct GetClientInfoListReply<I> {
    clients: I,
    _priv: (),
}

impl<'a, I> GetClientInfoListReply<I>
where I: IntoIterator<Item=ClientInfo<'a>> {
    pub fn new(clients: I) -> Self {
        Self {
            clients,
            _priv: (),
        }
    }
}

impl<'a, I> ToTagStruct for GetClientInfoListReply<I>
where I: IntoIterator<Item=ClientInfo<'a>> + Clone {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        for client in self.clients.clone() {
            w.write(client.index);
            w.write(client.app_name);
            w.write(u32::MAX);  // INVALID_INDEX = no/unknown module
            w.write(client.driver);
            w.write(client.props);
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct ClientInfo<'a> {
    index: u32,
    app_name: &'a PaStr,
    driver: &'a PaStr,
    props: &'a PropList,    // proto>=13
}

impl<'a> ClientInfo<'a> {
    pub fn new(index: u32, driver: &'a PaStr, props: &'a PropList) -> Self {
        Self {
            index,
            app_name: props.get_string(Prop::ApplicationName).unwrap_or_default(),
            driver,
            props,
        }
    }
}
