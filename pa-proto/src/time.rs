//! Defines custom time units for use with PulseAudio.

/// A (whole) number of microseconds.
///
/// Used for specifying latencies.
#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq)]
pub struct Microseconds(pub u64);
