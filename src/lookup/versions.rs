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

use crate::config::generate::{Config, LastSnapMode, ListSnapsOfType};
use crate::data::paths::{CompareVersionsContainer, PathData};
use crate::library::results::{HttmError, HttmResult};
use crate::GLOBAL_CONFIG;
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionsMap {
    inner: BTreeMap<PathData, Vec<PathData>>,
}

impl From<BTreeMap<PathData, Vec<PathData>>> for VersionsMap {
    fn from(map: BTreeMap<PathData, Vec<PathData>>) -> Self {
        Self { inner: map }
    }
}

impl Deref for VersionsMap {
    type Target = BTreeMap<PathData, Vec<PathData>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for VersionsMap {
    fn deref_mut(&mut self) -> &mut BTreeMap<PathData, Vec<PathData>> {
        &mut self.inner
    }
}

impl VersionsMap {
    pub fn new(config: &Config, path_set: &[PathData]) -> HttmResult<VersionsMap> {
        let all_snap_versions: BTreeMap<PathData, Vec<PathData>> = path_set
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
                // don't want to flatten this iter here b/c
                // we want to keep these values with this key
                let key = prox_opt_alts.pathdata.clone();
                let values: Vec<PathData> = prox_opt_alts
                    .into_search_bundles()
                    .par_bridge()
                    .flat_map(|relative_path_snap_mounts| {
                        relative_path_snap_mounts.versions_processed(&config.uniqueness)
                    })
                    .collect();

                if key.metadata.is_none() && values.is_empty() {
                    eprintln!(
                        "WARN: Input file may have never existed: {:?}",
                        key.path_buf
                    );
                }

                (key, values)
            })
            .collect();

        let mut versions_map: VersionsMap = all_snap_versions.into();

        // check if all files (snap and live) do not exist, if this is true, then user probably messed up
        // and entered a file that never existed (that is, perhaps a wrong file name)?
        if versions_map.values().all(std::vec::Vec::is_empty)
            && versions_map
                .keys()
                .all(|pathdata| pathdata.metadata.is_none())
        {
            return Err(HttmError::new(
                "httm could find neither a live version, nor any snapshot version for all the specified paths, so, umm, 🤷? Please try another file.",
            )
            .into());
        }

        // process last snap mode after omit_ditto
        if config.opt_omit_ditto {
            versions_map.omit_ditto()
        }

        if let Some(last_snap_mode) = &config.opt_last_snap {
            versions_map.last_snap(last_snap_mode)
        }

        Ok(versions_map)
    }

    pub fn is_live_version_redundant(live_pathdata: &PathData, snaps: &[PathData]) -> bool {
        if let Some(last_snap) = snaps.last() {
            return last_snap.metadata == live_pathdata.metadata;
        }

        false
    }

    fn omit_ditto(&mut self) {
        self.iter_mut().for_each(|(pathdata, snaps)| {
            // process omit_ditto before last snap
            if Self::is_live_version_redundant(pathdata, snaps) {
                snaps.pop();
            }
        });
    }

    fn last_snap(&mut self, last_snap_mode: &LastSnapMode) {
        self.iter_mut().for_each(|(pathdata, snaps)| {
            *snaps = match snaps.last() {
                // if last() is some, then should be able to unwrap pop()
                Some(last) => match last_snap_mode {
                    LastSnapMode::Any => vec![last.to_owned()],
                    LastSnapMode::DittoOnly if pathdata.metadata == last.metadata => {
                        vec![last.to_owned()]
                    }
                    LastSnapMode::NoDittoExclusive | LastSnapMode::NoDittoInclusive
                        if pathdata.metadata != last.metadata =>
                    {
                        vec![last.to_owned()]
                    }
                    _ => Vec::new(),
                },
                None => match last_snap_mode {
                    LastSnapMode::Without | LastSnapMode::NoDittoInclusive => {
                        vec![pathdata.clone()]
                    }
                    _ => Vec::new(),
                },
            };
        });
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ProximateDatasetAndOptAlts<'a> {
    pub pathdata: &'a PathData,
    pub proximate_dataset: &'a Path,
    pub relative_path: &'a Path,
    pub opt_alts: Option<&'a Vec<PathBuf>>,
}

impl<'a> ProximateDatasetAndOptAlts<'a> {
    pub fn new(pathdata: &'a PathData) -> HttmResult<Self> {
        // here, we take our file path and get back possibly multiple ZFS dataset mountpoints
        // and our most proximate dataset mount point (which is always the same) for
        // a single file
        //
        // we ask a few questions: has the location been user defined? if not, does
        // the user want all local datasets on the system, including replicated datasets?
        // the most common case is: just use the most proximate dataset mount point as both
        // the dataset of interest and most proximate ZFS dataset
        //
        // why? we need both the dataset of interest and the most proximate dataset because we
        // will compare the most proximate dataset to our our canonical path and the difference
        // between ZFS mount point and the canonical path is the path we will use to search the
        // hidden snapshot dirs
        let opt_alias = pathdata.alias();

        let proximate_dataset = opt_alias
            .as_ref()
            .map(|alias| alias.proximate_dataset)
            .map_or_else(|| pathdata.proximate_dataset(), Ok)?;

        let relative_path = opt_alias
            .as_ref()
            .and_then(|alias| alias.relative_path)
            .map_or_else(|| pathdata.relative_path(proximate_dataset), Ok)?;

        let opt_alts = GLOBAL_CONFIG
            .dataset_collection
            .opt_map_of_alts
            .as_ref()
            .and_then(|map_of_alts| map_of_alts.get(proximate_dataset))
            .and_then(|alt_metadata| alt_metadata.opt_datasets_of_interest.as_ref());

        Ok(Self {
            pathdata,
            proximate_dataset,
            relative_path,
            opt_alts,
        })
    }

    pub fn datasets_of_interest(&'a self) -> impl Iterator<Item = &'a Path> {
        let alts = self
            .opt_alts
            .as_deref()
            .into_iter()
            .flatten()
            .map(PathBuf::as_path);

        let base = [self.proximate_dataset].into_iter();

        alts.chain(base)
    }

    pub fn into_search_bundles(&'a self) -> impl Iterator<Item = RelativePathAndSnapMounts<'a>> {
        self.datasets_of_interest().flat_map(|dataset_of_interest| {
            RelativePathAndSnapMounts::new(self.pathdata, &self.relative_path, &dataset_of_interest)
        })
    }
}

#[derive(Debug, Clone)]
pub struct RelativePathAndSnapMounts<'a> {
    pub pathdata: &'a PathData,
    pub relative_path: &'a Path,
    pub snap_mounts: &'a [PathBuf],
}

impl<'a> RelativePathAndSnapMounts<'a> {
    fn new(
        pathdata: &'a PathData,
        relative_path: &'a Path,
        dataset_of_interest: &Path,
    ) -> Option<Self> {
        // building our relative path by removing parent below the snap dir
        //
        // for native searches the prefix is are the dirs below the most proximate dataset
        // for user specified dirs/aliases these are specified by the user
        let snap_mounts = GLOBAL_CONFIG
            .dataset_collection
            .map_of_snaps
            .get(dataset_of_interest)?
            .as_slice();

        Some(Self {
            pathdata,
            relative_path,
            snap_mounts,
        })
    }

    pub fn versions_processed(&'a self, uniqueness: &ListSnapsOfType) -> Vec<PathData> {
        let all_versions = self.versions_unprocessed();

        Self::sort_dedup_versions(all_versions, uniqueness)
    }

    pub fn last_version(&self) -> Option<PathData> {
        let mut sorted_versions = self.versions_processed(&ListSnapsOfType::All);

        sorted_versions.pop()
    }

    fn versions_unprocessed(&'a self) -> impl ParallelIterator<Item = PathData> + 'a {
        // get the DirEntry for our snapshot path which will have all our possible
        // snapshots, like so: .zfs/snapshots/<some snap name>/
        self
            .snap_mounts
            .par_iter()
            .map(|path| path.join(self.relative_path))
            .filter_map(|joined_path| {
                match joined_path.symlink_metadata() {
                    Ok(md) => {
                        Some(PathData::new(joined_path.as_path(), Some(md)))
                    },
                    Err(err) => {
                        match err.kind() {
                            // if we do not have permissions to read the snapshot directories
                            // fail/panic printing a descriptive error instead of flattening
                            ErrorKind::PermissionDenied => {
                                eprintln!("Error: When httm tried to find a file contained within a snapshot directory, permission was denied.  \
                                Perhaps you need to use sudo or equivalent to view the contents of this snapshot (for instance, btrfs by default creates privileged snapshots).  \
                                \nDetails: {err}");
                                std::process::exit(1)
                            },
                            // if file metadata is not found, or is otherwise not available, 
                            // continue, it simply means we do not have a snapshot of this file
                            _ => None,
                        }
                    },
                }
            })
    }

    // remove duplicates with the same system modify time and size/file len (or contents! See --uniqueness)
    #[allow(clippy::mutable_key_type)]
    fn sort_dedup_versions(
        iter: impl ParallelIterator<Item = PathData>,
        uniqueness: &ListSnapsOfType,
    ) -> Vec<PathData> {
        match uniqueness {
            ListSnapsOfType::All => iter.collect(),
            ListSnapsOfType::UniqueContents | ListSnapsOfType::UniqueMetadata => {
                let sorted_and_deduped: BTreeSet<CompareVersionsContainer> = iter
                    .map(|pd| CompareVersionsContainer::new(pd, uniqueness))
                    .collect();
                sorted_and_deduped.into_iter().map(PathData::from).collect()
            }
        }
    }
}
