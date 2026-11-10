//! A TCP proxy that can cut connections mid-stream: the network dropping
//! out in the middle of a download, an upload or SMTP DATA.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Client to server (uploads, APPEND, DATA).
    Up,
    /// Server to client (downloads, FETCH).
    Down,
}

/// How long the proxy waits to reach the real server.
const CONNECT: Duration = Duration::from_secs(5);

/// What arms the one cut.
#[derive(Clone, Copy)]
enum Trigger {
    /// Bytes still allowed; the chunk that spends the last of them is cut.
    Bytes(usize),
    /// Chunks still allowed; the chunk that would be the last is cut,
    /// unforwarded.
    Chunks(usize),
}

#[derive(Default)]
struct Budget {
    left: Option<(Direction, Trigger)>,
}

pub struct Proxy {
    pub port: u16,
    budget: Arc<Mutex<Budget>>,
}

impl Proxy {
    /// Forwards every connection on a free local port to `host:port`.
    pub async fn start(host: &str, port: u16) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local = listener.local_addr().unwrap().port();
        let budget = Arc::new(Mutex::new(Budget::default()));
        let target = format!("{host}:{port}");
        let shared = budget.clone();
        tokio::spawn(async move {
            while let Ok((client, _)) = listener.accept().await {
                let target = target.clone();
                let shared = shared.clone();
                // Connect per connection, so one slow connect cannot hold
                // up the accept loop.
                tokio::spawn(async move {
                    let connect = TcpStream::connect(&target);
                    if let Ok(Ok(server)) = tokio::time::timeout(CONNECT, connect).await {
                        pipe(client, server, shared).await;
                    }
                });
            }
        });
        Self {
            port: local,
            budget,
        }
    }

    /// From now on, once `bytes` more have passed in `direction` (counted
    /// across all connections), the connection that passes the last of
    /// them is cut: both of its legs close at that byte, and the client
    /// sees the stream end there. The budget is spent once, so later
    /// connections pass untouched.
    ///
    /// Bytes are what goes over the TCP socket, so under TLS they include
    /// record overhead; a test that needs an exact point uses
    /// [`Proxy::cut_at_chunk`].
    pub fn cut_after(&self, direction: Direction, bytes: usize) {
        self.budget.lock().unwrap().left = Some((direction, Trigger::Bytes(bytes)));
    }

    /// From now on, the `chunk`-th read in `direction` (counted across all
    /// connections, 1-based) is not forwarded: its connection is cut
    /// instead. Each read is one chunk, so the earlier reads pass through
    /// and the cut lands on a known message boundary. Spent once, like
    /// [`Proxy::cut_after`].
    pub fn cut_at_chunk(&self, direction: Direction, chunk: usize) {
        self.budget.lock().unwrap().left = Some((direction, Trigger::Chunks(chunk)));
    }
}

async fn pipe(client: TcpStream, server: TcpStream, budget: Arc<Mutex<Budget>>) {
    let (client_read, client_write) = client.into_split();
    let (server_read, server_write) = server.into_split();
    let (cut_tx, cut_rx) = watch::channel(false);
    let up = Leg {
        direction: Direction::Up,
        budget: budget.clone(),
        cut_tx: cut_tx.clone(),
        cut_rx: cut_rx.clone(),
    };
    let down = Leg {
        direction: Direction::Down,
        budget,
        cut_tx,
        cut_rx,
    };
    tokio::join!(
        up.copy(client_read, server_write),
        down.copy(server_read, client_write)
    );
}

/// One direction of a proxied connection.
struct Leg {
    direction: Direction,
    budget: Arc<Mutex<Budget>>,
    /// A cut on either leg closes both.
    cut_tx: watch::Sender<bool>,
    cut_rx: watch::Receiver<bool>,
}

impl Leg {
    /// Copies until EOF or a cut.
    async fn copy(mut self, mut from: OwnedReadHalf, mut to: OwnedWriteHalf) {
        let mut buf = vec![0; 4096];
        loop {
            let n = tokio::select! {
                read = from.read(&mut buf) => match read {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                },
                _ = self.cut_rx.changed() => return,
            };
            let (allowed, cut) = take(&self.budget, self.direction, n);
            if to.write_all(&buf[..allowed]).await.is_err() {
                break;
            }
            if cut {
                let _ = self.cut_tx.send(true);
                return;
            }
        }
        let _ = to.shutdown().await;
    }
}

/// How many of `n` bytes may pass, and whether this chunk ends the
/// connection. Spends the budget.
fn take(budget: &Mutex<Budget>, direction: Direction, n: usize) -> (usize, bool) {
    let mut budget = budget.lock().unwrap();
    let Some((cut, trigger)) = budget.left else {
        return (n, false);
    };
    if cut != direction {
        return (n, false);
    }
    match trigger {
        Trigger::Bytes(left) if n < left => {
            budget.left = Some((cut, Trigger::Bytes(left - n)));
            (n, false)
        }
        Trigger::Bytes(left) => {
            budget.left = None;
            (left.min(n), true)
        }
        Trigger::Chunks(1) => {
            budget.left = None;
            (0, true)
        }
        Trigger::Chunks(k) => {
            budget.left = Some((cut, Trigger::Chunks(k - 1)));
            (n, false)
        }
    }
}
