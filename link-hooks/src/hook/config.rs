// Copyright © 2022 The Radicle Link Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use core::fmt;
use std::{str::FromStr, time::Duration};

#[derive(Clone, Copy, Debug, Default)]
pub struct Config {
    /// Configuration for the set of [`super::Hooks`]
    pub hook: Hook,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hook {
    /// The buffer size for the hook's internal channel.
    pub buffer: Buffer,
    /// The duration to wait for a hook to complete after the
    /// end-of-transmission message before it is forcefully killed.
    pub timeout: Timeout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Buffer {
    pub size: usize,
}

impl Default for Buffer {
    fn default() -> Self {
        Self { size: 10 }
    }
}

impl FromStr for Buffer {
    type Err = <usize as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(|size| Self { size })
    }
}

impl fmt::Display for Buffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.size)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timeout {
    pub duration: Duration,
}

impl Default for Timeout {
    fn default() -> Self {
        Self {
            duration: Duration::from_secs(2),
        }
    }
}

impl FromStr for Timeout {
    type Err = <usize as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(|secs| Self {
            duration: Duration::from_secs(secs),
        })
    }
}

impl fmt::Display for Timeout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.duration.as_secs())
    }
}
