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

use crate::config::generate::{MountDisplay, PrintMode};
use crate::data::paths::PathData;
use crate::data::paths::PathDeconstruction;
use crate::data::paths::ZfsSnapPathGuard;
use crate::display_versions::format::{NOT_SO_PRETTY_FIXED_WIDTH_PADDING, QUOTATION_MARKS_LEN};
use crate::library::utility::delimiter;
use crate::{MountsForFiles, SnapNameMap, VersionsMap, GLOBAL_CONFIG};
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use std::collections::BTreeMap;
use std::ops::Deref;

#[derive(Debug)]
pub struct PrintAsMap {
    inner: BTreeMap<String, Vec<String>>,
}

impl Deref for PrintAsMap {
    type Target = BTreeMap<String, Vec<String>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl From<BTreeMap<String, Vec<String>>> for PrintAsMap {
    fn from(map: BTreeMap<String, Vec<String>>) -> Self {
        Self { inner: map }
    }
}

impl Serialize for PrintAsMap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(Some(self.inner.len()))?;
        self.inner
            .iter()
            .try_for_each(|(k, v)| state.serialize_entry(k, v))?;
        state.end()
    }
}

impl<'a> From<&MountsForFiles<'a>> for PrintAsMap {
    fn from(mounts_for_files: &MountsForFiles) -> Self {
        let inner = mounts_for_files
            .iter()
            .map(|prox| {
                let key = prox.pathdata;
                let res = prox
                    .datasets_of_interest()
                    .map(PathData::from)
                    .filter_map(|value| match ZfsSnapPathGuard::new(key) {
                        Some(spg) => match mounts_for_files.mount_display() {
                            MountDisplay::Target => spg
                                .target(prox.proximate_dataset)
                                .map(|path| path.to_path_buf()),
                            MountDisplay::Source => spg.source(None),
                            MountDisplay::RelativePath => spg
                                .relative_path(prox.proximate_dataset)
                                .ok()
                                .map(|path| path.to_path_buf()),
                        },
                        None => match mounts_for_files.mount_display() {
                            MountDisplay::Target => key.target(&value.path_buf),
                            MountDisplay::Source => key.source(Some(&value.path_buf)),
                            MountDisplay::RelativePath => key
                                .relative_path(&value.path_buf)
                                .ok()
                                .map(|path| path.to_path_buf()),
                        },
                    })
                    .map(|path| path.to_string_lossy().to_string())
                    .collect();
                (key.path_buf.to_string_lossy().to_string(), res)
            })
            .collect();
        Self { inner }
    }
}

impl From<&VersionsMap> for PrintAsMap {
    fn from(map: &VersionsMap) -> Self {
        let inner = map
            .iter()
            .map(|(key, values)| {
                let res = values
                    .iter()
                    .map(|value| value.path_buf.to_string_lossy().to_string())
                    .collect();
                (key.path_buf.to_string_lossy().to_string(), res)
            })
            .collect();
        Self { inner }
    }
}

impl From<&SnapNameMap> for PrintAsMap {
    fn from(map: &SnapNameMap) -> Self {
        let inner = map
            .iter()
            .map(|(key, value)| (key.path_buf.to_string_lossy().to_string(), value.clone()))
            .collect();
        Self { inner }
    }
}

impl std::string::ToString for PrintAsMap {
    fn to_string(&self) -> String {
        if GLOBAL_CONFIG.opt_json {
            return self.to_json();
        }

        let delimiter = delimiter();

        match &GLOBAL_CONFIG.print_mode {
            PrintMode::RawNewline | PrintMode::RawZero => {
                self.values()
                    .flatten()
                    .fold(String::new(), |mut buffer, value| {
                        buffer += format!("{value}{delimiter}").as_str();
                        buffer
                    })
            }
            PrintMode::FormattedDefault | PrintMode::FormattedNotPretty => self.format(),
        }
    }
}

impl PrintAsMap {
    pub fn map_padding(&self) -> usize {
        self.keys().max_by_key(|key| key.len()).map_or_else(
            || QUOTATION_MARKS_LEN,
            |key| key.len() + QUOTATION_MARKS_LEN,
        )
    }

    pub fn to_json(&self) -> String {
        let res = match GLOBAL_CONFIG.print_mode {
            PrintMode::FormattedNotPretty | PrintMode::RawNewline | PrintMode::RawZero => {
                serde_json::to_string(&self)
            }
            PrintMode::FormattedDefault => serde_json::to_string_pretty(&self),
        };

        match res {
            Ok(s) => {
                let delimiter = delimiter();
                format!("{s}{delimiter}")
            }
            Err(error) => {
                eprintln!("Error: {error}");
                std::process::exit(1)
            }
        }
    }

    pub fn format(&self) -> String {
        let padding = self.map_padding();

        let write_out_buffer = self
            .iter()
            .filter(|(_key, values)| {
                if GLOBAL_CONFIG.opt_last_snap.is_some() {
                    !values.is_empty()
                } else {
                    true
                }
            })
            .map(|(key, values)| {
                let display_path =
                    if matches!(&GLOBAL_CONFIG.print_mode, PrintMode::FormattedNotPretty) {
                        key.clone()
                    } else {
                        format!("\"{key}\"")
                    };

                let values_string: String = values
                    .iter()
                    .enumerate()
                    .map(|(idx, value)| {
                        if matches!(&GLOBAL_CONFIG.print_mode, PrintMode::FormattedNotPretty) {
                            format!("{NOT_SO_PRETTY_FIXED_WIDTH_PADDING}{value}")
                        } else if idx == 0 {
                            format!(
                                "{:<width$} : \"{}\"\n",
                                display_path,
                                value,
                                width = padding
                            )
                        } else {
                            format!("{:<padding$} : \"{value}\"\n", "")
                        }
                    })
                    .collect::<String>();

                if matches!(&GLOBAL_CONFIG.print_mode, PrintMode::FormattedNotPretty) {
                    format!("{display_path}:{values_string}\n")
                } else {
                    values_string
                }
            })
            .collect();

        write_out_buffer
    }
}
