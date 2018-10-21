//! Defines types that specify how samples are encoded.

use types::PropList;

use num_traits::FromPrimitive;

/// Describes how samples are encoded.
#[derive(Debug, Copy, Clone, FromPrimitive)]
pub enum FormatEncoding {
    /// Any encoding is supported.
    Any,
    /// Good old PCM.
    Pcm,
    /// AC3 data encapsulated in IEC 61937 header/padding.
    Ac3Iec61937,
    /// EAC3 data encapsulated in IEC 61937 header/padding.
    Eac3Iec61937,
    /// MPEG-1 or MPEG-2 (Part 3, not AAC) data encapsulated in IEC 61937 header/padding.
    MpegIec61937,
    /// DTS data encapsulated in IEC 61937 header/padding.
    DtsIec61937,
    /// MPEG-2 AAC data encapsulated in IEC 61937 header/padding. \since 4.0
    Mpeg2Iec61937,
    // TODO extensible
}

/// Sample encoding info.
///
/// Associates a simple `FormatEncoding` with a list of arbitrary properties.
#[derive(Debug, Clone)]
pub struct FormatInfo {
    encoding: FormatEncoding,
    props: PropList,
}

impl FormatInfo {
    /// Create a new `FormatInfo` from a sample encoding with an empty property list.
    pub fn new(encoding: FormatEncoding) -> Self {
        Self {
            encoding,
            props: PropList::new(),
        }
    }

    /// Create a `FormatInfo` from raw data parsed from a tagstruct.
    ///
    /// # Parameters
    ///
    /// * `encoding`: Raw value for a `FormatEncoding`.
    /// * `props`: Property list to associate with the `FormatInfo`.
    pub fn from_raw(encoding: u8, props: PropList) -> Result<Self, InvalidEncodingError> {
        let encoding = FormatEncoding::from_u8(encoding).ok_or(InvalidEncodingError::new(encoding))?;

        Ok(Self { encoding, props })
    }

    /// Get the actual sample encoding.
    pub fn encoding(&self) -> FormatEncoding { self.encoding }

    /// Get a reference to the property list for this `FormatInfo` object.
    pub fn props(&self) -> &PropList { &self.props }

    /// Get a mutable reference to the property list for this `FormatInfo` object.
    pub fn props_mut(&mut self) -> &mut PropList { &mut self.props }
}

/// Error returned for invalid values for `FormatEncoding`.
#[derive(Debug, Fail)]
#[fail(display = "{} is an invalid value for sample format encodings", raw)]
pub struct InvalidEncodingError {
    raw: u8,
}

impl InvalidEncodingError {
    fn new(raw_encoding: u8) -> Self {
        Self { raw: raw_encoding }
    }
}
