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

use std::collections::BTreeMap;
use std::ops::Deref;

use crate::config::generate::{Config, PrintMode};
use crate::display::format::{NOT_SO_PRETTY_FIXED_WIDTH_PADDING, QUOTATION_MARKS_LEN};

pub struct PrintableMap {
    pub inner: BTreeMap<String, Vec<String>>,
}

impl Deref for PrintableMap {
    type Target = BTreeMap<String, Vec<String>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl PrintableMap {
    pub fn get_map_padding(&self) -> usize {
        self.inner
            .iter()
            .map(|(key, _values)| key)
            .max_by_key(|key| key.len())
            .map_or_else(
                || QUOTATION_MARKS_LEN,
                |key| key.len() + QUOTATION_MARKS_LEN,
            )
    }

    pub fn format_as_map(&self, config: &Config) -> String {
        let padding = self.get_map_padding();

        let write_out_buffer = self
            .inner
            .iter()
            .filter(|(_key, values)| {
                if config.opt_last_snap.is_some() {
                    !values.is_empty()
                } else {
                    true
                }
            })
            .map(|(key, values)| {
                let display_path = if matches!(config.print_mode, PrintMode::FormattedNotPretty) {
                    key.to_owned()
                } else {
                    format!("\"{}\"", key)
                };

                let values_string: String = values
                    .iter()
                    .enumerate()
                    .map(|(idx, value)| {
                        if matches!(config.print_mode, PrintMode::FormattedNotPretty) {
                            format!("{}{}", NOT_SO_PRETTY_FIXED_WIDTH_PADDING, value)
                        } else if idx == 0 {
                            format!(
                                "{:<width$} : \"{}\"\n",
                                display_path,
                                value,
                                width = padding
                            )
                        } else {
                            format!("{:<width$} : \"{}\"\n", "", value, width = padding)
                        }
                    })
                    .collect::<String>();

                if matches!(config.print_mode, PrintMode::FormattedNotPretty) {
                    format!("{}:{}\n", display_path, values_string)
                } else {
                    values_string
                }
            })
            .collect();

        write_out_buffer
    }
}
