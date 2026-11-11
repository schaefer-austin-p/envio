//! A byte-for-byte TCP proxy in front of Stalwart that a test can cut:
//! every open connection drops, and new ones are refused while it is down.
//! TLS passes through untouched, so the server's pinned certificate holds.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Default)]
struct State {
    open: Mutex<Vec<TcpStream>>,
    down: AtomicBool,
    accepted: AtomicUsize,
    /// Milliseconds each batch from the server waits before it is passed on.
    delay_ms: AtomicU64,
    /// Batches that waited for the delay.
    held: AtomicUsize,
}

pub struct Proxy {
    pub port: u16,
    state: Arc<State>,
}

impl Proxy {
    /// Listens on a free local port and forwards to `host:target`.
    pub fn start(host: &str, target: u16) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(State::default());
        let shared = Arc::clone(&state);
        let host = host.to_owned();
        thread::spawn(move || {
            for client in listener.incoming().flatten() {
                if shared.down.load(Ordering::SeqCst) {
                    continue; // dropped: the client sees a closed connection
                }
                shared.accepted.fetch_add(1, Ordering::SeqCst);
                let Ok(server) = TcpStream::connect((host.as_str(), target)) else {
                    continue;
                };
                if let (Ok(a), Ok(b)) = (client.try_clone(), server.try_clone())
                    && let Ok(mut open) = shared.open.lock()
                {
                    open.push(a);
                    open.push(b);
                }
                pipe(
                    client.try_clone().unwrap(),
                    server.try_clone().unwrap(),
                    None,
                );
                pipe(server, client, Some(Arc::clone(&shared)));
            }
        });
        Self { port, state }
    }

    /// Drops every open connection and refuses new ones until `up`.
    pub fn cut(&self) {
        self.state.down.store(true, Ordering::SeqCst);
        for stream in self.state.open.lock().unwrap().drain(..) {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }

    /// Connections clients have opened through the proxy so far.
    pub fn connections(&self) -> usize {
        self.state.accepted.load(Ordering::SeqCst)
    }

    /// Holds each batch the server sends back for `delay` (open connections
    /// too); `Duration::ZERO` ends it.
    pub fn slow(&self, delay: Duration) {
        let ms = u64::try_from(delay.as_millis()).unwrap();
        self.state.delay_ms.store(ms, Ordering::SeqCst);
    }

    /// How many batches have waited for the delay so far.
    pub fn held(&self) -> usize {
        self.state.held.load(Ordering::SeqCst)
    }

    pub fn up(&self) {
        self.state.down.store(false, Ordering::SeqCst);
    }
}

fn pipe(mut from: TcpStream, mut to: TcpStream, slow: Option<Arc<State>>) {
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = from.read(&mut buf) {
            if let Some(state) = &slow {
                let ms = state.delay_ms.load(Ordering::SeqCst);
                if ms > 0 {
                    state.held.fetch_add(1, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(ms));
                }
            }
            if n == 0 || to.write_all(&buf[..n]).is_err() {
                break;
            }
        }
        let _ = to.shutdown(Shutdown::Both);
        let _ = from.shutdown(Shutdown::Both);
    });
}
