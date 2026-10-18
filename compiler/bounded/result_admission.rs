//! Result admission reservations retain typed tool failures. Injection is private
//! to this test build and occurs only when the actual reservation needs growth.
use super::{ResourceFailure, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Allocation {
    Syntax,
    Scratch,
    Table,
}
impl Allocation {
    fn resource(self) -> &'static str {
        match self {
            Self::Syntax => "Result type syntax allocation",
            Self::Scratch => "Result resolution scratch allocation",
            Self::Table => "Result canonical table allocation",
        }
    }
}

#[cfg(test)]
thread_local! {
    static FAILURE: std::cell::Cell<Option<(Allocation, usize)>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn with_failure<T>(site: Allocation, after: usize, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<(Allocation, usize)>);
    impl Drop for Restore {
        fn drop(&mut self) {
            FAILURE.with(|s| s.set(self.0));
        }
    }
    let _restore = Restore(FAILURE.with(|s| s.replace(Some((site, after)))));
    f()
}

pub(crate) fn reserve<T>(
    values: &mut Vec<T>,
    additional: usize,
    site: Allocation,
    span: Span,
) -> Result<(), ResourceFailure> {
    #[cfg(test)]
    if additional > values.capacity() - values.len() {
        let fail = FAILURE.with(|s| match s.get() {
            Some((selected, remaining)) if selected == site => {
                if remaining == 0 {
                    true
                } else {
                    s.set(Some((selected, remaining - 1)));
                    false
                }
            }
            _ => false,
        });
        if fail {
            return Err(ResourceFailure::host(site.resource(), span));
        }
    }
    let reserved = match site {
        Allocation::Scratch => values.try_reserve_exact(additional),
        Allocation::Syntax | Allocation::Table => values.try_reserve(additional),
    };
    reserved.map_err(|_| ResourceFailure::host(site.resource(), span))
}
