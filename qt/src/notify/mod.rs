//! New-mail alerts on the platform: which `Poster` this run uses, and the
//! one for desktops, `org.freedesktop.Notifications` over QtDBus
//! (qt/notify.cpp). The C++ side only makes the calls; what to send, the
//! card ids per tag and the choice of poster are decided here.

mod args;
mod choose;
mod ids;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard, Once, PoisonError};

use envio_core::{Alert, Poster};

use self::args::NotifyArgs;
use self::choose::{Backend, choose};
use self::ids::{Ids, ToSend};
use crate::postal::PostalPoster;

/// A UTF-8 slice for the C++ side; it copies it before the call returns.
#[repr(C)]
struct Str {
    ptr: *const u8,
    len: i32,
}

impl Str {
    fn of(text: &str) -> Self {
        Self {
            ptr: text.as_ptr(),
            len: i32::try_from(text.len()).unwrap_or(0),
        }
    }
}

type ReplyFn = extern "C" fn(tag: *const u8, tag_len: i32, id: u32);
type CardFn = extern "C" fn(id: u32);

#[cfg(not(test))]
unsafe extern "C" {
    fn envio_bus_has_service(name: *const u8, len: i32) -> bool;
    fn envio_notify_has_markup() -> bool;
    fn envio_notify_init(on_reply: ReplyFn, on_closed: CardFn, on_clicked: CardFn);
    #[allow(clippy::too_many_arguments)]
    fn envio_notify_send(
        tag: Str,
        app_name: Str,
        icon: Str,
        summary: Str,
        body: Str,
        category: Str,
        replaces_id: u32,
    );
    fn envio_activate_window();
}

// `cargo test` links no C++: the calls do nothing there.
#[cfg(test)]
unsafe fn envio_bus_has_service(_name: *const u8, _len: i32) -> bool {
    false
}
#[cfg(test)]
unsafe fn envio_notify_has_markup() -> bool {
    false
}
#[cfg(test)]
unsafe fn envio_notify_init(_on_reply: ReplyFn, _on_closed: CardFn, _on_clicked: CardFn) {}
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
unsafe fn envio_notify_send(
    _tag: Str,
    _app_name: Str,
    _icon: Str,
    _summary: Str,
    _body: Str,
    _category: Str,
    _replaces_id: u32,
) {
}
#[cfg(test)]
unsafe fn envio_activate_window() {}

static IDS: LazyLock<Mutex<Ids>> = LazyLock::new(|| Mutex::new(Ids::default()));
static MARKUP: AtomicBool = AtomicBool::new(false);
static INIT: Once = Once::new();

fn ids() -> MutexGuard<'static, Ids> {
    IDS.lock().unwrap_or_else(PoisonError::into_inner)
}

fn has_service(name: &str) -> bool {
    // SAFETY: the pointer and length describe `name`, which outlives the call.
    unsafe { envio_bus_has_service(name.as_ptr(), i32::try_from(name.len()).unwrap_or(0)) }
}

/// Lomiri sets `APP_ID` (`envio.aps_envio...`) for the apps it starts.
fn ut_env() -> bool {
    std::env::var("APP_ID").is_ok_and(|id| id.starts_with("envio.aps_envio"))
}

/// The poster for this system, or `None` when it has nowhere to show alerts.
/// Main thread, once at start (the bus probes block briefly).
pub(crate) fn poster() -> Option<Box<dyn Poster>> {
    // Lazy: the probes only run while the earlier answer is still open.
    let ut = ut_env();
    let postal = !ut && has_service("com.lomiri.Postal");
    let fdo = !ut && !postal && has_service("org.freedesktop.Notifications");
    match choose(ut, postal, fdo) {
        Backend::Postal => Some(Box::new(PostalPoster)),
        Backend::Freedesktop => {
            INIT.call_once(|| {
                // SAFETY: `envio_notify_has_markup` takes no pointers;
                // the callbacks are `extern "C"` fns that live for ever.
                unsafe {
                    MARKUP.store(envio_notify_has_markup(), Ordering::Relaxed);
                    envio_notify_init(on_reply, on_closed, on_clicked);
                }
            });
            Some(Box::new(FreedesktopPoster))
        }
        Backend::None => None,
    }
}

struct FreedesktopPoster;

impl Poster for FreedesktopPoster {
    fn post(&self, alert: &Alert) {
        // The lock is gone before the call to C++.
        let send = ids().begin(alert.clone());
        if let Some(send) = send {
            send_now(send);
        }
    }

    /// Generic desktops have no launcher badge.
    fn badge(&self, _total_unread: u32) {}
}

fn send_now((alert, replaces_id): ToSend) {
    let args = NotifyArgs::from_alert(&alert, replaces_id, MARKUP.load(Ordering::Relaxed));
    // SAFETY: every `Str` describes a string that outlives the call; the
    // C++ side copies the bytes before it returns.
    unsafe {
        envio_notify_send(
            Str::of(&alert.tag),
            Str::of(args.app_name),
            Str::of(args.icon),
            Str::of(&args.summary),
            Str::of(&args.body),
            Str::of(args.category),
            args.replaces_id,
        );
    }
}

/// Notify's reply, on the main thread: `id` is 0 when the call failed.
extern "C" fn on_reply(tag: *const u8, tag_len: i32, id: u32) {
    let Ok(len) = usize::try_from(tag_len) else {
        return;
    };
    // SAFETY: C++ passes the tag bytes it holds for the length of this call.
    let bytes = unsafe { std::slice::from_raw_parts(tag, len) };
    let Ok(tag) = std::str::from_utf8(bytes) else {
        return;
    };
    let next = ids().finished(tag, id);
    if let Some(next) = next {
        send_now(next);
    }
}

/// `NotificationClosed`, main thread.
extern "C" fn on_closed(id: u32) {
    ids().closed(id);
}

/// `ActionInvoked`, main thread: a click on one of Envio's cards. Raising
/// the window is best effort (Wayland may refuse without a token). Opening
/// the account's Inbox is #230 / #247: `account_of` is the hook for it.
extern "C" fn on_clicked(id: u32) {
    let ours = ids().account_of(id).is_some();
    if ours {
        // SAFETY: takes no pointers.
        unsafe { envio_activate_window() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alert(tag: &str) -> Alert {
        Alert {
            tag: tag.into(),
            summary: "s".into(),
            body: "b".into(),
            account: "acc".into(),
        }
    }

    #[test]
    fn the_poster_posts_and_takes_replies_without_panicking_off_the_bus() {
        // The tests link no C++: this walks the glue (lock, begin, reply).
        let poster = FreedesktopPoster;
        poster.post(&alert("glue-tag"));
        poster.post(&alert("glue-tag"));
        poster.badge(3);
        on_reply(b"glue-tag".as_ptr(), 8, 5);
        on_reply(b"glue-tag".as_ptr(), 8, 0);
        on_closed(5);
        on_clicked(5);
        assert_eq!(ids().account_of(5), None);
    }
}
