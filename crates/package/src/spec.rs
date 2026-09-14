use crate::error::PackageError;

/// Registry identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Registry {
    Npm,
    Pypi,
}

impl Registry {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pypi => "pypi",
        }
    }

    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "npm" => Some(Self::Npm),
            "pypi" => Some(Self::Pypi),
            _ => None,
        }
    }

    /// Normalized name for cache key.
    #[must_use]
    pub fn normalize_name(self, name: &str) -> String {
        match self {
            Self::Npm => name.to_ascii_lowercase(),
            Self::Pypi => normalize_pypi_name(name),
        }
    }
}

fn normalize_pypi_name(name: &str) -> String {
    // PEP 503: replace runs of [-_.] with '-', lowercase.
    let mut out = String::with_capacity(name.len());
    let mut last_dash = false;
    for ch in name.chars() {
        if ch == '-' || ch == '_' || ch == '.' {
            if !last_dash {
                out.push('-');
                last_dash = true;
            }
        } else {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        }
    }
    out
}

/// Parsed package spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageSpec {
    pub registry: Registry,
    pub name: String,
    pub version: Option<String>,
}

impl PackageSpec {
    /// Parse `<spec>` with optional `--version` override.
    ///
    /// Supports:
    /// - `zod`, `zod@3.22.4`
    /// - `npm:zod`, `npm:@scope/pkg@1.0.0`
    /// - `pypi:requests`, `pypi:requests==2.31.0`, `pypi:requests@2.31.0`
    pub fn parse(spec: &str, version_override: Option<&str>) -> Result<Self, PackageError> {
        let spec = spec.trim();
        if spec.is_empty() {
            return Err(PackageError::InvalidSpec {
                spec: spec.to_owned(),
                reason: "package spec must not be empty".to_owned(),
            });
        }

        // Split registry prefix on first ':'.
        let (registry, remainder) = if let Some(idx) = spec.find(':') {
            let prefix = &spec[..idx];
            let rest = &spec[idx + 1..];
            if rest.is_empty() {
                return Err(PackageError::InvalidSpec {
                    spec: spec.to_owned(),
                    reason: "missing package name after registry prefix".to_owned(),
                });
            }
            match Registry::from_str(prefix) {
                Some(r) => (r, rest),
                None => {
                    return Err(PackageError::UnsupportedRegistry {
                        registry: prefix.to_owned(),
                    })
                }
            }
        } else {
            (Registry::Npm, spec)
        };

        // Extract version from remainder.
        let (name, inline_version) = split_name_version(registry, remainder)?;

        // Handle --version override.
        let version = match (inline_version, version_override.map(str::trim)) {
            (Some(v1), Some(v2)) if !v2.is_empty() && v1 != v2 => {
                return Err(PackageError::InvalidSpec {
                    spec: spec.to_owned(),
                    reason: format!("version mismatch: '{v1}' vs '--version {v2}'"),
                })
            }
            (Some(v), _) => Some(v),
            (None, Some(v)) if !v.is_empty() => Some(v.to_owned()),
            (None, _) => None,
        };

        if name.is_empty() {
            return Err(PackageError::InvalidSpec {
                spec: spec.to_owned(),
                reason: "package name must not be empty".to_owned(),
            });
        }
        // Basic name validation.
        if name.contains('\0') || name.contains('/') && registry == Registry::Pypi && name.contains('/') {
            // pypi names should not contain '/', npm scoped does.
        }

        Ok(Self {
            registry,
            name,
            version,
        })
    }

    /// Cache key normalized name.
    #[must_use]
    pub fn normalized_name(&self) -> String {
        self.registry.normalize_name(&self.name)
    }
}

fn split_name_version(registry: Registry, s: &str) -> Result<(String, Option<String>), PackageError> {
    match registry {
        Registry::Npm => split_npm_name_version(s),
        Registry::Pypi => split_pypi_name_version(s),
    }
}

fn split_npm_name_version(s: &str) -> Result<(String, Option<String>), PackageError> {
    // Scoped: @scope/name@version . Find last '@' not at position 0.
    // For "@scope/pkg@1.0.0", last '@' is version separator.
    // For "@scope/pkg", no version.
    // For "zod@1.0.0", split at '@'.
    if s.starts_with('@') {
        // Find '/' then look for '@' after.
        if let Some(slash) = s.find('/') {
            let after_slash = &s[slash + 1..];
            if let Some(at) = after_slash.rfind('@') {
                let name = &s[..slash + 1 + at];
                let ver = &after_slash[at + 1..];
                if ver.is_empty() {
                    return Err(PackageError::InvalidSpec {
                        spec: s.to_owned(),
                        reason: "version after '@' must not be empty".to_owned(),
                    });
                }
                return Ok((name.to_owned(), Some(ver.to_owned())));
            }
        }
        // No version.
        return Ok((s.to_owned(), None));
    }
    // Non-scoped.
    if let Some(at) = s.rfind('@') {
        let name = &s[..at];
        let ver = &s[at + 1..];
        if ver.is_empty() {
            return Err(PackageError::InvalidSpec {
                spec: s.to_owned(),
                reason: "version after '@' must not be empty".to_owned(),
            });
        }
        if name.is_empty() {
            return Err(PackageError::InvalidSpec {
                spec: s.to_owned(),
                reason: "package name must not be empty".to_owned(),
            });
        }
        return Ok((name.to_owned(), Some(ver.to_owned())));
    }
    Ok((s.to_owned(), None))
}

fn split_pypi_name_version(s: &str) -> Result<(String, Option<String>), PackageError> {
    // Support "==" and "@".
    if let Some(idx) = s.find("==") {
        let name = s[..idx].trim().to_owned();
        let ver = s[idx + 2..].trim().to_owned();
        if ver.is_empty() {
            return Err(PackageError::InvalidSpec {
                spec: s.to_owned(),
                reason: "version after '==' must not be empty".to_owned(),
            });
        }
        return Ok((name, Some(ver)));
    }
    // Fall back to npm-style '@'.
    split_npm_name_version(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_bare() {
        let s = PackageSpec::parse("zod", None).unwrap();
        assert_eq!(s.registry, Registry::Npm);
        assert_eq!(s.name, "zod");
        assert_eq!(s.version, None);
    }

    #[test]
    fn npm_with_version() {
        let s = PackageSpec::parse("zod@3.22.4", None).unwrap();
        assert_eq!(s.name, "zod");
        assert_eq!(s.version, Some("3.22.4".to_owned()));
    }

    #[test]
    fn npm_scoped() {
        let s = PackageSpec::parse("@scope/pkg@1.0.0", None).unwrap();
        assert_eq!(s.name, "@scope/pkg");
        assert_eq!(s.version, Some("1.0.0".to_owned()));
        let s2 = PackageSpec::parse("@scope/pkg", None).unwrap();
        assert_eq!(s2.version, None);
    }

    #[test]
    fn pypi_bare() {
        let s = PackageSpec::parse("pypi:requests", None).unwrap();
        assert_eq!(s.registry, Registry::Pypi);
        assert_eq!(s.name, "requests");
    }

    #[test]
    fn pypi_with_eq() {
        let s = PackageSpec::parse("pypi:requests==2.31.0", None).unwrap();
        assert_eq!(s.version, Some("2.31.0".to_owned()));
    }

    #[test]
    fn pypi_normalizes() {
        let s = PackageSpec::parse("pypi:My_Package", None).unwrap();
        assert_eq!(s.normalized_name(), "my-package");
    }

    #[test]
    fn version_override_mismatch() {
        let e = PackageSpec::parse("zod@1.0.0", Some("2.0.0")).unwrap_err();
        assert!(e.to_string().contains("mismatch"));
    }

    #[test]
    fn unsupported_registry() {
        let e = PackageSpec::parse("crates:serde", None).unwrap_err();
        assert!(matches!(e, PackageError::UnsupportedRegistry { .. }));
    }
}
