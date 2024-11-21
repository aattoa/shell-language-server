use crate::lsp;

#[derive(Clone, Debug)]
pub struct Identifier {
    pub name: String,
    pub range: lsp::Range,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Expansion {
    Simple(Identifier),
}

#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    Symbol,
    Word(String),
    Expansion(Expansion),
    DoubleQuotedString(Vec<Expansion>),
}

#[derive(Clone, PartialEq, Debug)]
pub enum Statement {
    Command {
        name: Value,
        arguments: Vec<Value>,
    },
    FunctionDefinition {
        name: Identifier,
        body: Vec<Statement>,
    },
    ForLoop {
        variable: Identifier,
        values: Vec<Value>,
        body: Vec<Statement>,
    },
    WhileLoop {
        condition: Box<Statement>,
        body: Vec<Statement>,
    },
    Conditional {
        condition: Box<Statement>,
        true_branch: Vec<Statement>,
        false_branch: Option<Vec<Statement>>,
    },
}

impl PartialEq for Identifier {
    fn eq(&self, other: &Identifier) -> bool {
        self.name == other.name
    }
}
