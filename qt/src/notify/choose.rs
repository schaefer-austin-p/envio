//! Which poster this run uses: Postal on Ubuntu Touch, the freedesktop
//! notification service elsewhere, nothing when the bus has neither.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Backend {
    Postal,
    Freedesktop,
    None,
}

/// `ut_env`: the Lomiri app environment is set. `postal` / `fdo`: the
/// session bus has (or can start) that service.
pub(crate) fn choose(ut_env: bool, postal: bool, fdo: bool) -> Backend {
    if ut_env || postal {
        Backend::Postal
    } else if fdo {
        Backend::Freedesktop
    } else {
        Backend::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ubuntu_touch_gets_postal_however_the_probe_went() {
        for postal in [false, true] {
            for fdo in [false, true] {
                assert_eq!(choose(true, postal, fdo), Backend::Postal);
            }
        }
    }

    #[test]
    fn postal_on_the_bus_wins_over_the_desktop_service() {
        assert_eq!(choose(false, true, true), Backend::Postal);
        assert_eq!(choose(false, true, false), Backend::Postal);
    }

    #[test]
    fn a_desktop_with_a_notification_service_gets_freedesktop() {
        assert_eq!(choose(false, false, true), Backend::Freedesktop);
    }

    #[test]
    fn a_bus_with_neither_gets_nothing() {
        assert_eq!(choose(false, false, false), Backend::None);
    }
}
