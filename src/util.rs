#[macro_export]
macro_rules! assert_let {
    ($lhs:pat = $rhs:expr) => {
        let $lhs = $rhs
        else {
            panic!("assert_let failed, with rhs = {:?}", $rhs);
        };
    };
}

#[derive(Clone, Copy, Debug)]
pub struct View {
    pub start: u32,
    pub end: u32,
}

impl View {
    pub fn string(self, str: &str) -> &str {
        &str[(self.start as usize)..(self.end as usize)]
    }
}
