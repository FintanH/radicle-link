// Copyright © 2022 The Radicle Link Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{collections::HashMap, path::PathBuf, time::Duration};

use futures::{future::BoxFuture, stream::FuturesUnordered, FutureExt, StreamExt as _};
use multihash::Multihash;
use tokio::sync::{mpsc, oneshot};

use link_identities::urn::HasProtocol;

use super::{Data, Display, Track};

/// End of transimission character.
pub const EOT: u8 = 0x04;

/// A notification sent by the notifying process to the set of hook processes.
pub enum Notification<R> {
    Track(Track<R>),
    Data(Data<R>),
}

pub struct Hooks<P: Handle, R> {
    rx: mpsc::Receiver<Notification<R>>,
    data_hooks: Vec<Hook<P>>,
    track_hooks: Vec<Hook<P>>,
    config: Config,
}

pub struct Config {
    pub hook: config::Hook,
    pub notifier: config::Notifier,
}

pub mod config {
    use std::time::Duration;

    #[derive(Debug, Clone, Copy)]
    pub struct Hook {
        pub buffer: usize,
        pub timeout: Duration,
    }

    impl Default for Hook {
        fn default() -> Self {
            Self {
                buffer: 10,
                timeout: Duration::from_secs(2),
            }
        }
    }

    #[derive(Debug, Clone, Copy)]
    pub struct Notifier {
        pub buffer: usize,
    }

    impl Default for Notifier {
        fn default() -> Self {
            Self { buffer: 10 }
        }
    }
}

impl<P: Handle + Send + Sync + 'static, R> Hooks<P, R>
where
    R: Clone + HasProtocol + std::fmt::Display + Send + Sync + 'static,
    for<'a> &'a R: Into<Multihash>,
{
    pub fn new(
        config: Config,
        data_hooks: Vec<Hook<P>>,
        track_hooks: Vec<Hook<P>>,
    ) -> (Self, mpsc::Sender<Notification<R>>) {
        let (tx, rx) = mpsc::channel(config.notifier.buffer);
        (
            Self {
                rx,
                data_hooks,
                track_hooks,
                config,
            },
            tx,
        )
    }

    pub async fn run(mut self, stop: oneshot::Receiver<()>) {
        use senders::{Event, Senders};

        let mut routines = FuturesUnordered::new();
        let mut data_senders: Senders<Data<R>> = Senders::new(Event::Data);
        let mut track_senders: Senders<Track<R>> = Senders::new(Event::Track);

        for hook in self.data_hooks {
            let path = hook.path.clone();
            let (sender, routine) = hook.start(self.config.hook);
            data_senders.insert(path, sender);
            routines.push(routine);
        }
        for hook in self.track_hooks {
            let path = hook.path.clone();
            let (sender, routine) = hook.start(self.config.hook);
            track_senders.insert(path, sender);
            routines.push(routine);
        }
        futures::pin_mut!(stop);
        let mut stop = stop.fuse();
        loop {
            futures::select! {
                failed_hook_path = routines.next().fuse() => {
                    if let Some(failed_hook_path) = failed_hook_path {
                        data_senders.remove(&failed_hook_path);
                        track_senders.remove(&failed_hook_path);
                    } else {
                        tracing::error!("all hook routines have stopped");
                        break;
                    }
                }
                n = self.rx.recv().fuse() => {
                    match n {
                        Some(Notification::Data(d)) => data_senders.send(d),
                        Some(Notification::Track(t)) => track_senders.send(t),
                        None => break,
                    }
                },
                _ = stop => {
                    tracing::info!("hook routines shutting down");
                    break;
                }
            }
        }

        // Send EOTs to all senders
        data_senders.eot().await;
        track_senders.eot().await;

        // Wait for routines to complete
        for routine in routines {
            let path = routine.await;
            tracing::info!(hook = %path.display(), "hook finished");
        }
    }
}

/// A communication medium for a hook process.
///
/// # Cancel Safety
///
/// Since the cancel safety is based on the implementing data type of `Handle`,
/// it should be assumed that the methods are *not* cancel safe.
#[async_trait]
pub trait Handle: Sized {
    type SpawnError: std::error::Error + Send + Sync + 'static;
    type WriteError: std::error::Error + Send + Sync + 'static;
    type DieError: std::error::Error + Send + Sync + 'static;

    /// Spawn a new hook process where `path` points to the hook executable.
    async fn spawn(path: PathBuf) -> Result<Self, Self::SpawnError>;

    /// Write data to the hook process.
    async fn write(&mut self, bs: &[u8]) -> Result<(), Self::WriteError>;

    /// Wait for the hook process to finish, or kill after `duration`.
    async fn wait_or_kill(&mut self, duration: Duration) -> Result<(), Self::DieError>;
}

/// A spawned hook process.
pub struct Hook<P: Handle> {
    path: PathBuf,
    child: P,
}

pub enum HookMessage<T> {
    /// End of transmission message.
    EOT,
    /// The payload to be sent to a hook, usually [`Data`] or [`Track`].
    Payload(T),
}

impl<T> From<T> for HookMessage<T> {
    fn from(t: T) -> Self {
        Self::Payload(t)
    }
}

impl<P: Handle + Send + Sync + 'static> Hook<P> {
    pub fn new(path: PathBuf, child: P) -> Self {
        Self { path, child }
    }

    pub fn start<'a, D>(
        mut self,
        config: config::Hook,
    ) -> (mpsc::Sender<HookMessage<D>>, BoxFuture<'a, PathBuf>)
    where
        D: Display + Send + Sync + 'static,
    {
        let (sx, mut rx) = mpsc::channel::<HookMessage<D>>(config.buffer);
        let routine = async move {
            while let Some(msg) = rx.recv().await {
                match msg {
                    HookMessage::EOT => {
                        if let Err(err) = self.write(&[EOT]).await {
                            tracing::warn!(path = %self.path.display(), err = %err, "failed to write EOT to hook");
                        }
                        if let Err(err) = self.wait_or_kill(config.timeout).await {
                            tracing::warn!(path = %self.path.display(), err = %err, "failed to terminate hook");
                        }
                        return self.path;
                    },
                    HookMessage::Payload(msg) => {
                        if let Err(err) = self.write(msg.display().as_bytes()).await {
                            tracing::warn!(path = %self.path.display(), err = %err, "failed to write to hook");
                            return self.path;
                        }
                    }
                }
            }
            self.path
        }.boxed();
        (sx, routine)
    }
}

#[async_trait]
impl<P> Handle for Hook<P>
where
    P: Handle + Send + Sync + 'static,
{
    type WriteError = P::WriteError;
    type SpawnError = P::SpawnError;
    type DieError = P::DieError;

    async fn spawn(path: PathBuf) -> Result<Self, Self::SpawnError> {
        Ok(Self {
            path: path.clone(),
            child: P::spawn(path).await?,
        })
    }

    async fn write(&mut self, bs: &[u8]) -> Result<(), Self::WriteError> {
        self.child.write(bs).await
    }

    async fn wait_or_kill(&mut self, duration: Duration) -> Result<(), Self::DieError> {
        self.child.wait_or_kill(duration).await
    }
}

pub(super) mod senders {
    use super::*;

    #[derive(Debug)]
    pub enum Event {
        Track,
        Data,
    }

    pub struct Senders<P> {
        senders: HashMap<PathBuf, mpsc::Sender<HookMessage<P>>>,
        kind: Event,
    }

    impl<P> Senders<P> {
        pub fn new(kind: Event) -> Self {
            Self {
                senders: HashMap::new(),
                kind,
            }
        }

        pub fn insert(&mut self, path: PathBuf, sender: mpsc::Sender<HookMessage<P>>) {
            self.senders.insert(path, sender);
        }

        pub fn remove(&mut self, path: &PathBuf) {
            self.senders.remove(path);
        }

        pub fn send(&self, p: P)
        where
            P: Clone,
        {
            for (path, sender) in self.senders.iter() {
                if let Err(_) = sender.try_send(p.clone().into()) {
                    tracing::warn!(hook=%path.display(), kind=?self.kind, "dropping message for hook which is running too slowly");
                }
            }
        }

        pub async fn eot(&self) {
            for sender in self.senders.values() {
                sender.send(HookMessage::EOT).await.ok();
            }
        }
    }
}

mod tokio_impl {
    use std::{io, path::PathBuf, process::Stdio, time::Duration};
    use tokio::{
        io::AsyncWriteExt,
        process::{Child, Command},
    };

    use super::Handle;

    #[async_trait]
    impl Handle for Child {
        type WriteError = io::Error;
        type SpawnError = io::Error;
        type DieError = io::Error;

        async fn spawn(path: PathBuf) -> Result<Self, Self::SpawnError> {
            let child = Command::new(path.clone()).stdin(Stdio::piped()).spawn()?;
            Ok(child)
        }

        async fn write(&mut self, bs: &[u8]) -> Result<(), Self::WriteError> {
            self.stdin
                .as_mut()
                .expect("BUG: stdin was not set up for subprocess")
                .write_all(bs)
                .await
        }

        async fn wait_or_kill(&mut self, duration: Duration) -> Result<(), Self::DieError> {
            if let Err(_) = tokio::time::timeout(duration, self.wait()).await {
                self.kill().await
            } else {
                Ok(())
            }
        }
    }
}
