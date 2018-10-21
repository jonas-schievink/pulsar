use super::prelude::*;

/// Add or modify client properties.
#[derive(Debug)]
pub struct SetClientName {
    props: PropList,
}

impl SetClientName {
    /// The properties to set.
    ///
    /// The contained proplist should be merged into the existing client properties, overwriting
    /// exiting values.
    pub fn props(&self) -> &PropList {
        &self.props
    }
}

impl<'a> FromTagStruct<'a> for SetClientName {
    fn from_tag_struct(ts: &mut TagStructReader, _protocol_version: u16) -> Result<Self, Error> {
        // before protocol version 13, *only* the client name was transferred (as a string)
        // proto>=13
        let props = ts.read_proplist()?;
        Ok(Self { props })
    }
}

impl ToTagStruct for SetClientName {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        w.write(self.props());
        Ok(())
    }
}

#[derive(Debug)]
pub struct SetClientNameReply {
    client_id: u32,
}

impl SetClientNameReply {
    pub fn new(client_id: u32) -> Self {
        Self { client_id }
    }
}

impl ToTagStruct for SetClientNameReply {
    fn to_tag_struct(&self, w: &mut TagStructWriter, _protocol_version: u16) -> Result<(), Error> {
        w.write(self.client_id);
        Ok(())
    }
}
