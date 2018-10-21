//! Defines custom string types used by this library.

use std::ffi::{CString, CStr, NulError, FromBytesWithNulError};
use std::ops::Deref;
use std::{fmt, str};
use std::str::Utf8Error;
use std::borrow::Borrow;
use std::string::FromUtf8Error;

/// A `CString` that prints as UTF-8 when possible.
///
/// Contains no interior nul bytes, but a nul terminator, and might not be valid UTF-8. It
/// implements `Display` like normal and will replace invalid code points with the replacement
/// character.
pub struct PaString {
    inner: CString,
}

impl PaString {
    /// Try to create a `PaString` from a byte vector.
    pub fn new<S: Into<Vec<u8>>>(s: S) -> Result<Self, NulError> {
        Ok(Self { inner: CString::new(s)? })
    }

    pub fn as_pastr(&self) -> &PaStr {
        self.deref()
    }
}

impl From<CString> for PaString {
    fn from(cs: CString) -> Self {
        Self { inner: cs }
    }
}

impl fmt::Display for PaString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner.to_string_lossy())
    }
}

impl fmt::Debug for PaString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.inner)
    }
}

impl Deref for PaString {
    type Target = PaStr;

    fn deref(&self) -> &<Self as Deref>::Target {
        unsafe {
            PaStr::new_unchecked(self.inner.as_bytes_with_nul())
        }
    }
}

/// A `CStr` that prints as UTF-8 when possible.
pub struct PaStr {
    // this really wants `#[repr(transparent)]`, but CStr doesn't have one either ¯\_(ツ)_/¯

    inner: CStr,
}

impl PaStr {
    unsafe fn new_unchecked(bytes: &[u8]) -> &Self {
        // #YOLO, but this is literally what libstd does so we'll be fine, governed by the
        // protection of our holy lord, rustc
        &*(bytes as *const [u8] as *const CStr as *const Self)
    }

    /// Creates a `PaStr` from a raw byte slice that must end with a nul byte and contain no other
    /// nul bytes.
    pub fn from_bytes_with_nul(bytes: &[u8]) -> Result<&Self, FromBytesWithNulError> {
        Ok(<&Self>::from(CStr::from_bytes_with_nul(bytes)?))
    }

    /// Returns the underlying byte slice without the nul terminator.
    pub fn to_bytes(&self) -> &[u8] {
        self.inner.to_bytes()
    }

    /// Returns the underlying byte slice including the nul terminator.
    pub fn to_bytes_with_nul(&self) -> &[u8] {
        self.inner.to_bytes_with_nul()
    }

    /// Tries to convert this `PaStr` to a Rust `&str` slice.
    pub fn to_str(&self) -> Result<&str, Utf8Error> {
        self.inner.to_str()
    }
}

impl fmt::Display for PaStr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner.to_string_lossy())
    }
}

impl fmt::Debug for PaStr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", &self.inner)
    }
}

impl<'a> From<&'a CStr> for &'a PaStr {
    fn from(cstr: &'a CStr) -> Self {
        unsafe { PaStr::new_unchecked(cstr.to_bytes_with_nul()) }
    }
}

impl<'a> Default for &'a PaStr {
    fn default() -> Self {
        Self::from(<&'a CStr>::default())
    }
}

/// A nul-terminated UTF-8 encoded string without interior nul bytes.
///
/// This type can be freely converted to `&str`, `&CStr` and `&PaStr`. It can be seen as the
/// intersection of `String` and `CString` (everything valid as both a `String` and a `CString` is
/// valid as a `UnicodeCString`).
#[derive(Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct UnicodeCString {
    inner: String,
}

impl UnicodeCString {
    /// Creates a `UnicodeCString` from a Rust `String`.
    ///
    /// Returns a `FromBytesWithNulError` when the string contains nul bytes.
    pub fn from_string(mut s: String) -> Result<Self, FromBytesWithNulError> {
        s.push('\0');
        CStr::from_bytes_with_nul(s.as_bytes())?;
        Ok(Self { inner: s })
    }

    /// Creates a `UnicodeCString` from a `&str` slice.
    ///
    /// Returns a `FromBytesWithNulError` when the string contains nul bytes.
    pub fn from_str(s: &str) -> Result<Self, FromBytesWithNulError> {
        let mut s = s.to_string();
        s.push('\0');
        CStr::from_bytes_with_nul(s.as_bytes())?;
        Ok(Self { inner: s })
    }

    /// Creates a `UnicodeCString` from a `&CStr`.
    ///
    /// Returns a `FromUtf8Error` when the `&CStr` isn't valid UTF-8.
    pub fn from_cstr(cs: &CStr) -> Result<Self, FromUtf8Error> {
        let mut s = String::from_utf8(cs.to_bytes().into())?;
        s.push('\0');
        Ok(Self { inner: s })
    }

    /// Get this string as a `&str` slice.
    ///
    /// This cannot fail, as a `UnicodeCString` is always valid unicode.
    pub fn as_str(&self) -> &str {
        &self.inner[..self.inner.len()-1]   // remove trailing nul
    }

    /// Get this string as a `&CStr`.
    ///
    /// This cannot fail, as a `UnicodeCString` never contains interior nul bytes (and contains a
    /// nul terminator).
    pub fn as_cstr(&self) -> &CStr {
        CStr::from_bytes_with_nul(self.inner.as_bytes())
            .expect("couldn't create CStr from UnicodeCString")
    }

    /// Get this string as a `&PaStr`.
    ///
    /// This cannot fail, as a `UnicodeCString` never contains interior nul bytes (and contains a
    /// nul terminator).
    pub fn as_pastr(&self) -> &PaStr {
        <&PaStr>::from(self.as_cstr())
    }

    // TODO: as_ vs. to_
}

impl fmt::Display for UnicodeCString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Debug for UnicodeCString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

impl Borrow<str> for UnicodeCString {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}
