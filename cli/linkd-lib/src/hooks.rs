// Copyright © 2022 The Radicle Link Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io;

use librad::{
    git::hooks::{self, hooks, Notification},
    git_ext::Oid,
    paths::Paths,
};
use tokio::{process::Child, sync::mpsc};
use tokio_stream::wrappers::ReceiverStream;

pub struct Hooks {
    stream: ReceiverStream<Notification<Oid>>,
    hooks: hooks::Hooks<Child>,
}

impl Hooks {
    pub async fn new(
        paths: &Paths,
        config: hooks::Config,
    ) -> io::Result<(mpsc::Sender<Notification<Oid>>, Self)> {
        let (sx, rx) = mpsc::channel(config.hook.buffer.size);
        let hooks = hooks(paths, config).await?;
        Ok((
            sx,
            Self {
                stream: ReceiverStream::new(rx),
                hooks,
            },
        ))
    }

    pub async fn run(self) -> anyhow::Result<()> {
        self.hooks.run(self.stream).await;
        Ok(())
    }
}
