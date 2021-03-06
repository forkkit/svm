use svm_nibble::{NibbleIter, NibbleWriter};
use svm_types::{App, SpawnApp, TemplateAddr, WasmValue};

pub use crate::api::raw::{
    decode_abi_data, decode_varuint14, decode_version, encode_abi_data, encode_varuint14, Field,
};

use crate::{error::ParseError, helpers};

/// Encodes a raw Spawn-App transaction.
pub fn encode_spawn_app(spawn: &SpawnApp, w: &mut NibbleWriter) {
    encode_version(spawn, w);
    encode_template(spawn, w);
    encode_name(spawn, w);
    encode_ctor(spawn, w);
    encode_ctor_calldata(spawn, w);
}

/// Parsing a raw `spawn-app` transaction given as raw bytes.
/// Returns the parsed transaction as a tuple consisting of an `App` struct and `ctor_name` buffer args.
/// On failure, returns `ParseError`.
pub fn decode_spawn_app(iter: &mut NibbleIter) -> Result<SpawnApp, ParseError> {
    let version = decode_version(iter)?;
    let template = decode_template(iter)?;
    let name = decode_name(iter)?;
    let ctor_name = decode_ctor(iter)?;
    let calldata = decode_ctor_calldata(iter)?;

    let app = App {
        version,
        name,
        template,
    };

    let spawn = SpawnApp {
        app,
        ctor_name,
        calldata,
    };

    Ok(spawn)
}

/// Encoders

fn encode_version(spawn: &SpawnApp, w: &mut NibbleWriter) {
    let version = spawn.app.version;
    crate::api::raw::encode_version(version, w);
}

fn encode_name(spawn: &SpawnApp, w: &mut NibbleWriter) {
    let name = &spawn.app.name;
    helpers::encode_string(name, w);
}

fn encode_template(spawn: &SpawnApp, w: &mut NibbleWriter) {
    let template = &spawn.app.template;
    helpers::encode_address(template.inner(), w);
}

fn encode_ctor(spawn: &SpawnApp, w: &mut NibbleWriter) {
    let ctor_name = &spawn.ctor_name;

    helpers::encode_string(ctor_name, w);
}

fn encode_ctor_calldata(spawn: &SpawnApp, w: &mut NibbleWriter) {
    let ctor_calldata = &*spawn.calldata;
    encode_abi_data(ctor_calldata, w);
}

/// Decoders

fn decode_template(iter: &mut NibbleIter) -> Result<TemplateAddr, ParseError> {
    let addr = helpers::decode_address(iter, Field::TemplateAddr)?;

    Ok(TemplateAddr::new(addr))
}

fn decode_name(iter: &mut NibbleIter) -> Result<String, ParseError> {
    helpers::decode_string(iter, Field::NameLength, Field::Name)
}

fn decode_ctor(iter: &mut NibbleIter) -> Result<String, ParseError> {
    helpers::decode_string(iter, Field::NameLength, Field::Name)
}

fn decode_ctor_calldata(iter: &mut NibbleIter) -> Result<Vec<u8>, ParseError> {
    decode_abi_data(iter)
}

#[cfg(test)]
mod tests {
    use super::*;

    use svm_types::Address;

    #[test]
    fn encode_decode_spawn_app() {
        let spawn = SpawnApp {
            app: App {
                version: 0,
                name: "my-app".to_string(),
                template: Address::of("my-template").into(),
            },
            ctor_name: "initialize".to_string(),
            calldata: vec![0x10, 0x20, 0x30],
        };

        let mut w = NibbleWriter::new();
        encode_spawn_app(&spawn, &mut w);

        let bytes = w.into_bytes();
        let mut iter = NibbleIter::new(&bytes[..]);

        let decoded = decode_spawn_app(&mut iter).unwrap();

        assert_eq!(spawn, decoded);
    }
}
