// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use structopt::StructOpt;

use librad::profile::ProfileId;

/// Management of Radicle profiles and their associated configuration data.
#[derive(Debug, StructOpt)]
pub struct Args {
    #[structopt(subcommand)]
    pub command: Command,
}

#[derive(Debug, StructOpt)]
pub enum Command {
    Create(Create),
    Get(Get),
    Set(Set),
    List(List),
    Peer(GetPeerId),
    Paths(GetPaths),
    Ssh(Ssh),
}

/// Create a new profile, generating a new secret key and initialising
/// configurations and storage.
#[derive(Debug, StructOpt)]
pub struct Create {}

/// Get a profile, defaulting to the active profile if no identifier is given.
#[derive(Debug, StructOpt)]
pub struct Get {
    /// the identifier of the profile requested
    #[structopt(long)]
    pub id: Option<ProfileId>,
}

/// Set the active profile.
#[derive(Debug, StructOpt)]
pub struct Set {
    /// the identifier to set the active profile to
    #[structopt(long)]
    pub id: ProfileId,
}

/// List all profiles that have been created
#[derive(Debug, StructOpt)]
pub struct List {}

/// Get the peer identifier associated with the provided profile identfier. If
/// no profile was provided, then the active one is used.
#[derive(Debug, StructOpt)]
pub struct GetPeerId {
    /// the identifier to look up
    #[structopt(long)]
    pub id: Option<ProfileId>,
}

/// Get the paths associated with the provided profile identfier. If no profile
/// was provided, then the active one is used.
#[derive(Debug, StructOpt)]
pub struct GetPaths {
    /// the identifier to look up    
    #[structopt(long)]
    pub id: Option<ProfileId>,
}

/// Manage the profile's key material on the ssh-agent
#[derive(Debug, StructOpt)]
pub struct Ssh {
    #[structopt(subcommand)]
    pub options: ssh::Options,
}

pub mod ssh {
    use super::*;

    #[derive(Debug, StructOpt)]
    pub enum Options {
        Add(Add),
        Rm(Rm),
        Test(Test),
    }

    /// Add the profile's associated secret key to the ssh-agent. If no profile
    /// was provided, then the active one is used.
    #[derive(Debug, StructOpt)]
    pub struct Add {
        /// the identifier to look up
        #[structopt(long)]
        pub id: Option<ProfileId>,
        /* TODO(finto): This is currently not possible to support, see https://nest.pijul.com/pijul/thrussh/discussions/52
         * /// the lifetime of the key being added to the ssh-agent, if none is
         * /// provided then agent will ask to confirm each time
         * #[structopt(long, short)]
         * pub time: Option<u32>, */
    }

    /// Remove the profile's associated secret key from the ssh-agent. If no
    /// profile was provided, then the active one is used.
    #[derive(Debug, StructOpt)]
    pub struct Rm {
        /// the identifier to look up
        #[structopt(long)]
        pub id: Option<ProfileId>,
    }

    /// Test if the profile's associated secret key is present in the ssh-agent.
    /// If no profile was provided, then the active one is used.
    #[derive(Debug, StructOpt)]
    pub struct Test {
        /// the identifier to look up
        #[structopt(long)]
        pub id: Option<ProfileId>,
    }
}
