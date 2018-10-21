//! Defines sink data and utilities.

use string::{PaStr, PaString};
use types::{
    PropList, SampleSpec, SampleFormat, ChannelMap, ChannelPosition, CVolume, Volume, FormatInfo,
    FormatEncoding
};
use time::Microseconds;

use std::fmt::Debug;

bitflags! {
    pub struct SinkFlags: u32 {
        /// Supports hardware volume control. This is a dynamic flag and may
        /// change at runtime after the sink has initialized.
        const HW_VOLUME_CTRL = 0x0001;

        /// Supports latency querying.
        const LATENCY = 0x0002;

        /// Is a hardware sink of some kind, in contrast to
        /// "virtual"/software sinks. \since 0.9.3
        const HARDWARE = 0x0004;

        /// Is a networked sink of some kind. \since 0.9.7
        const NETWORK = 0x0008;

        /// Supports hardware mute control. This is a dynamic flag and may
        /// change at runtime after the sink has initialized. \since 0.9.11
        const HW_MUTE_CTRL = 0x0010;

        /// Volume can be translated to dB with pa_sw_volume_to_dB(). This is a
        /// dynamic flag and may change at runtime after the sink has initialized.
        /// \since 0.9.11
        const DECIBEL_VOLUME = 0x0020;

        /// This sink is in flat volume mode, i.e.\ always the maximum of
        /// the volume of all connected inputs. \since 0.9.15
        const FLAT_VOLUME = 0x0040;

        /// The latency can be adjusted dynamically depending on the
        /// needs of the connected streams. \since 0.9.15
        const DYNAMIC_LATENCY = 0x0080;

        /// The sink allows setting what formats are supported by the connected
        /// hardware. The actual functionality to do this might be provided by an
        /// extension. \since 1.0
        const SET_FORMATS = 0x0100;
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum SinkState {
    /// Sink is playing samples: The sink is used by at least one non-paused input.
    Running = 0,
    /// Sink is playing but has no connected inputs that send samples.
    Idle,
    /// Sink is not currently playing and can be closed.
    // FIXME: Is this what pasuspender uses?
    Suspended,
}

/// Specifies the direction of a port.
#[derive(Debug, Copy, Clone)]
pub enum Direction {
    /// The port is an input, ie. part of a source.
    Input,
    /// The port is an output, ie. part of a sink.
    Output,
}

/// Port availability / jack detection status.
/// \since 2.0
// TODO: Clarify if this means "port available for playback/recording"
#[derive(Debug, Copy, Clone)]
pub enum Available {
    /// This port does not support jack detection.
    Unknown = 0,
    /// This port is not available, likely because the jack is not plugged in. \since 2.0
    No = 1,
    /// This port is available, likely because the jack is plugged in. \since 2.0
    Yes = 2,
}

/// A port on a sink, to which a speaker or microphone can be connected.
#[derive(Debug)]
pub struct Port {
    name: PaString,
    desc: PaString,
    props: PropList,
    dir: Direction,
    priority: u32,
    avail: Available,
}

impl Port {
    pub fn new_output(name: PaString, description: PaString, priority: u32) -> Self {
        Port {
            name,
            desc: description,
            props: PropList::new(),
            dir: Direction::Output,
            priority,
            avail: Available::Unknown,
        }
    }

    pub fn new_input(name: PaString, description: PaString, priority: u32) -> Self {
        Port {
            name,
            desc: description,
            props: PropList::new(),
            dir: Direction::Input,
            priority,
            avail: Available::Unknown,
        }
    }

    pub fn name(&self) -> &PaStr { &self.name }
    pub fn description(&self) -> &PaStr { &self.desc }
    pub fn props(&self) -> &PropList { &self.props }
    pub fn direction(&self) -> Direction { self.dir }
    pub fn priority(&self) -> u32 { self.priority }
    pub fn available(&self) -> Available { self.avail }
}

/// A sink connected to a PulseAudio server.
///
/// Every sink can have any number of Sink Inputs, or streams connected to it. If more than one
/// input is connected, the inputs will be mixed together.
///
/// A sink always has a single configured sample spec, and all sink inputs are converted to that
/// format (using resampling to match the sample rates, if necessary).
#[derive(Debug)]
pub struct Sink {
    index: u32,
    name: PaString,
    props: PropList,
    state: SinkState,
    sample_spec: SampleSpec,
    channel_map: ChannelMap,    // make sure channel map length == sample spec channels
    cvolume: CVolume,
    /// Overrides `cvolume`.
    muted: bool,
    flags: SinkFlags,
    ports: Vec<Port>,   // TODO: Vec1
    active_port: usize,
    /// Supported sample formats.
    formats: Vec<FormatInfo>,
    /// The actual sink implementation.
    kind: Box<SinkImpl>,
}

impl Sink {
    /// Creates a dummy sink that will simply drop all samples sent to it.
    ///
    /// The server will create a dummy sink on startup if no other sinks can be found.
    pub fn new_dummy(index: u32) -> Self {
        Self {
            index,
            name: PaString::new("Dummy Sink").unwrap(),
            props: PropList::new(),
            state: SinkState::Idle,
            sample_spec: SampleSpec::new_checked(SampleFormat::Float32Le, 2, 48000).unwrap(),
            channel_map: {
                let mut map = ChannelMap::new();
                map.push(ChannelPosition::FrontLeft).unwrap();
                map.push(ChannelPosition::FrontRight).unwrap();
                map
            },
            cvolume: {
                let mut vol = CVolume::new();
                vol.push(Volume::from_linear(1.0)).unwrap();
                vol.push(Volume::from_linear(1.0)).unwrap();
                vol
            },
            muted: false,
            flags: SinkFlags::empty(),
            ports: vec![
                Port::new_output(PaString::new("Stereo Output").unwrap(), PaString::new("").unwrap(), 0),
            ],
            active_port: 0,
            formats: vec![
                FormatInfo::new(FormatEncoding::Pcm),
            ],
            kind: Box::new(DummySink),
        }
    }

    /// Server-internal sink ID.
    pub fn index(&self) -> u32 { self.index }

    /// The human readable name of the sink.
    ///
    /// This is likely to be something like the device name.
    pub fn name(&self) -> &PaStr { &self.name }

    /// Gets the property list storing the properties associated with this sink.
    pub fn props(&self) -> &PropList { &self.props }

    /// Current sink state (eg. whether the sink is actively playing samples).
    pub fn state(&self) -> SinkState { self.state }

    pub fn sample_spec(&self) -> &SampleSpec { &self.sample_spec }

    pub fn channel_map(&self) -> &ChannelMap { &self.channel_map }

    pub fn cvolume(&self) -> &CVolume { &self.cvolume }

    pub fn muted(&self) -> bool { self.muted }

    pub fn actual_latency(&self) -> Microseconds { Microseconds(0) }   // TODO

    pub fn requested_latency(&self) -> Microseconds { Microseconds(0) } // TODO

    pub fn flags(&self) -> SinkFlags { self.flags }

    pub fn base_volume(&self) -> Volume { Volume::from_linear(1.0) }    // TODO
    pub fn volume_steps(&self) -> u32 { 100 }   // TODO

    /// Get the ports of this sink.
    ///
    /// A sink has at least one port a plug can be plugged into, and only *one* port can be active
    /// at any given time. To obtain the currently active port, call
    /// [`active_port()`](#method.active_port).
    pub fn ports(&self) -> &[Port] { &self.ports }

    /// Get a reference to the currently active port of this sink.
    pub fn active_port(&self) -> &Port {
        &self.ports[self.active_port]
    }

    /// Get the list of supported sample formats.
    ///
    /// Most commonly used sinks of consumer hardware will only have support for a single format,
    /// PCM.
    pub fn formats(&self) -> &[FormatInfo] { &self.formats }
}

pub trait SinkImpl: Debug + Send + Sync {

}

/// A sink that simply drops all samples sent to it. `/dev/null`.
#[derive(Debug)]
pub struct DummySink;

impl SinkImpl for DummySink {

}
