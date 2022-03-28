#![feature(generic_associated_types)]

use std::{
    cell::RefCell,
    marker::PhantomData,
    sync::mpsc::{sync_channel, SyncSender},
};

use anyhow::{anyhow, Result};

#[derive(Clone, Debug)]
enum Index {
    Array(usize),
    Map(String),
}
type Path = Vec<Index>;

#[derive(Debug)]
enum PrimitiveValue {
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    EmptyArray,
    EmptyObject,
}

struct PathValue {
    path: Path,
    value: Option<PrimitiveValue>,
}
impl PathValue {
    fn print(&self) {
        print!("[");
        print!("[");
        for (i, v) in self.path.iter().enumerate() {
            if i != 0 {
                print!(",");
            }
            match v {
                Index::Array(i) => print!("{i}"),
                Index::Map(s) => print!("{s:?}"),
            }
        }
        print!("]");
        if let Some(value) = &self.value {
            print!(",");
            match value {
                PrimitiveValue::Null => print!("null"),
                PrimitiveValue::Boolean(v) => print!("{v}"),
                PrimitiveValue::Number(v) => print!("{v}"),
                PrimitiveValue::String(v) => print!("{v:?}"),
                PrimitiveValue::EmptyArray => print!("[]"),
                PrimitiveValue::EmptyObject => print!("{{}}"),
            }
        }
        print!("]");
        println!();
    }
}

struct StreamState<'a> {
    sender: SyncSender<Result<PathValue>>,
    path: &'a mut Path,
}

impl<'a> StreamState<'a> {
    fn emit_value(&mut self, value: PrimitiveValue) {
        self.sender
            .send(Ok(PathValue {
                path: self.path.clone(),
                value: Some(value),
            }))
            .ok(); // Discarding err since this indicates that the recever has already been dropped so they should already know what to do.
    }

    fn emit_close(&self) {
        self.sender
            .send(Ok(PathValue {
                path: self.path.clone(),
                value: None,
            }))
            .ok(); // Discarding err since this indicates that the recever has already been dropped so they should already know what to do.
    }
}

impl<'de, 'a> serde::de::Visitor<'de> for &mut StreamState<'a> {
    type Value = ();

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            formatter,
            "null, boolean, number, string, array, or map keyed with string"
        )
    }

    fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.emit_value(PrimitiveValue::Boolean(v));
        Ok(())
    }

    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.emit_value(PrimitiveValue::Number(v as f64));
        Ok(())
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.emit_value(PrimitiveValue::Number(v as f64));
        Ok(())
    }

    fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.emit_value(PrimitiveValue::Number(v as f64));
        Ok(())
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_string(v.into())
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.emit_value(PrimitiveValue::String(v));
        Ok(())
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.emit_value(PrimitiveValue::Null);
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_none()
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut i = 0;
        self.path.push(Index::Array(i));
        while seq.next_element_seed(&mut *self)?.is_some() {
            self.path.pop();
            i += 1;
            self.path.push(Index::Array(i));
        }
        self.path.pop();
        if i == 0 {
            self.emit_value(PrimitiveValue::EmptyArray);
        } else {
            i -= 1;
            self.path.push(Index::Array(i));
            self.emit_close();
            self.path.pop();
        }
        Ok(())
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        struct Str;
        impl<'de> serde::de::DeserializeSeed<'de> for Str {
            type Value = String;

            fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct V;
                impl<'de> serde::de::Visitor<'de> for V {
                    type Value = String;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                        write!(formatter, "string as the key of a map")
                    }

                    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
                    where
                        E: serde::de::Error,
                    {
                        Ok(v.into())
                    }

                    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
                    where
                        E: serde::de::Error,
                    {
                        Ok(v)
                    }
                }
                deserializer.deserialize_any(V)
            }
        }

        let mut empty = true;
        self.path.push(Index::Map("".into()));
        while let Some(key) = map.next_key_seed(Str)? {
            empty = false;
            self.path.pop();
            self.path.push(Index::Map(key));
            map.next_value_seed(&mut *self)?;
        }
        if empty {
            self.path.pop();
            self.emit_value(PrimitiveValue::EmptyObject);
        } else {
            self.emit_close();
            self.path.pop();
        }
        Ok(())
    }
}

impl<'de, 'a> serde::de::DeserializeSeed<'de> for &mut StreamState<'a> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(self)?;
        Ok(())
    }
}

trait MultiDocDeserializer<'de> {
    type Error: std::error::Error;
    type Iterator<T: serde::Deserialize<'de>>: Iterator<Item = Result<T, Self::Error>>;
    fn into_multidoc_iter<T: serde::Deserialize<'de>>(self) -> Self::Iterator<T>;
}

impl<'de, R: serde_json::de::Read<'de>> MultiDocDeserializer<'de>
    for serde_json::de::Deserializer<R>
{
    type Error = serde_json::Error;
    type Iterator<T: serde::Deserialize<'de>> = serde_json::de::StreamDeserializer<'de, R, T>;

    fn into_multidoc_iter<T: serde::Deserialize<'de>>(self) -> Self::Iterator<T> {
        self.into_iter()
    }
}

struct SerdeYamlMultiDocIter<'de, T> {
    inner: serde_yaml::Deserializer<'de>,
    _phantom: PhantomData<T>,
}
impl<'de, T: serde::Deserialize<'de>> Iterator for SerdeYamlMultiDocIter<'de, T> {
    type Item = Result<T, serde_yaml::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|de| T::deserialize(de))
    }
}
impl<'de> MultiDocDeserializer<'de> for serde_yaml::Deserializer<'de> {
    type Error = serde_yaml::Error;
    type Iterator<T: serde::Deserialize<'de>> = SerdeYamlMultiDocIter<'de, T>;

    fn into_multidoc_iter<T: serde::Deserialize<'de>>(self) -> Self::Iterator<T> {
        SerdeYamlMultiDocIter {
            inner: self,
            _phantom: PhantomData,
        }
    }
}

trait FromReader {
    type De<'de, R>: MultiDocDeserializer<'de>
    where
        R: 'de + std::io::Read;
    fn from_reader<'de, R: 'de + std::io::Read>(read: R) -> Self::De<'de, R>;
}
struct Yaml;
impl FromReader for Yaml {
    type De<'de, R> = serde_yaml::Deserializer<'de> where R: 'de + std::io::Read;

    fn from_reader<'de, R: 'de + std::io::Read>(read: R) -> Self::De<'de, R> {
        serde_yaml::Deserializer::from_reader(read)
    }
}
struct Json;
impl FromReader for Json {
    type De<'de, R> = serde_json::Deserializer<serde_json::de::IoRead<R>> where R: 'de + std::io::Read;

    fn from_reader<'de, R: 'de + std::io::Read>(read: R) -> Self::De<'de, R> {
        serde_json::Deserializer::from_reader(read)
    }
}

fn main_generic<T: FromReader>() -> impl Iterator<Item = Result<PathValue>> {
    let (sender, receiver) = sync_channel(1);
    std::thread::spawn(|| {
        thread_local! {
            static SENDER: RefCell<Option<SyncSender<Result<PathValue>>>> = RefCell::new(None);
        }
        SENDER.with(|snd| snd.borrow_mut().replace(sender));
        struct Stream;
        impl<'de> serde::Deserialize<'de> for Stream {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let mut path = vec![];
                let sender = SENDER.with(|snd| snd.borrow().as_ref().unwrap().clone());
                let mut visitor = StreamState {
                    sender,
                    path: &mut path,
                };
                deserializer.deserialize_any(&mut visitor)?;
                Ok(Self)
            }
        }
        let de = T::from_reader(std::io::stdin().lock());
        for v in de.into_multidoc_iter::<Stream>() {
            if let Err(e) = v {
                SENDER
                    .with(|snd| {
                        snd.borrow_mut()
                            .as_ref()
                            .unwrap()
                            .send(Err(anyhow!("Deserialization error: {e}")))
                    })
                    .ok();
                break;
            }
        }
    });
    receiver.into_iter()
}

fn main() -> Result<()> {
    for v in main_generic::<Json>() {
        match v {
            Ok(v) => v.print(),
            Err(e) => eprintln!("{e}"),
        }
    }
    Ok(())
}
