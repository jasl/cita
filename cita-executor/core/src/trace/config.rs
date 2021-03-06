// Copyright 2015-2017 Parity Technologies (UK) Ltd.
// This file is part of Parity.

// This software is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This software is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity.  If not, see <http://www.gnu.org/licenses/>.

//! Traces config.

use bloomchain::Config as BloomConfig;

/// Traces config.
#[derive(Debug, PartialEq, Clone)]
pub struct Config {
    /// Indicates if tracing should be enabled or not.
    /// If it's None, it will be automatically configured.
    pub enabled: bool,
    /// Traces blooms configuration.
    pub blooms: BloomConfig,
    /// Preferef cache-size.
    pub pref_cache_size: usize,
    /// Max cache-size.
    pub max_cache_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: false,
            blooms: BloomConfig {
                levels: 3,
                elements_per_index: 16,
            },
            pref_cache_size: 15 * 1024 * 1024,
            max_cache_size: 20 * 1024 * 1024,
        }
    }
}
