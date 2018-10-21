//! Defines mappings from stream channels to speaker positions.

use types::sample_spec::CHANNELS_MAX;

use std::fmt;

/// Channel position labels.
#[derive(Debug, Copy, Clone, FromPrimitive)]
pub enum ChannelPosition {
    Mono = 0,
    /// Apple, Dolby call this 'Left'.
    FrontLeft,
    /// Apple, Dolby call this 'Right'.
    FrontRight,
    /// Apple, Dolby call this 'Center'.
    FrontCenter,
    /// Microsoft calls this 'Back Center', Apple calls this 'Center Surround', Dolby calls this 'Surround Rear Center'.
    RearCenter,
    /// Microsoft calls this 'Back Left', Apple calls this 'Left Surround' (!), Dolby calls this 'Surround Rear Left'.
    RearLeft,
    /// Microsoft calls this 'Back Right', Apple calls this 'Right Surround' (!), Dolby calls this 'Surround Rear Right'.
    RearRight,
    /// Microsoft calls this 'Low Frequency', Apple calls this 'LFEScreen'.
    Lfe,
    /// Apple, Dolby call this 'Left Center'.
    FrontLeftOfCenter,
    /// Apple, Dolby call this 'Right Center'.
    FrontRightOfCenter,
    /// Apple calls this 'Left Surround Direct', Dolby calls this 'Surround Left' (!).
    SideLeft,
    /// Apple calls this 'Right Surround Direct', Dolby calls this 'Surround Right' (!).
    SideRight,
    Aux0,
    Aux1,
    Aux2,
    Aux3,
    Aux4,
    Aux5,
    Aux6,
    Aux7,
    Aux8,
    Aux9,
    Aux10,
    Aux11,
    Aux12,
    Aux13,
    Aux14,
    Aux15,
    Aux16,
    Aux17,
    Aux18,
    Aux19,
    Aux20,
    Aux21,
    Aux22,
    Aux23,
    Aux24,
    Aux25,
    Aux26,
    Aux27,
    Aux28,
    Aux29,
    Aux30,
    Aux31,
    /// Apple calls this 'Top Center Surround'.
    TopCenter,
    /// Apple calls this 'Vertical Height Left'.
    TopFrontLeft,
    /// Apple calls this 'Vertical Height Right'.
    TopFrontRight,
    /// Apple calls this 'Vertical Height Center'.
    TopFrontCenter,
    /// Microsoft and Apple call this 'Top Back Left'.
    TopRearLeft,
    /// Microsoft and Apple call this 'Top Back Right'.
    TopRearRight,
    /// Microsoft and Apple call this 'Top Back Center'.
    TopRearCenter,
}

/// A map from stream channels to speaker positions.
///
/// These values are relevant for conversion and mixing of streams.
#[derive(Clone)]
pub struct ChannelMap {
    /// Number of channels in the map.
    channels: u8,
    /// Channel position map.
    map: [ChannelPosition; CHANNELS_MAX as usize],
}

// FIXME are empty channel maps accepted by PA?

impl ChannelMap {
    /// Creates an empty channel map.
    pub fn new() -> Self {
        Self {
            channels: 0,
            map: [ChannelPosition::Mono; CHANNELS_MAX as usize],
        }
    }

    /// Tries to append another `ChannelPosition` to the end of this map.
    ///
    /// If the map is already at max. capacity, returns a `MapFullError`.
    pub fn push(&mut self, position: ChannelPosition) -> Result<(), MapFullError> {
        *(self.map.get_mut(self.channels as usize).ok_or(MapFullError {})?) = position;
        self.channels += 1;
        Ok(())
    }

    /// Returns the number of channel mappings stored in this `ChannelMap`.
    pub fn len(&self) -> u8 {
        self.channels
    }
}

impl fmt::Debug for ChannelMap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Only print the occupied part of the backing storage
        self.map[..self.channels.into()].fmt(f)
    }
}

impl<'a> IntoIterator for &'a ChannelMap {
    type Item = ChannelPosition;
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> <Self as IntoIterator>::IntoIter {
        Iter {
            map: self,
            next: 0,
        }
    }
}

/// An iterator over `ChannelPosition`s stored in a `ChannelMap`.
#[derive(Debug)]
pub struct Iter<'a> {
    map: &'a ChannelMap,
    next: u8,
}

impl<'a> Iterator for Iter<'a> {
    type Item = ChannelPosition;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        if self.next < self.map.len() {
            self.next += 1;
            Some(self.map.map[self.next as usize - 1])
        } else {
            None
        }
    }
}

/// An error indicating that a channel map is already full and cannot be extended.
#[derive(Debug)]
pub struct MapFullError {}
