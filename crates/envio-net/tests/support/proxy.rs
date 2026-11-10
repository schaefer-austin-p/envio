//! A TCP proxy that can cut connections mid-stream: the network dropping
//! out in the middle of a download, an upload or SMTP DATA.

use std::sync::{Arc, Mutex};

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

#[derive(Default)]
struct Budget {
    /// Bytes still allowed in one direction before every connection is cut.
    left: Option<(Direction, usize)>,
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
                let Ok(server) = TcpStream::connect(&target).await else {
                    continue;
                };
                tokio::spawn(pipe(client, server, shared.clone()));
            }
        });
        Self {
            port: local,
            budget,
        }
    }

    /// From now on, cut every open connection once `bytes` more have
    /// passed in `direction`. The budget is spent once: later
    /// connections pass untouched.
    pub fn cut_after(&self, direction: Direction, bytes: usize) {
        self.budget.lock().unwrap().left = Some((direction, bytes));
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
            let allowed = take(&self.budget, self.direction, n);
            if to.write_all(&buf[..allowed]).await.is_err() {
                break;
            }
            if allowed < n {
                let _ = self.cut_tx.send(true);
                return;
            }
        }
        let _ = to.shutdown().await;
    }
}

/// How many of `n` bytes may pass; spends the budget.
fn take(budget: &Mutex<Budget>, direction: Direction, n: usize) -> usize {
    let mut budget = budget.lock().unwrap();
    match budget.left {
        Some((cut, left)) if cut == direction => {
            if n <= left {
                budget.left = Some((cut, left - n));
                n
            } else {
                budget.left = None;
                left
            }
        }
        _ => n,
    }
}
