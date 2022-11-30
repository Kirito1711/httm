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

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    path::PathBuf,
    process::Command as ExecProcess,
};

use proc_mounts::MountIter;
use rayon::iter::Either;
use rayon::prelude::*;
use which::which;

use crate::library::results::{HttmError, HttmResult};
use crate::library::utility::get_fs_type_from_hidden_dir;
use crate::parse::aliases::FilesystemType;
use crate::parse::snaps::MapOfSnaps;
use crate::ZFS_SNAPSHOT_DIRECTORY;

pub const ZFS_FSTYPE: &str = "zfs";
pub const BTRFS_FSTYPE: &str = "btrfs";
pub const SMB_FSTYPE: &str = "smbfs";
pub const NFS_FSTYPE: &str = "nfs";
pub const AFP_FSTYPE: &str = "afpfs";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MountType {
    Local,
    Network,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetMetadata {
    pub name: String,
    pub fs_type: FilesystemType,
    pub mount_type: MountType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterDirs {
    pub dirs_set: BTreeSet<PathBuf>,
    pub opt_max_depth: Option<usize>,
}

pub type MapOfDatasets = BTreeMap<PathBuf, DatasetMetadata>;

pub struct BaseFilesystemInfo {
    pub map_of_datasets: MapOfDatasets,
    pub map_of_snaps: MapOfSnaps,
    pub filter_dirs: FilterDirs,
}

impl BaseFilesystemInfo {
    // divide by the type of system we are on
    // Linux allows us the read proc mounts
    pub fn new() -> HttmResult<Self> {
        let (map_of_datasets, filter_dirs) = if cfg!(target_os = "linux") {
            Self::from_proc_mounts()?
        } else {
            Self::from_mount_cmd()?
        };

        let map_of_snaps = MapOfSnaps::new(&map_of_datasets)?;

        Ok(BaseFilesystemInfo {
            map_of_datasets,
            map_of_snaps,
            filter_dirs,
        })
    }

    // parsing from proc mounts is both faster and necessary for certain btrfs features
    // for instance, allows us to read subvolumes mounts, like "/@" or "/@home"
    fn from_proc_mounts() -> HttmResult<(MapOfDatasets, FilterDirs)> {
        let (map_of_datasets, dirs_set): (MapOfDatasets, BTreeSet<PathBuf>) = MountIter::new()?
            .par_bridge()
            .flatten()
            // but exclude snapshot mounts.  we want only the raw filesystems
            .filter(|mount_info| {
                !mount_info
                    .dest
                    .to_string_lossy()
                    .contains(ZFS_SNAPSHOT_DIRECTORY)
            })
            .partition_map(|mount_info| match &mount_info.fstype.as_str() {
                &ZFS_FSTYPE => Either::Left((
                    mount_info.dest,
                    DatasetMetadata {
                        name: mount_info.source.to_string_lossy().into_owned(),
                        fs_type: FilesystemType::Zfs,
                        mount_type: MountType::Local,
                    },
                )),
                &SMB_FSTYPE | &AFP_FSTYPE | &NFS_FSTYPE => {
                    match get_fs_type_from_hidden_dir(&mount_info.dest) {
                        Ok(FilesystemType::Zfs) => Either::Left((
                            mount_info.dest,
                            DatasetMetadata {
                                name: mount_info.source.to_string_lossy().into_owned(),
                                fs_type: FilesystemType::Zfs,
                                mount_type: MountType::Network,
                            },
                        )),
                        Ok(FilesystemType::Btrfs) => Either::Left((
                            mount_info.dest,
                            DatasetMetadata {
                                name: mount_info.source.to_string_lossy().into_owned(),
                                fs_type: FilesystemType::Btrfs,
                                mount_type: MountType::Network,
                            },
                        )),
                        Err(_) => Either::Right(mount_info.dest),
                    }
                }
                &BTRFS_FSTYPE => {
                    let keyed_options: BTreeMap<String, String> = mount_info
                        .options
                        .par_iter()
                        .filter(|line| line.contains('='))
                        .filter_map(|line| {
                            line.split_once(&"=")
                                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                        })
                        .collect();

                    let name = match keyed_options.get("subvol") {
                        Some(subvol) => subvol.clone(),
                        None => mount_info.source.to_string_lossy().into_owned(),
                    };

                    let fs_type = FilesystemType::Btrfs;

                    let mount_type = MountType::Local;

                    Either::Left((
                        mount_info.dest,
                        DatasetMetadata {
                            name,
                            fs_type,
                            mount_type,
                        },
                    ))
                }
                _ => Either::Right(mount_info.dest),
            });

        let opt_max_depth = Self::get_filter_dirs_max_depth(&dirs_set);

        let filter_dirs = FilterDirs {
            dirs_set,
            opt_max_depth,
        };

        if map_of_datasets.is_empty() {
            Err(HttmError::new("httm could not find any valid datasets on the system.").into())
        } else {
            Ok((map_of_datasets, filter_dirs))
        }
    }

    // old fashioned parsing for non-Linux systems, nearly as fast, works everywhere with a mount command
    // both methods are much faster than using zfs command
    fn from_mount_cmd() -> HttmResult<(MapOfDatasets, FilterDirs)> {
        fn parse(mount_command: &Path) -> HttmResult<(MapOfDatasets, BTreeSet<PathBuf>)> {
            let command_output =
                std::str::from_utf8(&ExecProcess::new(mount_command).output()?.stdout)?.to_owned();

            // parse "mount" for filesystems and mountpoints
            let (map_of_datasets, dirs_set): (MapOfDatasets, BTreeSet<PathBuf>) = command_output
                .par_lines()
                // but exclude snapshot mounts.  we want the raw filesystem names.
                .filter(|line| !line.contains(ZFS_SNAPSHOT_DIRECTORY))
                // where to split, to just have the src and dest of mounts
                .filter_map(|line|
                    // GNU Linux mount output
                    if line.contains("type") {
                        line.split_once(&" type")
                    // Busybox and BSD mount output
                    } else {
                        line.split_once(&" (")
                    }
                )
                .map(|(filesystem_and_mount,_)| filesystem_and_mount )
                // mount cmd includes and " on " between src and dest of mount
                .filter_map(|filesystem_and_mount| filesystem_and_mount.split_once(&" on "))
                .map(|(filesystem, mount)| (filesystem.to_owned(), PathBuf::from(mount)))
                // sanity check: does the filesystem exist and have a ZFS hidden dir? if not, filter it out
                // and flip around, mount should key of key/value
                .partition_map(|(filesystem, mount)| {
                    match get_fs_type_from_hidden_dir(&mount) {
                        Ok(FilesystemType::Zfs) => {
                            Either::Left((mount, DatasetMetadata {
                                name: filesystem,
                                fs_type: FilesystemType::Zfs,
                                mount_type: MountType::Local
                            }))
                        },
                        Ok(FilesystemType::Btrfs) => {
                            Either::Left((mount, DatasetMetadata{
                                name: filesystem,
                                fs_type: FilesystemType::Btrfs,
                                mount_type: MountType::Local
                            }))
                        },
                        Err(_) => {
                            Either::Right(mount)
                        }
                    }
                });

            if map_of_datasets.is_empty() {
                Err(HttmError::new("httm could not find any valid datasets on the system.").into())
            } else {
                Ok((map_of_datasets, dirs_set))
            }
        }

        // do we have the necessary commands for search if user has not defined a snap point?
        // if so run the mount search, if not print some errors
        if let Ok(mount_command) = which("mount") {
            let (map_of_datasets, dirs_set) = parse(&mount_command)?;

            let opt_max_depth = Self::get_filter_dirs_max_depth(&dirs_set);

            let filter_dirs = FilterDirs {
                dirs_set,
                opt_max_depth,
            };

            Ok((map_of_datasets, filter_dirs))
        } else {
            Err(HttmError::new(
                "'mount' command not be found. Make sure the command 'mount' is in your path.",
            )
            .into())
        }
    }

    fn get_filter_dirs_max_depth(dirs_set: &BTreeSet<PathBuf>) -> Option<usize> {
        dirs_set.par_iter().map(|dir| dir.iter().count()).max()
    }
}
