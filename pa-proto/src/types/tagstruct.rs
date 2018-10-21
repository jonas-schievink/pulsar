//! A "tagstruct" is PulseAudio's central IPC data structure.
//!
//! A tagstruct is a sequence of type-tagged `Value`s. This module provides parsers for the format
//! and writers to easily create tagstruct byte streams.

use types::proplist::PropList;
use types::sample_spec::{SampleFormat, SampleSpec, CHANNELS_MAX};
use types::channel_map::{ChannelMap, ChannelPosition};
use types::cvolume::{CVolume, Volume};
use types::FormatInfo;
use error::Error;
use time::Microseconds;

use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use num_traits::FromPrimitive;
use std::io::{self, Cursor};
use std::io::prelude::*;
use std::{mem, fmt, u32};
use std::ffi::CStr;
use string::PaStr;
use string::PaString;
use string::UnicodeCString;

/// Max. size of a proplist value in Bytes.
const MAX_PROP_SIZE: u32 = 64 * 1024;

#[allow(bad_style)]
#[repr(u8)]
#[derive(Debug, Copy, Clone, FromPrimitive)]
enum Tag {
    STRING = b't',
    STRING_NULL = b'N',
    U32 = b'L',
    U8 = b'B',
    U64 = b'R',
    S64 = b'r',
    SAMPLE_SPEC = b'a',
    ARBITRARY = b'x',
    BOOLEAN_TRUE = b'1',
    BOOLEAN_FALSE = b'0',
    TIMEVAL = b'T',
    USEC = b'U',
    CHANNEL_MAP = b'm',
    CVOLUME = b'v',
    PROPLIST = b'P',
    VOLUME = b'V',
    FORMAT_INFO = b'f',
}


// TODO: implement these:

/// tv_sec: u32
/// tv_usec: u32
#[derive(Debug, Copy, Clone)]
pub enum Timeval {}

/// Enum of the different values that can be stored in a tagstruct.
#[derive(Debug, Clone)]
pub enum Value<'a> {
    /// Zero-terminated string without prefix length. The zero is *not* included in the slice.
    ///
    /// Per construction, the string data cannot contain any nul bytes.
    String(&'a CStr),
    /// Encodes a string that's a null pointer.
    ///
    /// This is distinguishable from an empty string and perhaps analogous to `Option::None`.
    NullString,
    U32(u32),
    U8(u8),
    U64(u64),
    S64(i64),
    SampleSpec(SampleSpec),
    /// Byte Blob with prefix length.
    Arbitrary(&'a [u8]),
    Boolean(bool),
    Timeval(Timeval),
    Usec(Microseconds),
    ChannelMap(ChannelMap),
    CVolume(CVolume),
    PropList(PropList),
    Volume(Volume),
    FormatInfo(FormatInfo),
}

impl From<u8> for Value<'static> {
    fn from(i: u8) -> Self {
        Value::U8(i)
    }
}

impl From<u32> for Value<'static> {
    fn from(i: u32) -> Self {
        Value::U32(i)
    }
}

impl From<u64> for Value<'static> {
    fn from(i: u64) -> Self {
        Value::U64(i)
    }
}

impl From<i64> for Value<'static> {
    fn from(i: i64) -> Self {
        Value::S64(i)
    }
}

impl From<bool> for Value<'static> {
    fn from(b: bool) -> Self {
        Value::Boolean(b)
    }
}

impl From<Microseconds> for Value<'static> {
    fn from(us: Microseconds) -> Self {
        Value::Usec(us)
    }
}

impl<'a> From<&'a [u8]> for Value<'a> {
    fn from(bytes: &'a [u8]) -> Self {
        Value::Arbitrary(bytes)
    }
}

// Conversion from strings isn't safe since they can contain internal nul bytes.

macro_rules! from_x_for_value_owned {
    ($($t:ident),+) => { $(
        impl From<$t> for Value<'static> {
            fn from(v: $t) -> Self {
                Value::$t(v)
            }
        }
    )+ };
}

from_x_for_value_owned!(SampleSpec, Timeval, ChannelMap, CVolume, PropList, FormatInfo);

/// Cheap lifting of `&T` to `T` when `T: Copy`.
impl<'a, 'b, T> From<&'a T> for Value<'b>
where
    T: Into<Value<'b>> + Copy {

    fn from(t: &T) -> Self {
        (*t).into()
    }
}

macro_rules! read_typed {
    ($method:ident = Value::$variant:ident -> $t:ty) => {
        pub fn $method(&mut self) -> Result<$t, Error> {
            match self.expect()? {
                Value::$variant(v) => Ok(v),
                v => Err(Error::string(format!(concat!("expected ", stringify!($t), ", got {:?}"), v))),
            }
        }
    };
}

/// Streaming zero-copy reader for untrusted data.
///
/// The data stream is parsed and checked on-the-fly so everything in here returns `Result`s.
#[derive(Clone)]
pub struct TagStructReader<'a> {
    data: Cursor<&'a [u8]>,
}

impl<'a> TagStructReader<'a> {
    /// Creates a tagstruct reader from raw bytes (eg. a packet payload).
    pub fn from_raw(raw: &'a [u8]) -> Self {
        TagStructReader {
            data: Cursor::new(raw),
        }
    }

    /// Resets the reader to the beginning of the associated data stream.
    pub fn reset(&mut self) {
        let data = mem::replace(&mut self.data, Cursor::new(Default::default())).into_inner();
        self.data = Cursor::new(data);
    }

    /// Creates an identical `TagStruct` with the data pointer reset to the beginning.
    pub fn to_reset(&self) -> TagStructReader {
        TagStructReader {
            data: Cursor::new(self.data.get_ref().as_ref().into()),
        }
    }

    /// Read a given number of bytes from the data stream.
    ///
    /// Returns an error if the stream ends prematurely.
    fn read_n(&mut self, bytes: usize) -> Result<&'a [u8], Error> {
        let pos = self.data.position() as usize;
        let left = self.data.get_ref()[pos..].len();
        if bytes > left {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "end of data reached when reading bytes from tagstruct"
            ).into());
        }

        self.data.consume(bytes);  // consume bytes we just read

        let slice = &(*self.data.get_ref())[pos..pos+bytes];
        assert_eq!(slice.len(), bytes);
        Ok(slice)
    }

    /// Read bytes from the data stream until a specific termination byte is reached.
    ///
    /// The termination byte is contained in the returned slice.
    fn read_until(&mut self, until: u8) -> Result<&'a [u8], Error> {
        // equivalent to `BufRead::read_until`
        // reading from a Cursor without allocation requires a small dance
        let pos = self.data.position() as usize;

        let length = self.data
            .get_ref()[pos..]   // get data still left to read
            .iter()
            .position(|&byte| byte == until)    // find delimiter or bail
            .ok_or_else(|| Error::string(format!("expected delimiter 0x{:02X} not found in stream", until)))?;

        let slice = self.read_n(length + 1)?;   // with terminator
        assert_eq!(slice.last(), Some(&until), "expected terminator in data");

        Ok(&slice[..slice.len()])
    }

    /// Reads the next value or returns `None` when at EOF.
    pub fn read(&mut self) -> Result<Option<Value<'a>>, Error> {
        use self::Tag::*;

        let raw_tag = match self.data.read_u8() {
            Ok(tag) => tag,
            // Return Ok(None) for well-behaved EOF
            Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        let tag = Tag::from_u8(raw_tag)
            .ok_or_else(|| Error::string(format!("invalid tag 0x{:02X} in tagstruct", raw_tag)))?;
        Ok(Some(match tag {
            STRING => {
                let data = self.read_until(0x00)?;
                Value::String(CStr::from_bytes_with_nul(data)
                    .expect("couldn't create CStr"))
            }
            STRING_NULL => Value::NullString,
            U32 => Value::U32(self.data.read_u32::<NetworkEndian>()?),
            U8 => Value::U8(self.data.read_u8()?),
            U64 => Value::U64(self.data.read_u64::<NetworkEndian>()?),
            S64 => Value::S64(self.data.read_i64::<NetworkEndian>()?),
            ARBITRARY => {
                // prefix length u32
                let len = self.data.read_u32::<NetworkEndian>()?;
                let data = self.read_n(len as usize)?;
                Value::Arbitrary(data)
            }
            BOOLEAN_TRUE => Value::Boolean(true),
            BOOLEAN_FALSE => Value::Boolean(false),
            PROPLIST => {
                // A proplist is a key-value map with string keys and blob values.
                // It's stored as a sequence of keys and values, terminated by a null string.
                let mut proplist = PropList::new();
                while let Some(key) = self.read_string()? {
                    if key.to_bytes().is_empty() {
                        return Err(Error::string(format!("proplist key is empty")));
                    }

                    let key = key.to_str()
                        .map_err(|e| Error::string(format!("proplist key contains invalid utf-8: {}", e)))?;
                    if !key.is_ascii() {
                        return Err(Error::string(format!("proplist key contains non-ASCII characters: {:?}", key)));
                    }

                    let data_len = self.read_u32()?;
                    if data_len > MAX_PROP_SIZE {
                        return Err(Error::string(format!("proplist value size {} exceeds hard limit of {} bytes", data_len, MAX_PROP_SIZE)));
                    }

                    let data = self.read_sized_arbitrary(data_len)?;

                    if let Some(old) = proplist.insert(UnicodeCString::from_str(key).unwrap(), data.into()) {
                        warn!("dropping old proplist entry {:?} due to duplicate key {}", old, key);
                    }
                }

                Value::PropList(proplist)
            }
            SAMPLE_SPEC => {
                let (format, channels, rate) = (self.data.read_u8()?, self.data.read_u8()?, self.data.read_u32::<NetworkEndian>()?);

                let format = SampleFormat::from_u8(format)
                    .ok_or_else(|| Error::string(format!("invalid sample format 0x{:02X}", format)))?;

                Value::SampleSpec(SampleSpec::new_checked(format, channels, rate)
                    .map_err(|e| Error::string(e.to_string()))?)
            }
            CHANNEL_MAP => {
                let channels = self.data.read_u8()?;
                if channels > CHANNELS_MAX {
                    return Err(Error::string(format!("channel map too large (max is {} channels, got {})", CHANNELS_MAX, channels)));
                }

                let mut map = ChannelMap::new();
                for _ in 0..channels {
                    let raw = self.data.read_u8()?;
                    map.push(ChannelPosition::from_u8(raw)
                            .ok_or_else(|| Error::string(format!("invalid channel position {}", raw)))?)
                        .expect("channel map full despite channels being in range");
                }

                Value::ChannelMap(map)
            }
            CVOLUME => {
                // Very similar to channel maps
                let channels = self.data.read_u8()?;
                if channels == 0 || channels > CHANNELS_MAX {
                    return Err(Error::string(format!("invalid cvolume channel count {}, must be between 1 and {}", channels, CHANNELS_MAX)));
                }

                let mut volumes = CVolume::new();
                for _ in 0..channels {
                    let raw = self.data.read_u32::<NetworkEndian>()?;
                    volumes.push(Volume::from_u32_clamped(raw))
                        .expect("cvolume push failed despite channels being in range");
                }

                Value::CVolume(volumes)
            }
            USEC => {
                Value::Usec(Microseconds(self.data.read_u64::<NetworkEndian>()?))
            }
            VOLUME => {
                Value::Volume(Volume::from_u32_clamped(self.data.read_u32::<NetworkEndian>()?))
            }
            FORMAT_INFO => {
                let encoding = self.read_u8()?;
                let props = self.read_proplist()?;
                Value::FormatInfo(FormatInfo::from_raw(encoding, props)
                    .map_err(|e| Error::string(e.to_string()))?)
            }
            TIMEVAL => unimplemented!("tagstruct tag {:?}", tag),
        }))
    }

    /// Read the next `Value`, treating EOF as an error.
    pub fn expect(&mut self) -> Result<Value<'a>, Error> {
        self.read()?.ok_or_else(|| Error::string("unexpected end of stream"))
    }

    // Helper methods that skip the `Value` enum:

    read_typed!(read_u32 = Value::U32 -> u32);
    read_typed!(read_u8 = Value::U8 -> u8);
    read_typed!(read_u64 = Value::U64 -> u64);
    read_typed!(read_i64 = Value::S64 -> i64);
    read_typed!(read_bool = Value::Boolean -> bool);
    read_typed!(read_arbitrary = Value::Arbitrary -> &'a [u8]);
    read_typed!(read_string_non_null = Value::String -> &'a CStr);
    read_typed!(read_proplist = Value::PropList -> PropList);
    read_typed!(read_sample_spec = Value::SampleSpec -> SampleSpec);
    read_typed!(read_channel_map = Value::ChannelMap -> ChannelMap);
    read_typed!(read_cvolume = Value::CVolume -> CVolume);
    read_typed!(read_format_info = Value::FormatInfo -> FormatInfo);

    /// Reads a `Value::Arbitrary` with an expected size.
    ///
    /// If the next value is not a `Value::Arbitrary` or has the wrong length, or the tagstruct is
    /// at EOF, an error is returned.
    pub fn read_sized_arbitrary(&mut self, expected_length: u32) -> Result<&'a [u8], Error> {
        let a = self.read_arbitrary()?;
        if a.len() != expected_length as usize {
            return Err(Error::string(format!("expected arbitrary of length {}, got length {}", expected_length, a.len())));
        }

        Ok(a)
    }

    /// Reads the next value from the tagstruct and expects it to be a string or a null string.
    ///
    /// If the next value is not a `Value::String` or a `Value::NullString` (or the tagstruct is at
    /// EOF), an error is returned.
    ///
    /// If the next value is a `Value::String`, returns an `Ok(Some(<string>))`. If the next value
    /// is a `Value::NullString`, returns an `Ok(None)`.
    ///
    /// Note that strings in a tagstruct aren't necessarily encoded in UTF-8.
    pub fn read_string(&mut self) -> Result<Option<&'a CStr>, Error> {
        match self.expect()? {
            Value::String(s) => Ok(Some(s)),
            Value::NullString => Ok(None),
            v => Err(Error::string(format!("expected string or null string, got {:?}", v))),
        }
    }

    /// Tries to read the rest of `self`, checking if the tagstruct is encoded correctly. Then
    /// returns an iterator over the contained values.
    // TODO more docs
    pub fn checked_iter(&self) -> Result<impl Iterator<Item=Value<'a>>, Error> {
        let clone = self.clone();
        let res: Result<(), Error> = clone.map(|result| result.map(|_| ())).collect();
        res?;

        Ok(UnwrappingIterator {
            inner: self.clone(),
        })
    }
}

impl<'a> Iterator for TagStructReader<'a> {
    type Item = Result<Value<'a>, Error>;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        // Transpose the result returned by `read`
        match self.read() {
            Ok(None) => None,
            Ok(Some(value)) => Some(Ok(value)),
            Err(e) => Some(Err(e)),
        }
    }
}

impl<'a> fmt::Debug for TagStructReader<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut list = f.debug_list();

        // Format tagstruct without advancing
        let mut fresh = self.to_reset();
        let result = || -> Result<(), Error> {
            loop {
                if let Some(val) = fresh.read()? {
                    list.entry(&val);
                } else {
                    break;
                }
            }

            Ok(())
        }();

        match result {
            Ok(()) => {},
            Err(e) => {
                list.entry(&format!("error reading tagstruct: {}", e));
            },
        }

        list.finish()
    }
}

pub struct TagStructWriter<'a> {
    buf: &'a mut Vec<u8>,
}

impl<'a> TagStructWriter<'a> {
    /// Creates a tagstruct writer that writes to a reusable buffer.
    pub fn new(buf: &'a mut Vec<u8>) -> Self {
        buf.clear();
        Self { buf }
    }

    /// Cheaply convert this writer to a tagstruct reader that will read back the data written by
    /// this writer.
    pub fn to_reader(&'a self) -> TagStructReader<'a> {
        TagStructReader::from_raw(self.buf)
    }

    fn try_write(&mut self, value: &Value) -> io::Result<()> {
        use self::Value::*;

        // Forward to the `ToTagStruct` impls. Protocol version doesn't matter since these are
        // primitive data types making up the tagstruct format.
        match value {
            String(s) => s.to_tag_struct(self, 0),
            NullString => {
                self.buf.write_u8(Tag::STRING_NULL as u8)?;
                Ok(())
            },
            U32(n) => n.to_tag_struct(self, 0),
            U8(n) => n.to_tag_struct(self, 0),
            U64(n) => n.to_tag_struct(self, 0),
            S64(n) => n.to_tag_struct(self, 0),
            SampleSpec(_) => unimplemented!(),
            Arbitrary(bytes) => bytes.to_tag_struct(self, 0),
            Boolean(b) => b.to_tag_struct(self, 0),
            Timeval(_) => unimplemented!(),
            Usec(n) => n.to_tag_struct(self, 0),
            ChannelMap(_) |
            CVolume(_) |
            PropList(_) |
            Volume(_) |
            FormatInfo(_) => unimplemented!(),
        }.expect("primitive to_tag_struct failed");

        Ok(())
    }

    /// Appends a single value to the tagstruct.
    ///
    /// To append multiple values at once, use the `Extend` implementation.
    pub fn write<T: ToTagStruct + VersionIndependent>(&mut self, value: T) {
        // this cannot fail when we're writing into a `Vec<u8>`
        value.to_tag_struct(self, 0).expect("to_tag_struct failed")
    }

    pub fn write_versioned<T: ToTagStruct>(&mut self, value: T, protocol_version: u16) {
        value.to_tag_struct(self, protocol_version).expect("to_tag_struct failed")
    }

    pub fn write_value(&mut self, value: &Value) {
        self.try_write(value).unwrap();
    }

    pub fn iter(&'a self) -> impl Iterator<Item=Value<'a>> + 'a {
        UnwrappingIterator {
            inner: self.to_reader(),
        }
    }
}

impl<'a> IntoIterator for &'a TagStructWriter<'a> {
    type Item = Value<'a>;
    type IntoIter = UnwrappingIterator<Value<'a>, Error, TagStructReader<'a>>;

    fn into_iter(self) -> <Self as IntoIterator>::IntoIter {
        UnwrappingIterator {
            inner: self.to_reader(),
        }
    }
}

impl<'a, 'v> Extend<Value<'v>> for TagStructWriter<'a> {
    fn extend<T: IntoIterator<Item=Value<'v>>>(&mut self, iter: T) {
        for value in iter {
            self.write(value);
        }
    }
}

impl<'a> fmt::Debug for TagStructWriter<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.to_reader().fmt(f)
    }
}

/// An iterator adapter that unwraps the yielded `Result`, causing a panic on
/// any yielded errors.
#[derive(Debug)]
pub struct UnwrappingIterator<T, E, I>
where
    E: fmt::Debug,
    I: Iterator<Item=Result<T, E>>,
{
    inner: I,
}

impl<T, E, I> Iterator for UnwrappingIterator<T, E, I>
where
    E: fmt::Debug,
    I: Iterator<Item=Result<T, E>>
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(Result::unwrap)
    }
}

/// Trait implemented by types that can be serialized into a tagstruct.
pub trait ToTagStruct {
    /// Write `self` into a tagstruct.
    fn to_tag_struct(&self, w: &mut TagStructWriter, protocol_version: u16) -> Result<(), Error>;
}

/// Implemented by types that can be deserialized from a tagstruct.
pub trait FromTagStruct<'a>: Sized {
    /// Read an instance of `Self` from a tagstruct.
    ///
    /// # Parameters
    ///
    /// * `ts`: The tagstruct to read from.
    /// * `protocol_version`: PulseAudio protocol version, used to decide on the precise data
    ///   format. For old versions, default values might be used for parts of `Self`.
    fn from_tag_struct(ts: &mut TagStructReader<'a>, protocol_version: u16) -> Result<Self, Error>;
}

/// Marker trait for tagstruct-serializable types that do not depend on the protocol version.
///
/// Implemented mostly by the primitive types used in tagstructs.
pub trait VersionIndependent {}

impl<'a, T: ?Sized> ToTagStruct for &'a T where T: ToTagStruct {
    fn to_tag_struct(&self, w: &mut TagStructWriter, protocol_version: u16) -> Result<(), Error> {
        (*self).to_tag_struct(w, protocol_version)
    }
}

impl<'a> ToTagStruct for Value<'a> {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        w.write_value(self);
        Ok(())
    }
}

impl ToTagStruct for bool {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        let tag = if *self { Tag::BOOLEAN_TRUE } else { Tag::BOOLEAN_FALSE };
        w.buf.write_u8(tag as u8)?;
        Ok(())
    }
}

impl ToTagStruct for u8 {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        w.buf.write_u8(Tag::U8 as u8)?;
        w.buf.write_u8(*self)?;
        Ok(())
    }
}

impl ToTagStruct for u32 {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        w.buf.write_u8(Tag::U32 as u8)?;
        w.buf.write_u32::<NetworkEndian>(*self)?;
        Ok(())
    }
}

impl ToTagStruct for u64 {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        w.buf.write_u8(Tag::U64 as u8)?;
        w.buf.write_u64::<NetworkEndian>(*self)?;
        Ok(())
    }
}

impl ToTagStruct for i64 {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        w.buf.write_u8(Tag::S64 as u8)?;
        w.buf.write_i64::<NetworkEndian>(*self)?;
        Ok(())
    }
}

impl<'a> ToTagStruct for &'a CStr {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        w.buf.write_u8(Tag::STRING as u8)?;
        w.buf.write_all(self.to_bytes_with_nul())?;
        Ok(())
    }
}

impl<'a> ToTagStruct for &'a PaStr {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        w.buf.write_u8(Tag::STRING as u8)?;
        w.buf.write_all(self.to_bytes_with_nul())?;
        Ok(())
    }
}

impl<'a> ToTagStruct for Option<&'a PaStr> {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        match self {
            Some(s) => w.write(s),
            None => w.buf.write_u8(Tag::STRING_NULL as u8)?,
        }
        Ok(())
    }
}

impl<'a> ToTagStruct for &'a UnicodeCString {
    fn to_tag_struct(&self, w: &mut TagStructWriter, protocol_version: u16) -> Result<(), Error> {
        self.as_cstr().to_tag_struct(w, protocol_version)
    }
}

impl ToTagStruct for PaString {
    fn to_tag_struct(&self, w: &mut TagStructWriter, protocol_version: u16) -> Result<(), Error> {
        self.as_pastr().to_tag_struct(w, protocol_version)
    }
}

impl ToTagStruct for Microseconds {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        w.buf.write_u8(Tag::USEC as u8)?;
        w.buf.write_u64::<NetworkEndian>(self.0)?;
        Ok(())
    }
}

impl<'a> ToTagStruct for &'a [u8] {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        assert!(self.len() <= u32::MAX as usize);
        w.buf.write_u8(Tag::ARBITRARY as u8)?;
        w.buf.write_u32::<NetworkEndian>(self.len() as u32)?;
        w.buf.write_all(self)?;
        Ok(())
    }
}

impl ToTagStruct for PropList {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        w.buf.write_u8(Tag::PROPLIST as u8)?;
        for (k, v) in self {
            assert!(v.len() < u32::MAX as usize);
            w.write(k);
            w.write(v.len() as u32);
            w.write(&**v);
        }
        w.write(None);
        Ok(())
    }
}

impl ToTagStruct for Volume {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        w.buf.write_u8(Tag::VOLUME as u8)?;
        w.buf.write_u32::<NetworkEndian>(self.as_u32())?;
        Ok(())
    }
}

impl ToTagStruct for SampleSpec {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        w.buf.write_u8(Tag::SAMPLE_SPEC as u8)?;
        w.buf.write_u8(self.format() as u8)?;
        w.buf.write_u8(self.channels())?;
        w.buf.write_u32::<NetworkEndian>(self.sample_rate())?;
        Ok(())
    }
}

impl ToTagStruct for ChannelMap {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        w.buf.write_u8(Tag::CHANNEL_MAP as u8)?;
        w.buf.write_u8(self.len())?;
        for channel_pos in self {
            w.buf.write_u8(channel_pos as u8)?;
        }
        Ok(())
    }
}

impl ToTagStruct for FormatInfo {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        w.buf.write_u8(Tag::FORMAT_INFO as u8)?;
        w.write(self.encoding() as u8);
        w.write(self.props());
        Ok(())
    }
}

impl ToTagStruct for CVolume {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        w.buf.write_u8(Tag::CVOLUME as u8)?;
        w.buf.write_u8(self.len())?;
        for volume in self {
            w.buf.write_u32::<NetworkEndian>(volume.as_u32())?;
        }
        Ok(())
    }
}

impl<'a, T: ?Sized> VersionIndependent for &'a T where T: VersionIndependent {}
impl<'a> VersionIndependent for Value<'a> {}
impl VersionIndependent for bool {}
impl VersionIndependent for u8 {}
impl VersionIndependent for u32 {}
impl VersionIndependent for u64 {}
impl VersionIndependent for i64 {}
impl VersionIndependent for Microseconds {}
impl VersionIndependent for PaString {}
impl<'a> VersionIndependent for &'a CStr {}
impl<'a> VersionIndependent for &'a PaStr {}
impl<'a> VersionIndependent for Option<&'a PaStr> {}
impl<'a> VersionIndependent for &'a UnicodeCString {}
impl<'a> VersionIndependent for &'a [u8] {}
impl VersionIndependent for PropList {}
impl VersionIndependent for Volume {}
impl VersionIndependent for SampleSpec {}
impl VersionIndependent for ChannelMap {}
impl VersionIndependent for FormatInfo {}
impl VersionIndependent for CVolume {}

// TODO: `tagstruct` vs. `tag_struct` and `TagStruct` vs. `Tagstruct`!
