use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub trait SensitiveData: Send + Sync {
    fn sanitized(&self) -> String {
        "***".to_string()
    }

    fn sensitive_fields() -> Vec<&'static str> {
        vec![]
    }
}

pub struct Sensitive<T>(pub T);

impl<T: Clone> Clone for Sensitive<T> {
    fn clone(&self) -> Self {
        Sensitive(self.0.clone())
    }
}

impl<T> Sensitive<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }

    pub fn into_inner(self) -> T {
        self.0
    }

    pub fn inner(&self) -> &T {
        &self.0
    }
}

impl<T: SensitiveData> fmt::Debug for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sensitive({})", self.0.sanitized())
    }
}

impl<T: SensitiveData> fmt::Display for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.sanitized())
    }
}

impl<T: SensitiveData> Serialize for Sensitive<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.sanitized())
    }
}

impl<'de, T> Deserialize<'de> for Sensitive<T>
where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let value = s.parse::<T>().map_err(serde::de::Error::custom)?;
        Ok(Sensitive(value))
    }
}

impl<T: SensitiveData> SensitiveData for Sensitive<T> {
    fn sanitized(&self) -> String {
        self.0.sanitized()
    }
}

impl SensitiveData for str {
    fn sanitized(&self) -> String {
        "***".to_string()
    }
}

impl SensitiveData for String {
    fn sanitized(&self) -> String {
        "***".to_string()
    }
}

impl<T: SensitiveData> SensitiveData for Option<T> {
    fn sanitized(&self) -> String {
        match self {
            Some(v) => v.sanitized(),
            None => "None".to_string(),
        }
    }
}

impl<T: SensitiveData> SensitiveData for Vec<T> {
    fn sanitized(&self) -> String {
        format!("[{} items]", self.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensitive_string_sanitized() {
        let s = "secret-api-key".to_string();
        assert_eq!(s.sanitized(), "***");
    }

    #[test]
    fn test_sensitive_str_sanitized() {
        let s = "another-secret";
        assert_eq!(s.sanitized(), "***");
    }

    #[test]
    fn test_sensitive_new_and_inner() {
        let s = Sensitive::new("test-value".to_string());
        assert_eq!(s.inner(), "test-value");
    }

    #[test]
    fn test_sensitive_into_inner() {
        let s = Sensitive::new("real-api-key".to_string());
        assert_eq!(s.into_inner(), "real-api-key");
    }

    #[test]
    fn test_sensitive_debug() {
        let s = Sensitive("sk-proj-abc123def456".to_string());
        let debug = format!("{:?}", s);
        assert_eq!(debug, "Sensitive(***)");
    }

    #[test]
    fn test_sensitive_display() {
        let s = Sensitive("my-api-key-value".to_string());
        let display = format!("{}", s);
        assert_eq!(display, "***");
    }

    #[test]
    fn test_sensitive_serialize() {
        let s = Sensitive("secret-key-12345".to_string());
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"***\"");
    }

    #[test]
    fn test_option_some_sanitized() {
        let opt: Option<Sensitive<String>> =
            Some(Sensitive::new("secret".to_string()));
        assert_eq!(opt.sanitized(), "***");
    }

    #[test]
    fn test_option_none_sanitized() {
        let opt: Option<Sensitive<String>> = None;
        assert_eq!(opt.sanitized(), "None");
    }

    #[test]
    fn test_vec_sanitized() {
        let vec: Vec<Sensitive<String>> = vec![
            Sensitive::new("key1".to_string()),
            Sensitive::new("key2".to_string()),
        ];
        assert_eq!(vec.sanitized(), "[2 items]");
    }

    #[test]
    fn test_empty_vec_sanitized() {
        let vec: Vec<Sensitive<String>> = vec![];
        assert_eq!(vec.sanitized(), "[0 items]");
    }

    #[test]
    fn test_sensitive_fields_default() {
        struct TestStruct;
        impl SensitiveData for TestStruct {}
        assert_eq!(TestStruct::sensitive_fields(), Vec::<&'static str>::new());
    }

    #[test]
    fn test_sensitive_deserialize() {
        let json = "\"my-secret-key\"";
        let s: Sensitive<String> = serde_json::from_str(json).unwrap();
        assert_eq!(s.into_inner(), "my-secret-key");
    }
}
