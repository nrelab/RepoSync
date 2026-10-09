//! Global utility functions for transformations.

use crate::Error;

/// Available global functions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalFn {
    /// Absolute value of a number.
    Abs { value: i64 },
    /// Return True if all elements are truthy.
    All { values: Vec<String> },
    /// Return True if any element is truthy.
    Any { values: Vec<String> },
    /// Convert to boolean.
    Bool { value: String },
    /// Create a dictionary.
    Dict { pairs: Vec<(String, String)> },
    /// List attributes of an object (placeholder).
    Dir { obj: String },
    /// Enumerate elements with indices.
    Enumerate { values: Vec<String> },
    /// Fail with an error message.
    Fail { message: String },
    /// Convert to float.
    Float { value: String },
    /// Get attribute of an object (placeholder).
    Getattr { obj: String, attr: String },
    /// Glob pattern matching.
    Glob { pattern: String },
    /// Check if object has attribute (placeholder).
    Hasattr { obj: String, attr: String },
    /// Hash value of an object.
    Hash { value: String },
    /// Convert to integer.
    Int { value: String },
    /// Length of a collection.
    Len { value: String },
    /// Create a list.
    List { values: Vec<String> },
    /// Maximum value.
    Max { values: Vec<String> },
    /// Minimum value.
    Min { values: Vec<String> },
    /// Create a new author signature.
    NewAuthor {
        name: String,
        email: String,
        time: Option<i64>,
    },
    /// Parse a commit message.
    ParseMessage { message: String },
    /// Print a value (debug output).
    Print { value: String },
    /// Generate a range of numbers.
    Range { start: i64, end: i64, step: Option<i64> },
    /// String representation.
    Repr { value: String },
    /// Reverse an iterable.
    Reversed { values: Vec<String> },
    /// Create a set.
    Set { values: Vec<String> },
    /// Sort values.
    Sorted { values: Vec<String> },
    /// Convert to string.
    Str { value: String },
    /// Create a tuple.
    Tuple { values: Vec<String> },
    /// Type of an object.
    Type { value: String },
    /// Zip multiple iterables.
    Zip { iterables: Vec<Vec<String>> },
}

impl GlobalFn {
    /// Execute the global function and return the result as a string.
    ///
    /// # Errors
    /// Returns an error if the function fails (e.g., parsing errors, invalid operations).
    ///
    /// # Panics
    /// Panics if system time is before UNIX epoch (extremely unlikely).
    #[allow(clippy::too_many_lines, clippy::cast_possible_wrap, clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::implicit_clone)]
    pub fn execute(&self) -> Result<String, Error> {
        match self {
            GlobalFn::Abs { value } => Ok(value.abs().to_string()),
            GlobalFn::All { values } => {
                let result = values.iter().all(|v| !v.is_empty() && v != "0" && v != "false");
                Ok(result.to_string())
            }
            GlobalFn::Any { values } => {
                let result = values.iter().any(|v| !v.is_empty() && v != "0" && v != "false");
                Ok(result.to_string())
            }
            GlobalFn::Bool { value } => {
                let result = !value.is_empty() && value != "0" && value != "false";
                Ok(result.to_string())
            }
            GlobalFn::Dict { pairs } => {
                let result: String = pairs
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                Ok(format!("{{{result}}}"))
            }
            GlobalFn::Dir { obj } => Ok(format!("[attributes of {obj}]")),
            GlobalFn::Enumerate { values } => {
                let result: String = values
                    .iter()
                    .enumerate()
                    .map(|(i, v)| format!("{i}: {v}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(result)
            }
            GlobalFn::Fail { message } => Err(Error::Transform {
                name: "global".to_string(),
                message: message.clone(),
            }),
            GlobalFn::Float { value } => {
                value
                    .parse::<f64>()
                    .map(|f| f.to_string())
                    .map_err(|_| Error::Transform {
                        name: "global".to_string(),
                        message: format!("cannot parse '{value}' as float"),
                    })
            }
            GlobalFn::Getattr { obj, attr } => Ok(format!("{obj}.{attr}")),
            GlobalFn::Glob { pattern } => {
                // Return the pattern itself - actual glob matching is context-dependent
                Ok(pattern.clone())
            }
            GlobalFn::Hasattr { obj: _, attr: _ } => Ok("true".to_string()), // Placeholder
            GlobalFn::Hash { value } => {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                value.hash(&mut hasher);
                Ok(hasher.finish().to_string())
            }
            GlobalFn::Int { value } => {
                value
                    .parse::<i64>()
                    .map(|i| i.to_string())
                    .map_err(|_| Error::Transform {
                        name: "global".to_string(),
                        message: format!("cannot parse '{value}' as integer"),
                    })
            }
            GlobalFn::Len { value } => Ok(value.len().to_string()),
            GlobalFn::List { values } => Ok(format!("[{}]", values.join(", "))),
            GlobalFn::Max { values } => {
                values
                    .iter()
                    .max()
                    .ok_or_else(|| Error::Transform {
                        name: "global".to_string(),
                        message: "max() arg is an empty sequence".to_string(),
                    })
                    .cloned()
            }
            GlobalFn::Min { values } => {
                values
                    .iter()
                    .min()
                    .ok_or_else(|| Error::Transform {
                        name: "global".to_string(),
                        message: "min() arg is an empty sequence".to_string(),
                    })
                    .cloned()
            }
            GlobalFn::NewAuthor { name, email, time } => {
                let timestamp = time.unwrap_or_else(|| {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64
                });
                Ok(format!("{name} <{email}> @ {timestamp}"))
            }
            GlobalFn::ParseMessage { message } => {
                // Simple parsing: split into subject and body
                let parts: Vec<&str> = message.splitn(2, '\n').collect();
                let subject = parts.first().unwrap_or(&"");
                let body = parts.get(1).unwrap_or(&"");
                Ok(format!("Subject: {subject}\nBody: {body}"))
            }
            GlobalFn::Print { value } => {
                println!("{value}");
                Ok(value.clone())
            }
            GlobalFn::Range { start, end, step } => {
                let step_val = step.unwrap_or(1);
                let result: Vec<String> = if step_val > 0 {
                    (*start..*end).step_by(step_val as usize).map(|i| i.to_string()).collect()
                } else {
                    (*end..=*start).rev().step_by((-step_val) as usize).map(|i| i.to_string()).collect()
                };
                Ok(result.join("\n"))
            }
            GlobalFn::Repr { value } => Ok(format!("{value:?}")),
            GlobalFn::Reversed { values } => {
                let result: Vec<String> = values.iter().rev().cloned().collect();
                Ok(result.join("\n"))
            }
            GlobalFn::Set { values } => {
                let unique: Vec<String> = values.iter().cloned().collect::<std::collections::HashSet<_>>().into_iter().collect();
                Ok(format!("{{{}}}", unique.join(", ")))
            }
            GlobalFn::Sorted { values } => {
                let mut sorted = values.clone();
                sorted.sort();
                Ok(sorted.join("\n"))
            }
            GlobalFn::Str { value } => Ok(value.clone()),
            GlobalFn::Tuple { values } => Ok(format!("({})", values.join(", "))),
            GlobalFn::Type { value } => {
                let inferred_type = if value.parse::<i64>().is_ok() {
                    "int"
                } else if value.parse::<f64>().is_ok() {
                    "float"
                } else if value == "true" || value == "false" {
                    "bool"
                } else {
                    "str"
                };
                Ok(inferred_type.to_string())
            }
            GlobalFn::Zip { iterables } => {
                if iterables.is_empty() {
                    return Ok(String::new());
                }
                let min_len = iterables.iter().map(std::vec::Vec::len).min().unwrap_or(0);
                let result: Vec<String> = (0..min_len)
                    .map(|i| {
                        iterables
                            .iter()
                            .map(|v| v.get(i).cloned().unwrap_or_default())
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .collect();
                Ok(result.join("\n"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abs() {
        let result = GlobalFn::Abs { value: -5 }.execute().unwrap();
        assert_eq!(result, "5");
    }

    #[test]
    fn test_all() {
        let result = GlobalFn::All {
            values: vec!["true".to_string(), "1".to_string(), "hello".to_string()],
        }
        .execute()
        .unwrap();
        assert_eq!(result, "true");
    }

    #[test]
    fn test_any() {
        let result = GlobalFn::Any {
            values: vec!["false".to_string(), "0".to_string(), "hello".to_string()],
        }
        .execute()
        .unwrap();
        assert_eq!(result, "true");
    }

    #[test]
    fn test_bool() {
        let result = GlobalFn::Bool {
            value: "true".to_string(),
        }
        .execute()
        .unwrap();
        assert_eq!(result, "true");
    }

    #[test]
    fn test_dict() {
        let result = GlobalFn::Dict {
            pairs: vec![("key1".to_string(), "value1".to_string())],
        }
        .execute()
        .unwrap();
        assert_eq!(result, "{key1: value1}");
    }

    #[test]
    fn test_enumerate() {
        let result = GlobalFn::Enumerate {
            values: vec!["a".to_string(), "b".to_string()],
        }
        .execute()
        .unwrap();
        assert_eq!(result, "0: a\n1: b");
    }

    #[test]
    fn test_float() {
        let result = GlobalFn::Float {
            value: "3.14".to_string(),
        }
        .execute()
        .unwrap();
        assert_eq!(result, "3.14");
    }

    #[test]
    fn test_int() {
        let result = GlobalFn::Int {
            value: "42".to_string(),
        }
        .execute()
        .unwrap();
        assert_eq!(result, "42");
    }

    #[test]
    fn test_len() {
        let result = GlobalFn::Len {
            value: "hello".to_string(),
        }
        .execute()
        .unwrap();
        assert_eq!(result, "5");
    }

    #[test]
    fn test_list() {
        let result = GlobalFn::List {
            values: vec!["a".to_string(), "b".to_string()],
        }
        .execute()
        .unwrap();
        assert_eq!(result, "[a, b]");
    }

    #[test]
    fn test_max() {
        let result = GlobalFn::Max {
            values: vec!["1".to_string(), "3".to_string(), "2".to_string()],
        }
        .execute()
        .unwrap();
        assert_eq!(result, "3");
    }

    #[test]
    fn test_min() {
        let result = GlobalFn::Min {
            values: vec!["1".to_string(), "3".to_string(), "2".to_string()],
        }
        .execute()
        .unwrap();
        assert_eq!(result, "1");
    }

    #[test]
    fn test_new_author() {
        let result = GlobalFn::NewAuthor {
            name: "John Doe".to_string(),
            email: "john@example.com".to_string(),
            time: Some(1_234_567_890),
        }
        .execute()
        .unwrap();
        assert!(result.contains("John Doe"));
        assert!(result.contains("john@example.com"));
        assert!(result.contains("1234567890"));
    }

    #[test]
    fn test_parse_message() {
        let result = GlobalFn::ParseMessage {
            message: "Subject\nBody line".to_string(),
        }
        .execute()
        .unwrap();
        assert!(result.contains("Subject: Subject"));
        assert!(result.contains("Body: Body line"));
    }

    #[test]
    fn test_range() {
        let result = GlobalFn::Range {
            start: 0,
            end: 3,
            step: None,
        }
        .execute()
        .unwrap();
        assert_eq!(result, "0\n1\n2");
    }

    #[test]
    fn test_repr() {
        let result = GlobalFn::Repr {
            value: "test".to_string(),
        }
        .execute()
        .unwrap();
        assert!(result.contains("\"test\""));
    }

    #[test]
    fn test_reversed() {
        let result = GlobalFn::Reversed {
            values: vec!["a".to_string(), "b".to_string(), "c".to_string()],
        }
        .execute()
        .unwrap();
        assert_eq!(result, "c\nb\na");
    }

    #[test]
    fn test_set() {
        let result = GlobalFn::Set {
            values: vec!["a".to_string(), "b".to_string(), "a".to_string()],
        }
        .execute()
        .unwrap();
        // Set should deduplicate
        assert!(result.contains('a'));
        assert!(result.contains('b'));
    }

    #[test]
    fn test_sorted() {
        let result = GlobalFn::Sorted {
            values: vec!["c".to_string(), "a".to_string(), "b".to_string()],
        }
        .execute()
        .unwrap();
        assert_eq!(result, "a\nb\nc");
    }

    #[test]
    fn test_tuple() {
        let result = GlobalFn::Tuple {
            values: vec!["a".to_string(), "b".to_string()],
        }
        .execute()
        .unwrap();
        assert_eq!(result, "(a, b)");
    }

    #[test]
    fn test_type() {
        let result = GlobalFn::Type {
            value: "42".to_string(),
        }
        .execute()
        .unwrap();
        assert_eq!(result, "int");
    }

    #[test]
    fn test_zip() {
        let result = GlobalFn::Zip {
            iterables: vec![
                vec!["a".to_string(), "b".to_string()],
                vec!["1".to_string(), "2".to_string()],
            ],
        }
        .execute()
        .unwrap();
        assert_eq!(result, "a,1\nb,2");
    }

    #[test]
    fn test_fail() {
        let result = GlobalFn::Fail {
            message: "test error".to_string(),
        }
        .execute();
        assert!(result.is_err());
    }
}
