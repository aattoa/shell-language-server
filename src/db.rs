use crate::lsp;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct Document {
    pub text: String,
    pub diagnostics: Vec<lsp::Diagnostic>,
}

#[derive(Default)]
pub struct Database {
    pub documents: HashMap<PathBuf, Document>,
}
