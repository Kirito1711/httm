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
// (c) Robert Swinford <robert.swinford<...at...>gmail.com>
//
// For the full copyright and license information, please view the LICENSE file
// that was distributed with this source code.

use std::ops::Deref;

use crate::HashbrownMap;
use rayon::prelude::*;

use crate::config::generate::{Config, MountDisplay};
use crate::data::paths::PathData;
use crate::library::iter_extensions::HttmIter;
use crate::lookup::versions::{MostProximateAndOptAlts, VersionsMap};

pub struct MountsForFiles<'a> {
    pub inner: HashbrownMap<PathData, Vec<PathData>>,
    pub mount_display: &'a MountDisplay,
    pub config: &'a Config,
}

impl<'a> From<MountsForFiles<'a>> for VersionsMap {
    fn from(map: MountsForFiles) -> Self {
        map.inner.into()
    }
}

impl<'a> Deref for MountsForFiles<'a> {
    type Target = HashbrownMap<PathData, Vec<PathData>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> MountsForFiles<'a> {
    pub fn new(config: &'a Config, mount_display: &'a MountDisplay) -> Self {
        // we only check for phantom files in "mount for file" mode because
        // people should be able to search for deleted files in other modes
        let (non_phantom_files, phantom_files): (Vec<PathData>, Vec<PathData>) = config
            .paths
            .clone()
            .into_par_iter()
            .partition(|pathdata| pathdata.metadata.is_some());

        if !phantom_files.is_empty() {
            eprintln!(
                "Error: httm was unable to determine mount locations for all input files, \
            because the following files do not appear to exist: "
            );

            phantom_files
                .iter()
                .for_each(|pathdata| eprintln!("{:?}", pathdata.path_buf));
        }

        MountsForFiles::from_raw_paths(config, &non_phantom_files, mount_display)
    }

    pub fn from_raw_paths(
        config: &'a Config,
        raw_vec: &[PathData],
        mount_display: &'a MountDisplay,
    ) -> Self {
        let map: HashbrownMap<PathData, Vec<PathData>> = raw_vec
            .iter()
            .map(|pathdata| {
                let datasets: Vec<MostProximateAndOptAlts> = config
                    .dataset_collection
                    .snaps_selected_for_search
                    .get_value()
                    .iter()
                    .flat_map(|dataset_type| {
                        MostProximateAndOptAlts::new(config, pathdata, dataset_type)
                    })
                    .collect();
                (pathdata.clone(), datasets)
            })
            .into_group_map_by(|(pathdata, _snap_types_for_search)| pathdata.clone())
            .into_iter()
            .map(|(pathdata, vec_snap_types_for_search)| {
                let datasets: Vec<PathData> = vec_snap_types_for_search
                    .into_iter()
                    .flat_map(|(_proximate_mount, snap_types_for_search)| snap_types_for_search)
                    .flat_map(|snap_types_for_search| {
                        snap_types_for_search.get_datasets_of_interest()
                    })
                    .map(|path| PathData::from(path.as_path()))
                    .rev()
                    .collect();
                (pathdata, datasets)
            })
            .collect();

        Self {
            inner: map,
            mount_display,
            config,
        }
    }
}
