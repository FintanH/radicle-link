use std::{sync::Arc, time::Duration};

use clap::Parser;
use futures::{FutureExt, StreamExt};
use librad::{
    crypto::{BoxedSignError, BoxedSigner},
    net,
    PeerId,
};
use lnk_clib::seed::{self, Seeds};
use lnk_thrussh as thrussh;
use lnk_thrussh_keys as thrussh_keys;
use tokio::{net::TcpListener, sync::mpsc};
use tracing::instrument;

use crate::peerless::Peerless;

mod args;
mod config;
mod exec_str;
mod git_subprocess;
mod hooks;
mod peerless;
mod processes;
mod server;
mod service;

#[derive(thiserror::Error, Debug)]
pub enum RunError {
    #[error("could not open storage")]
    CouldNotOpenStorage,
    #[error("no listen address was specified")]
    NoBindAddr,
    #[error("unable to bind to listen addr: {0}")]
    CouldNotBind(std::io::Error),
    #[error("unable to load server key: {0}")]
    UnableToLoadKey(Box<dyn std::error::Error>),
    #[error("unable to initialise peer")]
    Peer(#[from] net::peer::error::Init),
    #[error("error loading socket activation environment variables: {0}")]
    SocketActivation(#[from] lnk_clib::socket_activation::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub async fn main() {
    tracing_subscriber::fmt::init();
    let args = args::Args::parse();
    let spawner = Arc::new(link_async::Spawner::from_current().unwrap());
    let config = match args.into_config(spawner.clone()).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return;
        },
    };
    if let Err(e) = run(config, spawner).await {
        eprintln!("Error: {}", e);
    }
}

pub async fn run<S: librad::Signer<Error = BoxedSignError> + Clone>(
    config: config::Config<S>,
    spawner: Arc<link_async::Spawner>,
) -> Result<(), RunError> {
    tracing::info!(git=%config.paths.git_dir().display(), "starting git-server");
    // Load storage pool
    let storage_pool = Arc::new(librad::git::storage::Pool::new(
        librad::git::storage::pool::ReadWriteConfig::new(
            config.paths.clone(),
            config.signer.clone(),
            librad::git::storage::pool::Initialised::no(),
        ),
        librad::net::peer::config::UserStorage::default().pool_size,
    ));

    let peer_id = PeerId::from_signer(&config.signer);
    let peerless = Peerless::new(
        config.paths.clone(),
        BoxedSigner::new(config.signer.clone()),
    )?;
    let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
    let peer_task = spawner.spawn(peerless::routine(peerless.clone(), shutdown_rx));

    let seeds = {
        let path = config.paths.seeds_file();
        tracing::info!(seed_file=%path.display(), "loading seeds");
        let store = seed::store::FileStore::<String>::new(path)?;
        let (seeds, failures) = Seeds::load(&store, None).await?;
        for fail in &failures {
            tracing::warn!("failed to load configured seed: {}", fail);
        }
        seeds
    };

    // Create thrussh config from stored key or create a new one
    let server_key = create_or_load_key(peer_id)?;
    let mut thrussh_config = thrussh::server::Config::default();
    thrussh_config.keys.push(server_key);
    let thrussh_config = Arc::new(thrussh_config);

    // Processes thread which handles git subprocesses
    let (processes, handle) = processes::Processes::new(spawner.clone(), storage_pool.clone());

    let socket = bind_sockets(&config).await?;
    let processes_task = spawner.spawn(processes.run());
    let hooks = if config.announce_on_push {
        hooks::Hooks::announce(
            spawner.clone(),
            peerless,
            storage_pool.clone(),
            config.linkd_rpc_socket_path,
            seeds,
        )
    } else {
        hooks::Hooks::update_sigrefs(spawner.clone(), peerless, storage_pool.clone(), seeds)
    };
    let sh = server::Server::new(spawner.clone(), peer_id, handle.clone(), hooks);
    let ssh_tasks = sh.serve(&socket, thrussh_config).await;
    let server_complete = match config.linger_timeout {
        Some(d) => link_async::tasks::run_until_idle(ssh_tasks.boxed(), d).boxed(),
        None => link_async::tasks::run_forever(ssh_tasks.boxed()).boxed(),
    };

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;

    // Wait for everything to finish
    let mut processes_fused = processes_task.fuse();
    let mut server_complete = server_complete.fuse();
    let mut peer_task = peer_task.fuse();
    futures::select! {
        _ = server_complete => {
            tracing::info!("SSH server shutdown, shutting down subprocesses");
            shutdown_tx.send(()).await.ok();
            handle_shutdown(handle, server_complete, processes_fused, peer_task).await;
        },
        _ = sigterm.recv().fuse() => {
            tracing::info!("received SIGTERM, attmempting graceful shutdown");
            shutdown_tx.send(()).await.ok();
            handle_shutdown(handle, server_complete, processes_fused, peer_task).await;
        },
        _ = sigint.recv().fuse() => {
            tracing::info!("received SIGINT, attmempting graceful shutdown");
            shutdown_tx.send(()).await.ok();
            handle_shutdown(handle, server_complete, processes_fused, peer_task).await;
        },
        p = processes_fused => {
            tracing::error!("subprocesses loop terminated whilst server running");
            shutdown_tx.send(()).await.ok();
            match p {
                Ok(Ok(())) => {
                    panic!("subprocesses loop terminated for no reason");
                },
                Ok(Err(e)) => {
                    panic!("subprocesses loop terminated with error {:?}", e);
                },
                Err(link_async::JoinError::Panicked(e)) => {
                    std::panic::resume_unwind(e);
                },
                Err(link_async::JoinError::Cancelled) => {
                    panic!("subprocesses loop cancelled whilst server running");
                }
            }
        }
    }
    Ok(())
}

async fn bind_sockets<S: librad::Signer + Clone>(
    config: &config::Config<S>,
) -> Result<TcpListener, RunError> {
    match config.addr {
        Some(addr) => TcpListener::bind(addr)
            .await
            .map_err(RunError::CouldNotBind),
        None => {
            let socket_activated = lnk_clib::socket_activation::env_sockets()?;
            match socket_activated {
                None => Err(RunError::NoBindAddr),
                Some(mut socks) => match socks.remove("ssh") {
                    Some(lnk_clib::socket_activation::Socket::Tcp(s)) => {
                        s.set_nonblocking(true)?;
                        TcpListener::from_std(s).map_err(RunError::from)
                    },
                    _ => Err(RunError::NoBindAddr),
                },
            }
        },
    }
}

#[instrument]
fn create_or_load_key(peer_id: PeerId) -> Result<thrussh_keys::key::KeyPair, RunError> {
    let dirs = xdg::BaseDirectories::new().map_err(|e| RunError::UnableToLoadKey(Box::new(e)))?;
    let key_filename = format!("linkd-git/{}-ssh-key", peer_id);
    let key_path = dirs.place_state_file(&key_filename).map_err(|e| {
        tracing::error!(
            ?key_filename,
            "unable to get state file path for linkd ssh key"
        );
        RunError::UnableToLoadKey(Box::new(e))
    })?;
    if key_path.exists() {
        tracing::info!("found server key");
        let raw = std::fs::read(key_path).map_err(|e| {
            tracing::error!(err=?e, "unable to read linkd-git ssh key");
            RunError::UnableToLoadKey(Box::new(e))
        })?;
        let key_bytes: [u8; 64] = raw.try_into().map_err(|e| {
            tracing::error!(err=?e, "invalid key file");
            RunError::UnableToLoadKey(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "invalid key file",
            )))
        })?;
        let key = thrussh_keys::key::ed25519::SecretKey { key: key_bytes };
        Ok(thrussh_keys::key::KeyPair::Ed25519(key))
    } else {
        tracing::info!("no server key found, creating new one");
        let (_public, secret) = thrussh_keys::key::ed25519::keypair();
        std::fs::write(key_path, &secret.key).map_err(|e| {
            tracing::error!(err=?e, "error creating new key file");
            RunError::UnableToLoadKey(Box::new(e))
        })?;
        Ok(thrussh_keys::key::KeyPair::Ed25519(secret))
    }
}

async fn handle_shutdown<I, R, F>(
    handle: processes::ProcessesHandle<I, R>,
    server_complete: F,
    processes_fused: futures::future::Fuse<
        link_async::Task<Result<(), processes::ProcessRunError<server::ChannelAndSessionId>>>,
    >,
    peer_task: futures::future::Fuse<link_async::Task<()>>,
) where
    F: futures::Future<Output = ()>,
    I: std::fmt::Debug,
{
    match handle.stop().await {
        Ok(()) => {
            tracing::info!("waiting for subprocesses to finish");
            let timeout = link_async::timeout(Duration::from_secs(10), processes_fused).fuse();
            futures::pin_mut!(timeout);
            futures::select! {
                _ = server_complete.fuse() => {
                    tracing::error!("SSH server shutdown whilst waiting for processees to finish, exiting");
                },
                _ = peer_task.fuse() => {},
                timeout_res = timeout => {
                    match timeout_res {
                        Ok(Ok(Ok(()))) => {},
                        Ok(Ok(Err(e))) => {
                            tracing::error!(err=?e, "processes task completed with an error whilst shutting down");
                        },
                        Ok(Err(link_async::JoinError::Cancelled)) => {
                            tracing::warn!("subprocesses cancelled whilst waiting to stop");
                        },
                        Ok(Err(link_async::JoinError::Panicked(e))) => {
                            std::panic::resume_unwind(e);
                        },
                        Err(link_async::Elapsed) => {
                            tracing::warn!("timed out waiting for subprocesses to finish");
                        },
                    }
                }
            }
        },
        Err(e) => {
            tracing::error!(err=?e, "error sending shutdown to subprocesses");
        },
    }
}
