//! A custom domain whose MX records say Microsoft 365 (#384): the answer to
//! Detect is a card, not servers.

use envio_net::autoconfig::plan::domain_of;
use envio_net::oauth::ProviderKind;

use super::Worker;
use crate::api::event::Event;
use crate::text;

impl Worker<'_> {
    /// Tells the app the mail is on Microsoft: the card with sign-in when
    /// the Microsoft provider is set up, else the #381 refusal with the
    /// domain. Never `Detected`: no servers are offered, so the confirm
    /// dialog and `srv_hosts` stay as they were.
    pub(super) fn mail_on_microsoft(&mut self, email: String) {
        let domain = domain_of(&email).unwrap_or_default();
        if self.provider_of(ProviderKind::Microsoft).is_some() {
            let (title, text) = text::setup::microsoft_hosted(&domain);
            self.emit(Event::MailOnMicrosoft { email, title, text });
        } else {
            let (title, text) = text::setup::microsoft_hosted_refused(&domain);
            self.emit(Event::SetupRefused {
                email,
                title,
                text,
                manual_ok: true,
            });
        }
    }
}
