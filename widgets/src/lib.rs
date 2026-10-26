//! Luma's store widgets as data (docs/widgets.md): manifests, the documents their transforms
//! give, the catalog's trust checks, request chains and the jq runner.
pub mod document;
pub mod jq;
pub mod manifest;
pub mod sandbox;

/// Why a manifest, document, catalog or transform was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}
