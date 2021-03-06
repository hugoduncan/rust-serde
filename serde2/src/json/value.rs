use std::collections::{BTreeMap, btree_map};
use std::fmt;
use std::io;
use std::str;
use std::vec;

use de;
use ser;
use super::error::Error;

#[derive(PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    I64(i64),
    F64(f64),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

impl ser::Serialize for Value {
    #[inline]
    fn visit<
        V: ser::Visitor,
    >(&self, visitor: &mut V) -> Result<V::Value, V::Error> {
        match *self {
            Value::Null => visitor.visit_unit(),
            Value::Bool(v) => visitor.visit_bool(v),
            Value::I64(v) => visitor.visit_i64(v),
            Value::F64(v) => visitor.visit_f64(v),
            Value::String(ref v) => visitor.visit_str(&v),
            Value::Array(ref v) => v.visit(visitor),
            Value::Object(ref v) => v.visit(visitor),
        }
    }
}

struct WriterFormatter<'a, 'b: 'a> {
    inner: &'a mut fmt::Formatter<'b>,
}

impl<'a, 'b> io::Write for WriterFormatter<'a, 'b> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.inner.write_str(str::from_utf8(buf).unwrap()) {
            Ok(_) => Ok(buf.len()),
            Err(_) => Err(io::Error::last_os_error()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl fmt::Debug for Value {
    /// Serializes a json value into a string
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut wr = WriterFormatter { inner: f };
        super::ser::to_writer(&mut wr, self).map_err(|_| fmt::Error)
    }
}

enum State {
    Value(Value),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

pub struct Serializer {
    state: Vec<State>,
}

impl Serializer {
    pub fn new() -> Serializer {
        Serializer {
            state: Vec::with_capacity(4),
        }
    }

    pub fn unwrap(mut self) -> Value {
        match self.state.pop().unwrap() {
            State::Value(value) => value,
            _ => panic!(),
        }
    }
}

impl ser::Serializer for Serializer {
    type Value = ();
    type Error = ();

    #[inline]
    fn visit<
        T: ser::Serialize,
    >(&mut self, value: &T) -> Result<(), ()> {
        try!(value.visit(self));
        Ok(())
    }
}

impl ser::Visitor for Serializer {
    type Value = ();
    type Error = ();

    #[inline]
    fn visit_unit(&mut self) -> Result<(), ()> {
        self.state.push(State::Value(Value::Null));
        Ok(())
    }

    #[inline]
    fn visit_bool(&mut self, value: bool) -> Result<(), ()> {
        self.state.push(State::Value(Value::Bool(value)));
        Ok(())
    }

    #[inline]
    fn visit_i64(&mut self, value: i64) -> Result<(), ()> {
        self.state.push(State::Value(Value::I64(value)));
        Ok(())
    }

    #[inline]
    fn visit_u64(&mut self, value: u64) -> Result<(), ()> {
        self.state.push(State::Value(Value::I64(value as i64)));
        Ok(())
    }

    #[inline]
    fn visit_f64(&mut self, value: f64) -> Result<(), ()> {
        self.state.push(State::Value(Value::F64(value as f64)));
        Ok(())
    }

    #[inline]
    fn visit_char(&mut self, value: char) -> Result<(), ()> {
        self.state.push(State::Value(Value::String(value.to_string())));
        Ok(())
    }

    #[inline]
    fn visit_str(&mut self, value: &str) -> Result<(), ()> {
        self.state.push(State::Value(Value::String(value.to_string())));
        Ok(())
    }

    #[inline]
    fn visit_none(&mut self) -> Result<(), ()> {
        self.visit_unit()
    }

    #[inline]
    fn visit_some<
        V: ser::Serialize,
    >(&mut self, value: V) -> Result<(), ()> {
        value.visit(self)
    }

    #[inline]
    fn visit_seq<V>(&mut self, mut visitor: V) -> Result<(), ()>
        where V: ser::SeqVisitor,
    {
        let len = match visitor.size_hint() {
            (_, Some(len)) => len,
            (len, None) => len,
        };

        let values = Vec::with_capacity(len);

        self.state.push(State::Array(values));

        while let Some(()) = try!(visitor.visit(self)) { }

        match self.state.pop().unwrap() {
            State::Array(values) => {
                self.state.push(State::Value(Value::Array(values)));
            }
            _ => panic!(),
        }

        Ok(())
    }

    #[inline]
    fn visit_seq_elt<T>(&mut self, _first: bool, value: T) -> Result<(), ()>
        where T: ser::Serialize,
    {
        try!(value.visit(self));

        let value = match self.state.pop().unwrap() {
            State::Value(value) => value,
            _ => panic!(),
        };

        match *self.state.last_mut().unwrap() {
            State::Array(ref mut values) => { values.push(value); }
            _ => panic!(),
        }

        Ok(())
    }

    #[inline]
    fn visit_map<V>(&mut self, mut visitor: V) -> Result<(), ()>
        where V: ser::MapVisitor,
    {
        let values = BTreeMap::new();

        self.state.push(State::Object(values));

        while let Some(()) = try!(visitor.visit(self)) { }

        match self.state.pop().unwrap() {
            State::Object(values) => {
                self.state.push(State::Value(Value::Object(values)));
            }
            _ => panic!(),
        }

        Ok(())
    }

    #[inline]
    fn visit_map_elt<K, V>(&mut self, _first: bool, key: K, value: V) -> Result<(), ()>
        where K: ser::Serialize,
              V: ser::Serialize,
    {
        try!(key.visit(self));
        try!(value.visit(self));

        let key = match self.state.pop().unwrap() {
            State::Value(Value::String(value)) => value,
            _ => panic!(),
        };

        let value = match self.state.pop().unwrap() {
            State::Value(value) => value,
            _ => panic!(),
        };

        match *self.state.last_mut().unwrap() {
            State::Object(ref mut values) => { values.insert(key, value); }
            _ => panic!(),
        }

        Ok(())
    }
}

pub struct Deserializer {
    value: Option<Value>,
}

impl Deserializer {
    /// Creates a new deserializer instance for deserializing the specified JSON value.
    pub fn new(value: Value) -> Deserializer {
        Deserializer {
            value: Some(value),
        }
    }
}

impl de::Deserializer for Deserializer {
    type Error = Error;

    #[inline]
    fn visit<
        V: de::Visitor,
    >(&mut self, visitor: &mut V) -> Result<V::Value, Error> {
        let value = match self.value.take() {
            Some(value) => value,
            None => { return Err(de::Error::end_of_stream_error()); }
        };

        match value {
            Value::Null => visitor.visit_unit(),
            Value::Bool(v) => visitor.visit_bool(v),
            Value::I64(v) => visitor.visit_i64(v),
            Value::F64(v) => visitor.visit_f64(v),
            Value::String(v) => visitor.visit_string(v),
            Value::Array(v) => {
                let len = v.len();
                visitor.visit_seq(SeqDeserializer {
                    de: self,
                    iter: v.into_iter(),
                    len: len,
                })
            }
            Value::Object(v) => {
                let len = v.len();
                visitor.visit_map(MapDeserializer {
                    de: self,
                    iter: v.into_iter(),
                    value: None,
                    len: len,
                })
            }
        }
    }

    #[inline]
    fn visit_option<
        V: de::Visitor,
    >(&mut self, visitor: &mut V) -> Result<V::Value, Error> {
        match self.value {
            Some(Value::Null) => visitor.visit_none(),
            Some(_) => visitor.visit_some(self),
            None => Err(de::Error::end_of_stream_error()),
        }
    }
}

struct SeqDeserializer<'a> {
    de: &'a mut Deserializer,
    iter: vec::IntoIter<Value>,
    len: usize,
}

impl<'a> de::SeqVisitor for SeqDeserializer<'a> {
    type Error = Error;

    fn visit<T>(&mut self) -> Result<Option<T>, Error>
        where T: de::Deserialize
    {
        match self.iter.next() {
            Some(value) => {
                self.len -= 1;
                self.de.value = Some(value);
                Ok(Some(try!(de::Deserialize::deserialize(self.de))))
            }
            None => Ok(None),
        }
    }

    fn end(&mut self) -> Result<(), Error> {
        if self.len == 0 {
            Ok(())
        } else {
            Err(de::Error::end_of_stream_error())
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

struct MapDeserializer<'a> {
    de: &'a mut Deserializer,
    iter: btree_map::IntoIter<String, Value>,
    value: Option<Value>,
    len: usize,
}

impl<'a> de::MapVisitor for MapDeserializer<'a> {
    type Error = Error;

    fn visit_key<T>(&mut self) -> Result<Option<T>, Error>
        where T: de::Deserialize
    {
        match self.iter.next() {
            Some((key, value)) => {
                self.len -= 1;
                self.value = Some(value);
                self.de.value = Some(Value::String(key));
                Ok(Some(try!(de::Deserialize::deserialize(self.de))))
            }
            None => Ok(None),
        }
    }

    fn visit_value<T>(&mut self) -> Result<T, Error>
        where T: de::Deserialize
    {
        let value = self.value.take().unwrap();
        self.de.value = Some(value);
        Ok(try!(de::Deserialize::deserialize(self.de)))
    }

    fn end(&mut self) -> Result<(), Error> {
        if self.len == 0 {
            Ok(())
        } else {
            Err(de::Error::end_of_stream_error())
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

/// Shortcut function to encode a `T` into a JSON `Value`
pub fn to_value<T>(value: &T) -> Value
    where T: ser::Serialize
{
    let mut ser = Serializer::new();
    ser::Serializer::visit(&mut ser, value).ok().unwrap();
    ser.unwrap()
}

/// Shortcut function to decode a JSON `Value` into a `T`
pub fn from_value<T>(value: Value) -> Result<T, Error>
    where T: de::Deserialize
{
    let mut de = Deserializer::new(value);
    de::Deserialize::deserialize(&mut de)
}
