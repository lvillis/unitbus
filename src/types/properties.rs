use std::collections::HashMap;

use zbus::zvariant::OwnedValue;

/// A `GetAll` property bag (values are stored internally as D-Bus variants).
///
/// This type intentionally does not expose any zbus/zvariant types in its public API.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct Properties {
    values: HashMap<String, OwnedValue>,
}

impl Properties {
    pub(crate) fn from_dbus(values: HashMap<String, OwnedValue>) -> Self {
        Self { values }
    }

    /// Returns true if the property exists.
    pub fn contains(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    /// List all property names.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.values.keys().map(String::as_str)
    }

    /// Number of properties in this bag.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns true if this property bag is empty.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Get a string property as `&str` (no allocation).
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.values.get(key).and_then(|v| <&str>::try_from(v).ok())
    }

    /// Get a string property as `&str`, mapping empty strings to `None`.
    pub fn get_opt_str(&self, key: &str) -> Option<&str> {
        let s = self.get_str(key)?;
        if s.is_empty() { None } else { Some(s) }
    }

    /// Get a string property as `String` (allocates).
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.get_str(key).map(|s| s.to_string())
    }

    /// Get a string property as `String`, mapping empty strings to `None`.
    pub fn get_opt_string(&self, key: &str) -> Option<String> {
        self.get_opt_str(key).map(|s| s.to_string())
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.values.get(key).and_then(|v| bool::try_from(v).ok())
    }

    pub fn get_u32(&self, key: &str) -> Option<u32> {
        self.values.get(key).and_then(|v| u32::try_from(v).ok())
    }

    pub fn get_u64(&self, key: &str) -> Option<u64> {
        self.values.get(key).and_then(|v| u64::try_from(v).ok())
    }

    pub fn get_i32(&self, key: &str) -> Option<i32> {
        self.values.get(key).and_then(|v| i32::try_from(v).ok())
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.values.get(key).and_then(|v| i64::try_from(v).ok())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    #![allow(clippy::panic)]
    #![allow(clippy::unwrap_used)]

    use super::*;
    use zbus::zvariant::Value;

    fn owned_str(s: &str) -> OwnedValue {
        OwnedValue::try_from(Value::from(s)).expect("owned string value")
    }

    #[test]
    fn string_getters_work_and_opt_maps_empty_to_none() {
        let mut m = HashMap::new();
        m.insert("A".to_string(), owned_str("hello"));
        m.insert("B".to_string(), owned_str(""));

        let p = Properties::from_dbus(m);
        assert_eq!(p.get_str("A"), Some("hello"));
        assert_eq!(p.get_string("A"), Some("hello".to_string()));

        assert_eq!(p.get_str("B"), Some(""));
        assert_eq!(p.get_opt_str("B"), None);
        assert_eq!(p.get_opt_string("B"), None);
    }

    #[test]
    fn numeric_getters_work() {
        let mut m = HashMap::new();
        m.insert("U32".to_string(), OwnedValue::from(7u32));
        m.insert("U64".to_string(), OwnedValue::from(9u64));
        m.insert("I32".to_string(), OwnedValue::from(-3i32));
        m.insert("I64".to_string(), OwnedValue::from(-5i64));
        m.insert("B".to_string(), OwnedValue::from(true));

        let p = Properties::from_dbus(m);
        assert_eq!(p.get_u32("U32"), Some(7));
        assert_eq!(p.get_u64("U64"), Some(9));
        assert_eq!(p.get_i32("I32"), Some(-3));
        assert_eq!(p.get_i64("I64"), Some(-5));
        assert_eq!(p.get_bool("B"), Some(true));
    }

    #[test]
    fn getters_return_none_on_type_mismatch() {
        let mut m = HashMap::new();
        m.insert("X".to_string(), OwnedValue::from(1u32));

        let p = Properties::from_dbus(m);
        assert_eq!(p.get_str("X"), None);
        assert_eq!(p.get_bool("X"), None);
    }
}
