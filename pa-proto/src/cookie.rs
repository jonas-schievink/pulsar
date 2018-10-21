//! Authentication token.

use rand::thread_rng;
use rand::prelude::*;

use std::path::Path;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::io::{self, Result};
use std::{fmt, fs};
use std::os::unix::fs::{OpenOptionsExt, MetadataExt};

const COOKIE_LENGTH: usize = 256;

/// A randomly generated blob that is made accessible only to the user running the server.
pub struct AuthCookie {
    data: [u8; COOKIE_LENGTH],
}

impl AuthCookie {
    /// Loads an existing cookie from disk or generates one and writes it to disk.
    ///
    /// # Parameters
    ///
    /// * `file`: Path to the cookie file.
    #[allow(unused)]
    pub fn load_or_create<P: AsRef<Path>>(file: P) -> Result<Self> {
        Self::load(file.as_ref())
            .or_else(|_| Self::create(file.as_ref()))
    }

    /// Generates a new cookie and stores it to disk, overwriting any existing cookie file.
    pub fn create<P: AsRef<Path>>(file: P) -> Result<Self> {
        info!("generating new auth cookie at {}", file.as_ref().display());

        // TODO check dir access permissions
        fs::remove_file(&file).ok();

        const ACCESS_MODE: u32 = 0o600;  // -rw------
        let mut file = OpenOptions::new()
            .mode(ACCESS_MODE)
            .write(true)
            .create(true)
            .open(file)?;

        let mode = file.metadata()?.mode() & 0o777;  // mask out access bits
        if mode != ACCESS_MODE {
            // couldn't remove file or other process intervened
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("couldn't create cookie file with correct permissions (needs 0o{:03o}, has 0o{:03o})", ACCESS_MODE, mode)
            ));
        }

        let mut data = [0; COOKIE_LENGTH];
        thread_rng().fill(&mut data);

        file.write_all(&data)?;
        file.flush()?;
        drop(file);

        Ok(AuthCookie { data })
    }

    /// Loads an existing cookie from disk.
    ///
    /// If the cookie doesn't exist, an error is returned.
    pub fn load<P: AsRef<Path>>(_file: P) -> Result<Self> {
        unimplemented!();
    }
}

impl fmt::Debug for AuthCookie {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "AuthCookie {{ (data hidden) }}")
    }
}

impl PartialEq for AuthCookie {
    fn eq(&self, other: &Self) -> bool {
        &self.data[..] == &other.data[..]
    }
}

impl<'a> PartialEq<&'a [u8]> for AuthCookie {
    fn eq(&self, other: &&[u8]) -> bool {
        &self.data[..] == *other
    }
}

impl Eq for AuthCookie {}
