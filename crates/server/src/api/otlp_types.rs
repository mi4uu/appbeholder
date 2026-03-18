use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;

/// OTLP Resource with attributes
#[derive(Debug, Deserialize)]
pub struct Resource {
    #[serde(default)]
    pub attributes: Vec<KeyValue>,
}

impl Resource {
    /// Find an attribute by key
    pub fn get_attribute(&self, key: &str) -> Option<String> {
        self.attributes
            .iter()
            .find(|kv| kv.key == key)
            .and_then(|kv| kv.value.as_string())
    }

    /// Extract service.name or default to "unknown"
    pub fn service_name(&self) -> String {
        self.get_attribute("service.name")
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Extract host.name or default to "unknown"
    pub fn host_name(&self) -> String {
        self.get_attribute("host.name")
            .unwrap_or_else(|| "unknown".to_string())
    }
}

/// OTLP KeyValue pair
#[derive(Debug, Deserialize)]
pub struct KeyValue {
    pub key: String,
    pub value: AnyValue,
}

/// OTLP AnyValue — supports multiple value types
#[derive(Debug, Default, Deserialize)]
pub struct AnyValue {
    #[serde(rename = "stringValue", default)]
    pub string_value: Option<String>,
    /// IMPORTANT: OTLP JSON encodes int64 as STRING
    #[serde(rename = "intValue", default)]
    pub int_value: Option<String>,
    #[serde(rename = "doubleValue", default)]
    pub double_value: Option<f64>,
    #[serde(rename = "boolValue", default)]
    pub bool_value: Option<bool>,
}

impl AnyValue {
    /// Convert to string representation (used by Resource::get_attribute)
    pub fn as_string(&self) -> Option<String> {
        if let Some(ref s) = self.string_value {
            Some(s.clone())
        } else if let Some(ref i) = self.int_value {
            Some(i.clone())
        } else if let Some(d) = self.double_value {
            Some(d.to_string())
        } else if let Some(b) = self.bool_value {
            Some(b.to_string())
        } else {
            None
        }
    }

    /// Convert to JSON value, parsing int_value string to i64
    pub fn to_json(&self) -> serde_json::Value {
        if let Some(ref s) = self.string_value {
            json!(s)
        } else if let Some(ref i) = self.int_value {
            // Parse int_value string to i64
            match i.parse::<i64>() {
                Ok(parsed) => json!(parsed),
                Err(_) => json!(i),
            }
        } else if let Some(d) = self.double_value {
            json!(d)
        } else if let Some(b) = self.bool_value {
            json!(b)
        } else {
            json!(null)
        }
    }
}

/// OTLP InstrumentationScope
#[derive(Debug, Deserialize)]
pub struct InstrumentationScope {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

/// Convert KeyValue list to JSON object
pub fn attributes_to_json(attrs: &[KeyValue]) -> serde_json::Value {
    let obj: serde_json::Map<String, serde_json::Value> = attrs
        .iter()
        .map(|kv| (kv.key.clone(), kv.value.to_json()))
        .collect();
    json!(obj)
}

/// Convert nanosecond unix timestamp string to chrono DateTime
pub fn nanos_to_datetime(nanos_str: &str) -> DateTime<Utc> {
    // Parse string as u64
    let nanos = match nanos_str.parse::<u64>() {
        Ok(n) => n,
        Err(_) => return Utc::now(), // Fallback on parse error
    };

    // Split into seconds and nanoseconds
    let secs = (nanos / 1_000_000_000) as i64;
    let nsecs = (nanos % 1_000_000_000) as u32;

    // Create DateTime from timestamp
    DateTime::from_timestamp(secs, nsecs).unwrap_or_else(Utc::now)
}
