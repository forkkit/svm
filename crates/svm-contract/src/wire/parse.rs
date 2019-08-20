use super::error::Error;
use super::field::Field;
use crate::wasm::WasmContract;
use svm_common::Address;

use byteorder::{BigEndian, ReadBytesExt};
use std::io::{Cursor, Read};

macro_rules! ensure_enough_bytes {
    ($res: expr, $field: expr) => {{
        if $res.is_err() {
            return Err(Error::NotEnoughBytes($field));
        }
    }};
}

/// Parsing a on-the-wire contract given as raw bytes.
/// Returns the parsed contract as a `WasmContract` struct.
#[allow(dead_code)]
pub fn parse_contract(bytes: &[u8]) -> Result<WasmContract, Error> {
    let mut cursor = Cursor::new(bytes);

    parse_version(&mut cursor)?;
    let name = parse_name(&mut cursor)?;
    let author = parse_author(&mut cursor)?;
    let admins = parse_admins(&mut cursor)?;
    let _deps = parse_deps(&mut cursor)?;
    let wasm = parse_code(&mut cursor)?;

    let contract = WasmContract {
        name,
        wasm,
        author,
        admins,
    };

    Ok(contract)
}

fn parse_version(cursor: &mut Cursor<&[u8]>) -> Result<u32, Error> {
    let res = cursor.read_u32::<BigEndian>();

    ensure_enough_bytes!(res, Field::Version);

    let version = res.unwrap();
    if version != 0 {
        return Err(Error::UnsupportedProtoVersion(version));
    }

    Ok(version)
}

fn parse_name(cursor: &mut Cursor<&[u8]>) -> Result<String, Error> {
    let res = cursor.read_u8();

    ensure_enough_bytes!(res, Field::NameLength);

    let name_len = res.unwrap() as usize;
    if name_len == 0 {
        return Err(Error::EmptyName);
    }

    let mut name_buf = Vec::<u8>::with_capacity(name_len);
    let res = cursor.read_exact(&mut name_buf);

    if res.is_err() {
        return Err(Error::NotEnoughBytes(Field::Name));
    }

    let name = String::from_utf8(name_buf);
    if name.is_err() {
        Ok(name.unwrap())
    } else {
        Err(Error::NameNotValidUTF8String)
    }
}

#[inline(always)]
fn parse_author(cursor: &mut Cursor<&[u8]>) -> Result<Address, Error> {
    parse_address(cursor, Field::Author)
}

fn parse_admins(cursor: &mut Cursor<&[u8]>) -> Result<Vec<Address>, Error> {
    let res = cursor.read_u8();

    ensure_enough_bytes!(res, Field::AdminsCount);

    let admin_count = res.unwrap() as usize;
    if admin_count > 0 {
        return Err(Error::AdminsNotSupportedYet);
    }
    // let mut admins = Vec::<Address>::with_capacity(admin_count);
    // for i in 0..admin_count {
    //     let addr = parse_address(addr, Field::Admins);
    //     admins.push(addr);
    // }

    Ok(Vec::new())
}

fn parse_deps(cursor: &mut Cursor<&[u8]>) -> Result<(), Error> {
    let res = cursor.read_u16::<BigEndian>();

    ensure_enough_bytes!(res, Field::DepsCount);

    let deps_count = res.unwrap() as usize;
    if deps_count > 0 {
        return Err(Error::DepsNotSupportedYet);
    }

    Ok(())
}

fn parse_code(cursor: &mut Cursor<&[u8]>) -> Result<Vec<u8>, Error> {
    let res = cursor.read_u64::<BigEndian>();
    ensure_enough_bytes!(res, Field::CodeLength);

    let code_len = res.unwrap() as usize;
    let mut code = Vec::<u8>::with_capacity(code_len);

    let res = cursor.read_exact(&mut code);
    ensure_enough_bytes!(res, Field::Code);

    Ok(code)
}

fn parse_address(cursor: &mut Cursor<&[u8]>, field: Field) -> Result<Address, Error> {
    let mut addr = [0; 32];
    let res = cursor.read_exact(&mut addr);
    ensure_enough_bytes!(res, field);

    Ok(Address(addr))
}
