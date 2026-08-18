use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::Value;

use crate::{ContractError, MAX_RECORD_BYTES, Record, Result, SCHEMA_VERSION};

pub fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(value)?;
    reject_floats(&value)?;
    sort_objects(&mut value);
    let mut output = serde_json::to_vec(&value)?;
    output.push(b'\n');
    if output.len() > MAX_RECORD_BYTES {
        return Err(ContractError::RecordTooLarge);
    }
    Ok(output)
}

pub fn parse_record<T>(bytes: &[u8]) -> Result<T>
where
    T: Record + for<'de> Deserialize<'de>,
{
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(ContractError::RecordTooLarge);
    }
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Err(ContractError::InvalidValue("UTF-8 BOM is forbidden".into()));
    }
    let Some(payload) = bytes.strip_suffix(b"\n") else {
        return Err(ContractError::InvalidValue(
            "record must end with exactly one LF".into(),
        ));
    };
    if payload.ends_with(b"\n") {
        return Err(ContractError::InvalidValue(
            "record must end with exactly one LF".into(),
        ));
    }
    validate_lexical_json(payload)?;

    let mut deserializer = serde_json::Deserializer::from_slice(payload);
    let unique = UniqueValue::deserialize(&mut deserializer).map_err(map_json_error)?;
    deserializer.end().map_err(map_json_error)?;
    let value = unique.0;
    reject_floats(&value)?;

    let header: Header = serde_json::from_value(value.clone())?;
    if header.schema_version != SCHEMA_VERSION {
        return Err(ContractError::UnsupportedSchema(header.schema_version));
    }
    if header.record_kind != T::RECORD_KIND {
        return Err(ContractError::RecordKind {
            expected: T::RECORD_KIND,
            actual: header.record_kind,
        });
    }
    let record: T = serde_json::from_value(value)?;
    record.validate()?;
    Ok(record)
}

fn sort_objects(value: &mut Value) {
    match value {
        Value::Array(values) => values.iter_mut().for_each(sort_objects),
        Value::Object(values) => {
            let mut sorted = BTreeMap::new();
            for (key, mut value) in std::mem::take(values) {
                sort_objects(&mut value);
                sorted.insert(key, value);
            }
            values.extend(sorted);
        }
        _ => {}
    }
}

fn validate_lexical_json(bytes: &[u8]) -> Result<()> {
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                let start = index;
                index += 1;
                let mut escaped = false;
                while index < bytes.len() {
                    let byte = bytes[index];
                    index += 1;
                    if escaped {
                        escaped = false;
                    } else if byte == b'\\' {
                        escaped = true;
                    } else if byte == b'"' {
                        break;
                    }
                }
                if index > bytes.len() || bytes.get(index - 1) != Some(&b'"') {
                    return Err(ContractError::InvalidValue(
                        "unterminated JSON string".into(),
                    ));
                }
                let token = &bytes[start..index];
                let decoded: String = serde_json::from_slice(token)?;
                if serde_json::to_vec(&decoded)? != token {
                    return Err(ContractError::InvalidValue(
                        "record contains noncanonical string escaping".into(),
                    ));
                }
            }
            b'-' | b'0'..=b'9' => {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && matches!(bytes[index], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                {
                    index += 1;
                }
                let token = &bytes[start..index];
                let number: serde_json::Number = serde_json::from_slice(token)?;
                if number.is_f64() {
                    return Err(ContractError::FloatingPoint);
                }
                if number.to_string().as_bytes() != token {
                    return Err(ContractError::InvalidValue(
                        "record contains a noncanonical integer".into(),
                    ));
                }
            }
            byte if byte.is_ascii_whitespace() => {
                return Err(ContractError::InvalidValue(
                    "record contains framing whitespace".into(),
                ));
            }
            _ => index += 1,
        }
    }
    Ok(())
}

fn map_json_error(error: serde_json::Error) -> ContractError {
    let message = error.to_string();
    if let Some(rest) = message.strip_prefix("duplicate object key: ") {
        let key = rest.split(" at line").next().unwrap_or(rest);
        ContractError::DuplicateKey(key.to_owned())
    } else {
        ContractError::InvalidJson(error)
    }
}

fn reject_floats(value: &Value) -> Result<()> {
    match value {
        Value::Number(number) if number.is_f64() => Err(ContractError::FloatingPoint),
        Value::Array(values) => values.iter().try_for_each(reject_floats),
        Value::Object(values) => values.values().try_for_each(reject_floats),
        _ => Ok(()),
    }
}

#[derive(Deserialize)]
struct Header {
    schema_version: u32,
    record_kind: String,
}

struct UniqueValue(Value);

impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(UniqueValueVisitor)
    }
}

struct UniqueValueVisitor;

impl<'de> de::Visitor<'de> for UniqueValueVisitor {
    type Value = UniqueValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value without duplicate object keys")
    }

    fn visit_bool<E>(self, value: bool) -> std::result::Result<Self::Value, E> {
        Ok(UniqueValue(Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> std::result::Result<Self::Value, E> {
        Ok(UniqueValue(Value::Number(value.into())))
    }

    fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E> {
        Ok(UniqueValue(Value::Number(value.into())))
    }

    fn visit_f64<E>(self, value: f64) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        let number = serde_json::Number::from_f64(value)
            .ok_or_else(|| E::custom("non-finite JSON number"))?;
        Ok(UniqueValue(Value::Number(number)))
    }

    fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_string(value.to_owned())
    }

    fn visit_string<E>(self, value: String) -> std::result::Result<Self::Value, E> {
        Ok(UniqueValue(Value::String(value)))
    }

    fn visit_none<E>(self) -> std::result::Result<Self::Value, E> {
        Ok(UniqueValue(Value::Null))
    }

    fn visit_unit<E>(self) -> std::result::Result<Self::Value, E> {
        Ok(UniqueValue(Value::Null))
    }

    fn visit_some<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        UniqueValue::deserialize(deserializer)
    }

    fn visit_seq<A>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element::<UniqueValue>()? {
            values.push(value.0);
        }
        Ok(UniqueValue(Value::Array(values)))
    }

    fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: de::MapAccess<'de>,
    {
        let mut values = BTreeMap::new();
        while let Some((key, value)) = map.next_entry::<String, UniqueValue>()? {
            if values.insert(key.clone(), value.0).is_some() {
                return Err(de::Error::custom(format!("duplicate object key: {key}")));
            }
        }
        Ok(UniqueValue(Value::Object(values.into_iter().collect())))
    }
}
