//! Validation shared by scenarios and rulesets.

use std::collections::BTreeSet;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DefinitionError {
    #[error("invalid YAML: {0}")]
    Yaml(String),
    #[error("duplicate {kind} id `{id}`")]
    Duplicate { kind: &'static str, id: String },
    #[error("{context} refers to unknown {kind} `{id}`")]
    Unknown {
        context: String,
        kind: &'static str,
        id: String,
    },
    #[error("{0}")]
    Invalid(String),
}

/// Parse YAML whose enums are written as `{ variant: value }` maps instead of `!tags`.
pub(crate) fn from_yaml<T: serde::de::DeserializeOwned>(src: &str) -> Result<T, DefinitionError> {
    serde_yaml_ng::with::singleton_map_recursive::deserialize(
        serde_yaml_ng::Deserializer::from_str(src),
    )
    .map_err(|e| DefinitionError::Yaml(e.to_string()))
}

/// Ids must be unique within `ids`, non-empty, and free of the characters the engine uses to
/// build streak and random-stream keys.
pub(crate) fn unique<'a>(
    kind: &'static str,
    ids: impl Iterator<Item = &'a str>,
) -> Result<(), DefinitionError> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if id.is_empty() || id.contains(['@', ':', '#']) {
            return Err(DefinitionError::Invalid(format!(
                "{kind} id `{id}` must be non-empty and may not contain `@`, `:` or `#`"
            )));
        }
        if !seen.insert(id) {
            return Err(DefinitionError::Duplicate {
                kind,
                id: id.to_owned(),
            });
        }
    }
    Ok(())
}

pub(crate) fn unknown(ctx: &str, kind: &'static str, id: &str) -> DefinitionError {
    DefinitionError::Unknown {
        context: ctx.to_owned(),
        kind,
        id: id.to_owned(),
    }
}

pub(crate) fn invalid(msg: impl Into<String>) -> DefinitionError {
    DefinitionError::Invalid(msg.into())
}
