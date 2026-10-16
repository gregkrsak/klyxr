//! Canonical KED-030 audit identities. Membership never grants permission.
use std::fmt;

/// Independent built-in identities, in canonical presentation order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Effect {
    Io,
    Heap,
    Blocking,
    Network,
    Filesystem,
    Clock,
    Random,
    Unsafe,
}

impl Effect {
    pub const ALL: [Self; 8] = [
        Self::Io,
        Self::Heap,
        Self::Blocking,
        Self::Network,
        Self::Filesystem,
        Self::Clock,
        Self::Random,
        Self::Unsafe,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Io => "io",
            Self::Heap => "heap",
            Self::Blocking => "blocking",
            Self::Network => "network",
            Self::Filesystem => "filesystem",
            Self::Clock => "clock",
            Self::Random => "random",
            Self::Unsafe => "unsafe",
        }
    }

    /// Source-name resolution only; runtime operations use canonical IDs.
    pub fn resolve(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|effect| effect.name() == name)
    }
}

/// All 256 byte values are valid sets. There are no spare byte bits.
/// Private storage prevents unchecked wider encodings from being narrowed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EffectSet(u8);

impl EffectSet {
    pub const EMPTY: Self = Self(0);

    pub const fn contains(self, effect: Effect) -> bool {
        self.0 & (1 << effect as u8) != 0
    }

    /// Returns false for a duplicate; source resolution must reject that case.
    pub fn insert(&mut self, effect: Effect) -> bool {
        let fresh = !self.contains(effect);
        self.0 |= 1 << effect as u8;
        fresh
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn is_subset(self, declared: Self) -> bool {
        self.0 & !declared.0 == 0
    }

    pub fn iter(self) -> impl Iterator<Item = Effect> {
        Effect::ALL.into_iter().filter(move |e| self.contains(*e))
    }
}

impl TryFrom<u16> for EffectSet {
    type Error = &'static str;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        u8::try_from(value)
            .map(Self)
            .map_err(|_| "foreign effect identity bits")
    }
}

impl fmt::Display for EffectSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, effect) in self.iter().enumerate() {
            if index != 0 {
                f.write_str(", ")?;
            }
            f.write_str(effect.name())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_are_independent_and_render_in_canonical_order() {
        let mut set = EffectSet::EMPTY;
        for effect in Effect::ALL.into_iter().rev() {
            assert_eq!(Effect::resolve(effect.name()), Some(effect));
            assert!(set.insert(effect));
            assert!(!set.insert(effect));
        }
        assert_eq!(
            set.to_string(),
            "io, heap, blocking, network, filesystem, clock, random, unsafe"
        );
        for name in ["IO", "unknown", "", "trusted", "network_io"] {
            assert_eq!(Effect::resolve(name), None);
        }
        let mut network = EffectSet::EMPTY;
        network.insert(Effect::Network);
        assert!(!network.contains(Effect::Io));
        assert!(!network.contains(Effect::Unsafe));
    }

    #[test]
    fn every_bounded_set_obeys_union_and_subset() {
        for a in 0u16..=255 {
            let left = EffectSet::try_from(a).unwrap();
            assert_eq!(left.iter().count(), a.count_ones() as usize);
            assert!(left.is_subset(left));
            assert!(EffectSet::EMPTY.is_subset(left));
            assert_eq!(left.union(left), left);
            for b in 0u16..=255 {
                let right = EffectSet::try_from(b).unwrap();
                assert_eq!(left.is_subset(right), a & !b == 0);
                assert_eq!(left.union(right), EffectSet::try_from(a | b).unwrap());
            }
        }
    }

    #[test]
    fn wider_foreign_encodings_reject_before_narrowing() {
        for value in 256u16..=u16::MAX {
            assert!(EffectSet::try_from(value).is_err());
        }
        assert_eq!(std::mem::size_of::<EffectSet>(), 1);
        assert_eq!(EffectSet::EMPTY.to_string(), "");
    }
}
