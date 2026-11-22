//! When both the password and the token removal fail, the second error is
//! logged (without a secret). The logger is global, so this binary owns it.

mod support;

use std::io::Write;
use std::sync::{Arc, Mutex};

use envio_core::{Command, Event, logging};
use log::LevelFilter;
use secrecy::SecretString;
use support::Harness;

#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Vec<u8>>>);

impl Write for Shared {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_second_removal_failure_is_logged_without_the_token() {
    let shared = Shared::default();
    logging::init_with_writer(Box::new(shared.clone()), LevelFilter::Info).unwrap();
    let mut h = Harness::unstarted();
    let key = h.seed_account("ada@example.org", Some("pw-secret-9"));
    let mut tokens = envio_core::oauth_tokens::OAuthTokens::open(&h.data_dir()).unwrap();
    tokens
        .set(
            &key,
            envio_net::oauth::ProviderKind::Microsoft,
            SecretString::from("rt-secret-9".to_owned()),
        )
        .unwrap();
    h.start_core();
    h.wait(|e| matches!(e, Event::Started { .. }).then_some(()));
    for name in [
        envio_core::secrets::FILE_NAME,
        envio_core::oauth_tokens::FILE_NAME,
    ] {
        let file = h.data_dir().join(name);
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir(&file).unwrap();
    }
    h.send(Command::RemoveAccount {
        account: key.clone(),
        confirmed: true,
    });
    h.wait(|e| matches!(e, Event::Notice { .. }).then_some(()));
    drop(h.core.take());
    let out = String::from_utf8(shared.0.lock().unwrap().clone()).unwrap();
    assert!(out.contains("removing the OAuth token"), "{out}");
    assert!(
        !out.contains("rt-secret-9") && !out.contains("pw-secret-9"),
        "{out}"
    );
}
