use std::iter::FromIterator;
use std::iter::Iterator;
use std::mem::transmute;

use core::convert::*;
use serde::{de::DeserializeOwned, ser::Serialize};

#[derive(Debug, Clone, Hash, PartialEq)]
pub struct OutgoingMessage {
    pub data: Vec<u8>,
}

impl OutgoingMessage {
    pub fn new() -> OutgoingMessage {
        OutgoingMessage { data: Vec::new() }
    }

    pub fn write_u8(&mut self, value: u8) {
        self.data.push(value);
    }

    pub fn write_bool(&mut self, value: bool) {
        self.write_u8(value as u8);
    }

    pub fn write_i8(&mut self, value: i8) {
        self.write_u8(value as u8);
    }

    pub fn write_u16(&mut self, value: u16) {
        unsafe {
            let bytes = transmute::<u16, [u8; 2]>(value);
            self.write_u8(bytes[0]);
            self.write_u8(bytes[1]);
        }
    }

    pub fn write_i16(&mut self, value: i16) {
        self.write_u16(value as u16);
    }

    pub fn write_u32(&mut self, value: u32) {
        unsafe {
            let shorts = transmute::<u32, [u16; 2]>(value);
            self.write_u16(shorts[0]);
            self.write_u16(shorts[1]);
        }
    }

    pub fn write_i32(&mut self, value: i32) {
        self.write_u32(value as u32);
    }

    pub fn write_u64(&mut self, value: u64) {
        unsafe {
            let ints = transmute::<u64, [u32; 2]>(value);
            self.write_u32(ints[0]);
            self.write_u32(ints[1]);
        }
    }

    pub fn write_i64(&mut self, value: i64) {
        self.write_u64(value as u64);
    }

    pub fn write_f32(&mut self, value: f32) {
        unsafe {
            self.write_u32(transmute::<f32, u32>(value));
        }
    }

    pub fn write_f64(&mut self, value: f64) {
        unsafe {
            self.write_u64(transmute::<f64, u64>(value));
        }
    }

    pub fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }

    pub fn write_isize(&mut self, value: isize) {
        self.write_i64(value as i64);
    }

    pub fn write_u8s<T>(&mut self, values: T)
    where
        T: AsRef<[u8]>,
    {
        let bytes = values.as_ref();

        self.write_usize(bytes.len());
        for byte in bytes {
            self.write_u8(*byte);
        }
    }

    pub fn write_data<T>(&mut self, values: T)
    where
        T: AsRef<[u8]>,
    {
        let data = values.as_ref();

        for byte in data {
            self.write_u8(*byte);
        }
    }

    pub fn write_string(&mut self, value: &str) {
        self.write_u8s(value.as_bytes());
    }

    pub fn write_serializable<T: Serialize>(&mut self, value: T) {
        let binary = bincode::serialize(&value).unwrap();
        self.write_u8s(binary);
    }

    pub fn into_incoming(self) -> IncomingMessage {
        IncomingMessage::new(self.data)
    }
}

pub trait IntoOutgoingMessage {
    fn into(self) -> OutgoingMessage;
}

impl IntoOutgoingMessage for OutgoingMessage {
    fn into(self) -> OutgoingMessage {
        self
    }
}

impl<T: Serialize> IntoOutgoingMessage for T {
    fn into(self) -> OutgoingMessage {
        let mut message = OutgoingMessage::new();
        message.write_serializable(&self);
        message
    }
}

#[derive(Debug, PartialEq)]
pub struct IncomingMessage {
    data: Vec<u8>,
    cursor: usize,
}

impl IncomingMessage {
    pub fn new(data: Vec<u8>) -> IncomingMessage {
        IncomingMessage { data, cursor: 0 }
    }

    pub fn read_u8(&mut self) -> Option<u8> {
        let result = self.data.get(self.cursor)?;
        self.cursor += 1;
        Some(*result)
    }

    pub fn read_bool(&mut self) -> Option<bool> {
        Some(self.read_u8()? != 0)
    }

    pub fn read_i8(&mut self) -> Option<i8> {
        Some(self.read_u8()? as i8)
    }

    pub fn read_u16(&mut self) -> Option<u16> {
        unsafe {
            let bytes = [self.read_u8()?, self.read_u8()?];
            Some(transmute::<[u8; 2], u16>(bytes))
        }
    }

    pub fn read_i16(&mut self) -> Option<i16> {
        Some(self.read_u16()? as i16)
    }

    pub fn read_u32(&mut self) -> Option<u32> {
        unsafe {
            let shorts = [self.read_u16()?, self.read_u16()?];
            Some(transmute::<[u16; 2], u32>(shorts))
        }
    }

    pub fn read_i32(&mut self) -> Option<i32> {
        Some(self.read_u32()? as i32)
    }

    pub fn read_u64(&mut self) -> Option<u64> {
        unsafe {
            let ints = [self.read_u32()?, self.read_u32()?];
            Some(transmute::<[u32; 2], u64>(ints))
        }
    }

    pub fn read_i64(&mut self) -> Option<i64> {
        Some(self.read_u64()? as i64)
    }

    pub fn read_f32(&mut self) -> Option<f32> {
        unsafe { Some(transmute::<u32, f32>(self.read_u32()?)) }
    }

    pub fn read_f64(&mut self) -> Option<f64> {
        unsafe { Some(transmute::<u64, f64>(self.read_u64()?)) }
    }

    pub fn read_usize(&mut self) -> Option<usize> {
        Some(self.read_u64()? as usize)
    }

    pub fn read_isize(&mut self) -> Option<isize> {
        Some(self.read_u64()? as isize)
    }

    pub fn read_n_u8s(&mut self, n: usize) -> Option<Vec<u8>> {
        let mut bytes = Vec::new();
        for _ in 0..n {
            bytes.push(self.read_u8()?);
        }
        Some(bytes)
    }

    pub fn read_at_most_n_u8s(&mut self, n: usize) -> Vec<u8> {
        let length = std::cmp::min(n, self.data.len() - self.cursor);
        self.read_n_u8s(length).unwrap_or(Vec::new())
    }

    pub fn read_u8s(&mut self) -> Option<Vec<u8>> {
        let length = self.read_usize()?;
        Some(self.read_n_u8s(length)?)
    }

    pub fn read_string(&mut self) -> Option<String> {
        Some(String::from_utf8(self.read_u8s()?).ok()?)
    }

    pub fn read_serializable<T>(&mut self) -> Option<T>
    where
        T: DeserializeOwned,
    {
        let bytes = self.read_u8s()?;
        Some(bincode::deserialize_from(&bytes[..]).ok()?)
    }

    pub fn read_rest(self) -> Vec<u8> {
        Vec::from_iter(self.data.into_iter().skip(self.cursor))
    }

    pub fn at_end(&self) -> bool {
        self.cursor == self.data.len()
    }
}

#[cfg(test)]
mod test {
    use serde::{Deserialize, Serialize};

    use super::*;

    macro_rules! test_read_write {
        ( $name:ident, $value:expr ) => {
            paste::item! {
                #[test]
                fn [<written_ $name _equals_read_ $name>]() {
                    let mut outgoing = OutgoingMessage::new();
                    outgoing.[<write_ $name>]($value);

                    let mut incoming = IncomingMessage::new(outgoing.data);
                    assert_eq!(incoming.[<read_ $name>]().unwrap(), $value);
                    assert!(incoming.at_end());
                }
            }
        };
    }

    test_read_write!(u8, 7u8);
    test_read_write!(bool, true);
    test_read_write!(i8, -4i8);
    test_read_write!(u16, 1010u16);
    test_read_write!(i16, -1010i16);
    test_read_write!(u32, 101010u32);
    test_read_write!(i32, -101010i32);
    test_read_write!(u64, 10101010101u64);
    test_read_write!(i64, -10101010101i64);
    test_read_write!(usize, 10101010101usize);
    test_read_write!(isize, -10101010101isize);
    test_read_write!(f32, 3.1415926);
    test_read_write!(f64, -3.141592685358979323);
    test_read_write!(string, "Hello world!");
    test_read_write!(u8s, vec![3u8, 1u8, 4u8, 1u8, 5u8]);

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestSerializable {
        foo: usize,
        bar: String,
        baz: bool,
    }

    #[test]
    fn written_serializable_equals_read_serializable() {
        let mut outgoing = OutgoingMessage::new();
        outgoing.write_serializable(TestSerializable {
            foo: 42,
            bar: "bar".to_owned(),
            baz: false,
        });

        let mut incoming = IncomingMessage::new(outgoing.data);
        assert_eq!(
            incoming.read_serializable::<TestSerializable>().unwrap(),
            TestSerializable {
                foo: 42,
                bar: "bar".to_owned(),
                baz: false
            }
        );
        assert!(incoming.at_end());
    }

    #[test]
    fn message_read_rest_works() {
        let mut incoming = IncomingMessage::new(vec![3u8, 1u8, 4u8, 1u8, 5u8]);

        let _ = incoming.read_u8();
        let rest = incoming.read_rest();

        assert_eq!(rest, vec![1u8, 4u8, 1u8, 5u8]);
    }
}
