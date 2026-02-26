use core::fmt;
use core::num::NonZeroU32;

/// Compact, stable identifier used across the engine graph.
///
/// - `u32` keeps memory small
/// - `NonZero` enables `Option<Id>` to be pointer-optimized
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id(NonZeroU32);

impl Id {
    /// Create an Id from a 0-based index by storing index+1.
    pub fn from_index(index: u32) -> Self {
        // index+1 must be nonzero
        Self(NonZeroU32::new(index + 1).expect("index+1 is nonzero"))
    }

    /// Recover the 0-based index.
    pub fn index(self) -> u32 {
        self.0.get() - 1
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", self.index())
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.index())
    }
}

/// Domain-specific ID aliases for clarity (no runtime cost).
pub type NodeId = Id;
pub type CompId = Id;
pub type PortId = Id;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_round_trip_index() {
        for i in [0_u32, 1, 2, 42, 10_000] {
            let id = Id::from_index(i);
            assert_eq!(id.index(), i);
        }
    }

    #[test]
    fn option_id_is_small() {
        // This is a classic reason for NonZero: Option<Id> can be same size as Id.
        assert_eq!(
            core::mem::size_of::<Id>(),
            core::mem::size_of::<Option<Id>>()
        );
    }
}
