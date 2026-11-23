//! New-mail alerts on the platform: which `Poster` this run uses, and the
//! one for desktops, `org.freedesktop.Notifications` over QtDBus
//! (qt/notify.cpp). The C++ side only makes the calls; what to send, the
//! card ids per tag and the choice of poster are decided here.

mod args;
mod choose;
mod ids;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard, Once, OnceLock, PoisonError};

use envio_core::{Alert, Poster, paths};

use self::args::{Markup, NotifyArgs};
use self::choose::{Backend, choose};
use self::ids::{Ids, ToSend};
use crate::postal::PostalPoster;

/// The app icon, in the binary: an external notification server can't read
/// the qrc, and `cargo make run-desktop` installs nothing. It is written
/// once to the cache folder, whose path goes to the server.
const ICON_SVG: &[u8] = include_bytes!("../../../assets/logo.svg");

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

/// Everything C++ puts into a Notify call, typed; it decides nothing.
#[repr(C)]
struct Card {
    tag: Str,
    app_name: Str,
    icon: Str,
    summary: Str,
    body: Str,
    category: Str,
    desktop_entry: Str,
    /// Empty: no `image-path` hint.
    image_path: Str,
    replaces_id: u32,
    /// The epoch of the call; its reply names it back.
    generation: u32,
    urgency: u8,
}

type ReplyFn = extern "C" fn(tag: *const u8, tag_len: i32, id: u32, generation: u32);
type CardFn = extern "C" fn(id: u32);
/// `ActivationToken`: the card and the token (UTF-8, valid for the call).
type TokenFn = extern "C" fn(id: u32, token: *const u8, token_len: i32);
/// `answer`: 0 the question failed; 1 no `body-markup`; 2 `body-markup`.
/// `generation` is the one the question was asked under.
type CapsFn = extern "C" fn(answer: i32, generation: u32);
/// `had_owner`: the name had an owner before (a server replaced or gone),
/// false when it was only taken (first start, D-Bus activation).
type OwnerFn = extern "C" fn(had_owner: bool);

#[cfg(not(test))]
unsafe extern "C" {
    fn envio_bus_has_service(name: *const u8, len: i32) -> bool;
    fn envio_notify_init(
        on_reply: ReplyFn,
        on_closed: CardFn,
        on_clicked: CardFn,
        on_token: TokenFn,
        on_caps: CapsFn,
        on_owner: OwnerFn,
    );
    fn envio_notify_query_caps(generation: u32);
    fn envio_notify_send(card: *const Card);
    fn envio_activate_window(token: *const u8, token_len: i32);
}

// `cargo test` links no C++: the calls do nothing there.
#[cfg(test)]
unsafe fn envio_bus_has_service(_name: *const u8, _len: i32) -> bool {
    false
}
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
unsafe fn envio_notify_init(
    _on_reply: ReplyFn,
    _on_closed: CardFn,
    _on_clicked: CardFn,
    _on_token: TokenFn,
    _on_caps: CapsFn,
    _on_owner: OwnerFn,
) {
}
#[cfg(test)]
unsafe fn envio_notify_query_caps(_generation: u32) {}
#[cfg(test)]
unsafe fn envio_notify_send(_card: *const Card) {}
/// The tokens the tests' window raises were given.
#[cfg(test)]
static ACTIVATED: Mutex<Vec<String>> = Mutex::new(Vec::new());
#[cfg(test)]
unsafe fn envio_activate_window(token: *const u8, token_len: i32) {
    let bytes = unsafe { std::slice::from_raw_parts(token, usize::try_from(token_len).unwrap()) };
    ACTIVATED
        .lock()
        .unwrap()
        .push(String::from_utf8_lossy(bytes).into_owned());
}

static IDS: LazyLock<Mutex<Ids>> = LazyLock::new(|| Mutex::new(Ids::default()));
static INIT: Once = Once::new();
static ICON: OnceLock<Option<String>> = OnceLock::new();
/// 0 unknown, 1 no body markup, 2 body markup.
static CAPS: AtomicU8 = AtomicU8::new(0);
static CAPS_ASKED: AtomicBool = AtomicBool::new(false);
/// Bumped when the server changes owner: answers to older questions are
/// stale.
static CAPS_EPOCH: AtomicU32 = AtomicU32::new(0);

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

/// The poster for this system: Postal on Ubuntu Touch (one short bus probe
/// unless `APP_ID` already says so), else the freedesktop one, whether or
/// not a server is up yet. Main thread, once at start.
pub(crate) fn poster() -> Box<dyn Poster> {
    match choose(ut_env(), || has_service("com.lomiri.Postal")) {
        Backend::Postal => Box::new(PostalPoster),
        Backend::Freedesktop => {
            INIT.call_once(|| {
                // SAFETY: the callbacks are `extern "C"` fns that live for
                // ever; the call takes no pointers otherwise.
                unsafe {
                    envio_notify_init(on_reply, on_closed, on_clicked, on_token, on_caps, on_owner);
                }
                ask_caps();
            });
            Box::new(FreedesktopPoster)
        }
    }
}

/// Asks the server for its capabilities (async), once at a time.
fn ask_caps() {
    if !CAPS_ASKED.swap(true, Ordering::AcqRel) {
        // SAFETY: takes no pointers.
        unsafe { envio_notify_query_caps(CAPS_EPOCH.load(Ordering::Acquire)) };
    }
}

fn markup() -> Markup {
    match CAPS.load(Ordering::Relaxed) {
        1 => Markup::No,
        2 => Markup::Yes,
        _ => {
            // Unknown: the first question failed or has not come back.
            ask_caps();
            Markup::Unknown
        }
    }
}

/// The icon file in the cache folder, written once; `None` when it can't
/// be (the card then names the theme icon).
fn icon_file() -> Option<&'static str> {
    ICON.get_or_init(|| {
        let env = |name: &str| std::env::var(name).ok();
        let (cache, _) = paths::app_dirs(&env);
        write_icon(&cache.join("notify"), ICON_SVG)
    })
    .as_deref()
}

fn write_icon(dir: &std::path::Path, bytes: &[u8]) -> Option<String> {
    let file: PathBuf = dir.join("logo.svg");
    if std::fs::read(&file).is_ok_and(|have| have == bytes) {
        return file.to_str().map(str::to_owned);
    }
    std::fs::create_dir_all(dir).ok()?;
    // Beside the file, then renamed: a server never reads half an icon.
    let tmp = dir.join(format!("logo.svg.{}", std::process::id()));
    std::fs::write(&tmp, bytes).ok()?;
    std::fs::rename(&tmp, &file).ok()?;
    file.to_str().map(str::to_owned)
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

fn send_now((alert, replaces_id, generation): ToSend) {
    let args = NotifyArgs::from_alert(&alert, replaces_id, markup(), icon_file());
    let card = Card {
        tag: Str::of(&alert.tag),
        app_name: Str::of(args.app_name),
        icon: Str::of(&args.icon),
        summary: Str::of(&args.summary),
        body: Str::of(&args.body),
        category: Str::of(args.category),
        desktop_entry: Str::of(args.desktop_entry),
        image_path: Str::of(&args.image_path),
        replaces_id: args.replaces_id,
        generation,
        urgency: args.urgency,
    };
    // SAFETY: `card` and every string it points to outlive the call; the C++
    // side copies them before it returns.
    unsafe { envio_notify_send(&raw const card) };
}

/// Notify's reply, on the main thread: `id` is 0 when the call failed.
extern "C" fn on_reply(tag: *const u8, tag_len: i32, id: u32, generation: u32) {
    let Ok(len) = usize::try_from(tag_len) else {
        return;
    };
    // SAFETY: C++ passes the tag bytes it holds for the length of this call.
    let bytes = unsafe { std::slice::from_raw_parts(tag, len) };
    let Ok(tag) = std::str::from_utf8(bytes) else {
        return;
    };
    let next = ids().finished(tag, id, generation);
    if let Some(next) = next {
        send_now(next);
    }
}

/// `NotificationClosed`, main thread.
extern "C" fn on_closed(id: u32) {
    ids().closed(id);
}

/// `ActivationToken`, main thread: the server's xdg-activation token for a
/// click that is about to be reported.
extern "C" fn on_token(id: u32, token: *const u8, token_len: i32) {
    let Ok(len) = usize::try_from(token_len) else {
        return;
    };
    // SAFETY: C++ passes the token bytes it holds for the length of this call.
    let bytes = unsafe { std::slice::from_raw_parts(token, len) };
    if let Ok(token) = std::str::from_utf8(bytes) {
        ids().token(id, token.to_owned());
    }
}

/// `ActionInvoked`, main thread: a click on one of Envio's cards. The window
/// comes to the front (with the server's token where there is one, so
/// Wayland allows it) and the app is asked for that account's Inbox.
extern "C" fn on_clicked(id: u32) {
    let click = ids().click(id);
    let Some(click) = click else {
        return;
    };
    // SAFETY: the pointer and length describe `click.token`, which outlives
    // the call.
    unsafe {
        envio_activate_window(
            click.token.as_ptr(),
            i32::try_from(click.token.len()).unwrap_or(0),
        )
    };
    crate::open_route::open_inbox(&click.account);
}

/// The answer to `GetCapabilities`, main thread. One to a question from
/// before a change of owner is stale and ignored.
extern "C" fn on_caps(answer: i32, generation: u32) {
    if generation != CAPS_EPOCH.load(Ordering::Acquire) {
        return;
    }
    CAPS.store(u8::try_from(answer).unwrap_or(0).min(2), Ordering::Relaxed);
    // A failed question may be asked again by the next card.
    CAPS_ASKED.store(false, Ordering::Release);
}

/// The notification server changed owner, main thread. A server that was
/// replaced or went away: its ids and capabilities are not the old one's.
/// A server only taking the name (our own first call starts it) changes
/// nothing, or the call that woke it would lose its reply.
extern "C" fn on_owner(had_owner: bool) {
    if !had_owner {
        return;
    }
    let again = ids().reset();
    CAPS_EPOCH.fetch_add(1, Ordering::AcqRel);
    CAPS.store(0, Ordering::Relaxed);
    CAPS_ASKED.store(false, Ordering::Release);
    ask_caps();
    // Alerts that waited on the old server's reply go out as new cards.
    for send in again {
        send_now(send);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The statics (capabilities, ids) are shared by the tests.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn serial() -> MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
    }

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
        let _s = serial();
        // The tests link no C++: this walks the glue (lock, begin, reply).
        let poster = FreedesktopPoster;
        poster.post(&alert("glue-tag"));
        poster.post(&alert("glue-tag"));
        poster.badge(3);
        let epoch = ids().epoch();
        on_reply(b"glue-tag".as_ptr(), 8, 5, epoch);
        on_reply(b"glue-tag".as_ptr(), 8, 0, epoch);
        on_closed(5);
        on_clicked(5);
        on_owner(true);
        assert_eq!(ids().account_of(5), None);
    }

    #[test]
    fn the_capabilities_answer_sets_the_markup_and_a_failure_means_unknown() {
        let _s = serial();
        on_caps(2, CAPS_EPOCH.load(Ordering::Acquire));
        assert_eq!(markup(), Markup::Yes);
        on_caps(1, CAPS_EPOCH.load(Ordering::Acquire));
        assert_eq!(markup(), Markup::No);
        on_caps(0, CAPS_EPOCH.load(Ordering::Acquire));
        assert_eq!(markup(), Markup::Unknown);
    }

    #[test]
    fn the_icon_is_written_once_and_rewritten_when_it_differs() {
        let dir = std::env::temp_dir().join(format!("envio-notify-icon-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = write_icon(&dir, b"one").unwrap();
        assert!(path.ends_with("/logo.svg"));
        assert_eq!(std::fs::read(&path).unwrap(), b"one");
        write_icon(&dir, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        let _ = std::fs::remove_dir_all(&dir);
        // A folder that can't be made: no icon, no panic.
        assert_eq!(
            write_icon(std::path::Path::new("/proc/envio/x"), b"x"),
            None
        );
    }

    #[test]
    fn the_shipped_icon_is_an_svg() {
        assert!(ICON_SVG.starts_with(b"<svg"));
    }

    #[test]
    fn a_server_only_taking_the_name_keeps_the_call_that_woke_it() {
        let _s = serial();
        // Our first Notify D-Bus-activates the server: the name changes
        // owner with no old owner, while the reply is still on its way.
        FreedesktopPoster.post(&alert("wake-tag"));
        FreedesktopPoster.post(&alert("wake-tag"));
        // The reply names the epoch the call was sent under.
        let epoch = ids().epoch();
        on_owner(false);
        on_reply(b"wake-tag".as_ptr(), 8, 31, epoch);
        assert_eq!(ids().account_of(31), Some("acc"));
        on_closed(31);
    }

    #[test]
    fn a_stale_capabilities_answer_is_ignored() {
        let _s = serial();
        let old = CAPS_EPOCH.load(Ordering::Acquire);
        on_owner(true);
        assert_eq!(markup(), Markup::Unknown);
        // The question from before the change answers while the new one
        // is still on its way.
        on_caps(2, old);
        assert_eq!(markup(), Markup::Unknown);
        assert!(CAPS_ASKED.load(Ordering::Acquire));
        on_caps(2, CAPS_EPOCH.load(Ordering::Acquire));
        assert_eq!(markup(), Markup::Yes);
    }

    #[test]
    fn a_click_raises_the_window_with_its_token_and_opens_that_accounts_inbox() {
        let _s = serial();
        let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
        let sink = std::sync::Arc::clone(&seen);
        crate::open_route::set_handler(move |account| sink.lock().unwrap().push(account));
        ACTIVATED.lock().unwrap().clear();
        FreedesktopPoster.post(&alert("click-tag"));
        let epoch = ids().epoch();
        on_reply(b"click-tag".as_ptr(), 9, 41, epoch);
        // The server sends the token just before the click.
        on_token(41, b"tok-1".as_ptr(), 5);
        on_clicked(41);
        assert_eq!(*seen.lock().unwrap(), vec!["acc".to_owned()]);
        assert_eq!(*ACTIVATED.lock().unwrap(), vec!["tok-1".to_owned()]);
        // The token is used up; a click without one raises without a token.
        on_clicked(41);
        assert_eq!(ACTIVATED.lock().unwrap()[1], "");
        // Another app's card (or a token for one) does nothing.
        on_token(77, b"x".as_ptr(), 1);
        on_clicked(77);
        assert_eq!(seen.lock().unwrap().len(), 2);
        assert_eq!(ACTIVATED.lock().unwrap().len(), 2);
        on_closed(41);
    }
}
