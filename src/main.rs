use anyhow::Result;

#[derive(Debug)]
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
}

struct StreamState<'a> {
    path: &'a mut Path,
}

impl<'a> StreamState<'a> {
    fn emit_path(&self) {
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
    }

    fn emit_value(&mut self, value: PrimitiveValue) {
        print!("[");
        self.emit_path();
        print!(",");
        match value {
            PrimitiveValue::Null => print!("null"),
            PrimitiveValue::Boolean(v) => print!("{v}"),
            PrimitiveValue::Number(v) => print!("{v}"),
            PrimitiveValue::String(v) => print!("{v:?}"),
        }
        print!("]");
        println!("");
    }

    fn emit_close(&self) {
        print!("[");
        self.emit_path();
        print!("]");
        println!("");
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
        while let Some(_) = seq.next_element_seed(&mut *self)? {
            self.path.pop();
            i += 1;
            self.path.push(Index::Array(i));
        }
        self.path.pop();
        i -= 1;
        self.path.push(Index::Array(i));
        self.emit_close();
        self.path.pop();
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

        self.path.push(Index::Map("".into()));
        while let Some(key) = map.next_key_seed(Str)? {
            self.path.pop();
            self.path.push(Index::Map(key));
            map.next_value_seed(&mut *self)?;
        }
        self.emit_close();
        self.path.pop();
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

struct Stream;
impl<'de> serde::Deserialize<'de> for Stream {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut path = vec![];
        let mut visitor = StreamState { path: &mut path };
        deserializer.deserialize_any(&mut visitor)?;
        Ok(Self)
    }
}

fn main() -> Result<()> {
    let stdin = std::io::stdin();
    for v in serde_json::de::Deserializer::from_reader(stdin).into_iter::<Stream>() {
        if let Err(e) = v {
            eprintln!("{:?}", e);
        }
    }
    Ok(())
}
