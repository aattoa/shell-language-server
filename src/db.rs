#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Range {
    pub begin: Position,
    pub end: Position,
}

impl Position {
    pub fn advance(&mut self, char: char) {
        if char == '\n' {
            self.line += 1;
            self.column = 0;
        }
        else {
            self.column += 1;
        }
    }
}
