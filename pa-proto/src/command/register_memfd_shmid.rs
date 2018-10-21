use super::prelude::*;

// FIXME: Figure out exact semantics of this command
// FIXME: PA ignores this (doesn't attach the received memfd) at proto<31
#[derive(Debug)]
pub struct RegisterMemfdShmid {
    shmid: u32,
}

impl<'a> FromTagStruct<'a> for RegisterMemfdShmid {
    fn from_tag_struct(ts: &mut TagStructReader<'a>, _protocol_version: u16) -> Result<Self, Error> {
        Ok(Self {
            shmid: ts.read_u32()?,
        })
    }
}

impl ToTagStruct for RegisterMemfdShmid {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        w.write(self.shmid);
        Ok(())
    }
}
