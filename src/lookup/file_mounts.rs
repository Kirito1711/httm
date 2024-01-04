//       ___           ___           ___           ___
//      /\__\         /\  \         /\  \         /\__\
//     /:/  /         \:\  \        \:\  \       /::|  |
//    /:/__/           \:\  \        \:\  \     /:|:|  |
//   /::\  \ ___       /::\  \       /::\  \   /:/|:|__|__
//  /:/\:\  /\__\     /:/\:\__\     /:/\:\__\ /:/ |::::\__\
//  \/__\:\/:/  /    /:/  \/__/    /:/  \/__/ \/__/~~/:/  /
//       \::/  /    /:/  /        /:/  /            /:/  /
//       /:/  /     \/__/         \/__/            /:/  /
//      /:/  /                                    /:/  /
//      \/__/                                     \/__/
//
// Copyright (c) 2023, Robert Swinford <robert.swinford<...at...>gmail.com>
//
// For the full copyright and license information, please view the LICENSE file
// that was distributed with this source code.

use crate::config::generate::MountDisplay;
use crate::library::results::{HttmError, HttmResult};
use crate::lookup::versions::ProximateDatasetAndOptAlts;
use crate::GLOBAL_CONFIG;
use hashbrown::HashSet;
use rayon::prelude::*;
use std::ops::Deref;

#[derive(Debug)]
pub struct MountsForFiles<'a> {
    inner: HashSet<ProximateDatasetAndOptAlts<'a>>,
    mount_display: &'a MountDisplay,
}

impl<'a> Deref for MountsForFiles<'a> {
    type Target = HashSet<ProximateDatasetAndOptAlts<'a>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> MountsForFiles<'a> {
    pub fn mount_display(&self) -> &'a MountDisplay {
        self.mount_display
    }

    pub fn new(mount_display: &'a MountDisplay) -> HttmResult<Self> {
        // we only check for phantom files in "mount for file" mode because
        // people should be able to search for deleted files in other modes
        let set: HashSet<ProximateDatasetAndOptAlts> = GLOBAL_CONFIG
            .paths
            .par_iter()
            .filter_map(|pd| match ProximateDatasetAndOptAlts::new(pd) {
                Ok(prox_opt_alts) => Some(prox_opt_alts),
                Err(_) => {
                    eprintln!(
                        "WARN: Filesystem upon which the path resides is not supported: {:?}",
                        pd.path_buf
                    );
                    None
                }
            })
            .map(|prox_opt_alts| {
                let count = prox_opt_alts.datasets_of_interest().count();

                if prox_opt_alts.pathdata.metadata.is_none() && count == 0 {
                    eprintln!(
                        "WARN: Input file may have never existed: {:?}",
                        prox_opt_alts.pathdata.path_buf
                    );
                }

                prox_opt_alts
            })
            .collect();

        // this is disjunctive instead of conjunctive, like the error re: versions
        // this is because I think the appropriate behavior when a path DNE is to error when requesting a mount
        // whereas re: versions, a file which DNE may still have snapshot versions
        if set
            .iter()
            .all(|prox| prox.datasets_of_interest().count() == 0)
            || set.iter().all(|prox| prox.pathdata.metadata.is_none())
        {
            return Err(HttmError::new(
                "httm could either not find any mounts for the path/s specified, or all the path do not exist, so, umm, 🤷? Please try another path.",
            )
            .into());
        }

        Ok(Self {
            inner: set,
            mount_display,
        })
    }
}
