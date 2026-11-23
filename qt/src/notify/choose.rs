//! Which poster this run uses: Postal on Ubuntu Touch, the freedesktop
//! notification service elsewhere. Off Ubuntu Touch the freedesktop poster
//! is always installed: a failed call is answered with id 0, so cards start
//! working when a server appears later.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Backend {
    Postal,
    Freedesktop,
}

/// `ut_env`: the Lomiri app environment is set. `postal`: the session bus
/// has (or can start) Postal; asked only when `ut_env` is false.
pub(crate) fn choose(ut_env: bool, postal: impl FnOnce() -> bool) -> Backend {
    if ut_env || postal() {
        Backend::Postal
    } else {
        Backend::Freedesktop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ubuntu_touch_gets_postal_without_asking_the_bus() {
        assert_eq!(
            choose(true, || panic!("no probe on Ubuntu Touch")),
            Backend::Postal
        );
    }

    #[test]
    fn postal_on_the_bus_gets_postal() {
        assert_eq!(choose(false, || true), Backend::Postal);
    }

    #[test]
    fn a_desktop_always_gets_freedesktop_even_without_a_server_yet() {
        assert_eq!(choose(false, || false), Backend::Freedesktop);
    }
}
