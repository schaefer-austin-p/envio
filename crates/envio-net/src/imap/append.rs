//! APPEND, run by hand: async-imap's own `append` drops the server's
//! reason for a refusal, and envio needs the response codes (a NO with
//! `[UNAVAILABLE]` is "busy", not "refused").

use async_imap::imap_proto::{RequestId, Response};
use tokio::io::AsyncWriteExt;

use super::ImapSession;
use super::client::{Stream, command_end, map_error};
use super::query::{self, Flag};
use crate::connect;
use crate::error::{NetError, Result};

enum Step {
    Continue,
    End(Result<()>),
    Skip,
}

impl ImapSession {
    /// Uploads a message (Sent copy, draft) with `flags`.
    pub async fn append(&mut self, folder: &str, flags: &[Flag], eml: &[u8]) -> Result<()> {
        let mailbox = query::quote(folder)?;
        let flags = if flags.is_empty() {
            String::new()
        } else {
            format!(" {}", query::flag_list(flags)?)
        };
        let command = format!("APPEND {mailbox}{flags} {{{}}}", eml.len());
        // The upload alone can outlast a command, so the limit grows with
        // the message.
        connect::within_for(connect::transfer_timeout(eml.len()), async {
            let id = self.session.run_command(command).await.map_err(map_error)?;
            loop {
                match self.step(&id).await? {
                    Step::Continue => break,
                    Step::End(Err(error)) => return Err(error),
                    Step::End(Ok(())) => {
                        return Err(NetError::Protocol("APPEND ended before its literal".into()));
                    }
                    Step::Skip => {}
                }
            }
            let stream: &mut Stream = self.session.as_mut();
            stream.write_all(eml).await?;
            stream.write_all(b"\r\n").await?;
            stream.flush().await?;
            loop {
                if let Step::End(end) = self.step(&id).await? {
                    return end;
                }
            }
        })
        .await
    }

    /// Reads one reply and says what it means for the command.
    async fn step(&mut self, id: &RequestId) -> Result<Step> {
        let response = match self.session.read_response().await {
            Ok(Some(response)) => response,
            Ok(None) => return Err(NetError::Io(std::io::ErrorKind::UnexpectedEof.into())),
            Err(error) => return Err(NetError::Io(error)),
        };
        Ok(match response.parsed() {
            Response::Continue { .. } => Step::Continue,
            other => command_end(other, id).map_or(Step::Skip, Step::End),
        })
    }
}
