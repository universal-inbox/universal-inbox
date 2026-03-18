use std::{fmt, marker::PhantomData, ops::Deref};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A generic type-checked wrapper around a generic identifier type.
/// Provides type safety for IDs that would otherwise be interchangeable.
/// This is a drop-in replacement for the `typed_id` crate.
pub struct TypedId<I, T>(pub I, PhantomData<T>);

impl<I, T> TypedId<I, T> {
    pub fn new(id: I) -> Self {
        Self(id, PhantomData)
    }

    pub fn convert<B: From<I>>(self) -> B {
        B::from(self.0)
    }
}

impl<I: Default, T> Default for TypedId<I, T> {
    fn default() -> Self {
        Self(Default::default(), Default::default())
    }
}

impl<I: fmt::Debug, T> fmt::Debug for TypedId<I, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("TypedId").field(&self.0).finish()
    }
}

impl<I: fmt::Display, T> fmt::Display for TypedId<I, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<I: Clone, T> Clone for TypedId<I, T> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), PhantomData)
    }
}

impl<I: Copy, T> Copy for TypedId<I, T> {}

impl<I: std::hash::Hash, T> std::hash::Hash for TypedId<I, T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

impl<I: PartialEq, T> PartialEq for TypedId<I, T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}

impl<I: Eq, T> Eq for TypedId<I, T> {}

impl<I: PartialOrd, T> PartialOrd for TypedId<I, T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl<I: Ord, T> Ord for TypedId<I, T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl<I, T> Deref for TypedId<I, T> {
    type Target = I;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<I, T> From<I> for TypedId<I, T> {
    fn from(other: I) -> TypedId<I, T> {
        TypedId(other, PhantomData)
    }
}

impl<'de, I: Deserialize<'de>, T> Deserialize<'de> for TypedId<I, T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        I::deserialize(deserializer).map(|id| id.into())
    }
}

impl<I: Serialize, T> Serialize for TypedId<I, T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}
