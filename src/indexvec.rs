pub trait VecIndex {
    fn get(self) -> usize;
    fn new(index: usize) -> Self;
}

#[derive(Clone, Debug)]
pub struct IndexVec<T, Index: VecIndex> {
    pub underlying: Vec<T>,
    marker: std::marker::PhantomData<Index>,
}

// This impl can not be derived.
impl<T, Index: VecIndex> Default for IndexVec<T, Index> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, Index: VecIndex> std::ops::Index<Index> for IndexVec<T, Index> {
    type Output = T;
    fn index(&self, index: Index) -> &T {
        &self.underlying[index.get()]
    }
}

impl<T, Index: VecIndex> std::ops::IndexMut<Index> for IndexVec<T, Index> {
    fn index_mut(&mut self, index: Index) -> &mut T {
        &mut self.underlying[index.get()]
    }
}

impl<T, Index: VecIndex> IndexVec<T, Index> {
    pub fn new() -> Self {
        Self { underlying: Vec::new(), marker: std::marker::PhantomData }
    }
    pub fn push(&mut self, element: T) -> Index {
        self.underlying.push(element);
        Index::new(self.underlying.len() - 1)
    }
    pub fn get(&self, index: Index) -> Option<&T> {
        self.underlying.get(index.get())
    }
    pub fn get_mut(&mut self, index: Index) -> Option<&mut T> {
        self.underlying.get_mut(index.get())
    }
    pub fn len(&self) -> usize {
        self.underlying.len()
    }
}

#[macro_export]
macro_rules! define_index {
    ($visibility:vis $name:ident as $type:ty) => {
        #[derive(Clone, Copy, PartialEq, Debug)]
        $visibility struct $name($type);
        impl $crate::indexvec::VecIndex for $name {
            fn get(self) -> usize { self.0 as usize }
            fn new(index: usize) -> Self { Self(index as $type) }
        }
    };
    ($visibility:vis $name:ident) => {
        $crate::define_index!($visibility $name as usize);
    };
}

#[cfg(test)]
mod tests {
    define_index!(MyIndex as u32);

    #[test]
    fn index_vec() {
        let mut vec = super::IndexVec::<String, MyIndex>::default();
        let a: MyIndex = vec.push("hello".to_string());
        let b: MyIndex = vec.push("world".to_string());
        assert_eq!(vec[a], "hello");
        assert_eq!(vec[b], "world");
    }
}
