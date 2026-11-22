//! Sending through Graph's sendMail. The recipients come from the MIME
//! headers, so the envelope's Bcc goes in as a header of the sent copy. A
//! send is never retried here: a lost or doubtful answer is `unconfirmed`,
//! which the Outbox turns into "may have been sent".

use envio_net::NetError;
use envio_net::smtp::SendError;

use super::GraphServer;
use crate::plan::bcc::with_bcc;
use crate::server::MailSender;

impl MailSender for GraphServer {
    async fn send(&mut self, _from: &str, to: &[String], eml: &[u8]) -> Result<(), SendError> {
        // The sender is the signed-in mailbox; Graph ignores any other.
        let mime = with_bcc(eml, to);
        self.client.send_mime(&mime).await.map_err(|error| {
            let unconfirmed = matches!(error, NetError::Unconfirmed(_));
            SendError { error, unconfirmed }
        })
    }
}
