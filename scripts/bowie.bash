#!/bin/bash

set -euf -o pipefail
#set -x

print_version() {
	printf "\
bowie $(httm --version | cut -f2 -d' ')
" 1>&2
	exit 0
}


print_usage() {
	local bowie="\e[31mbowie\e[0m"
	local httm="\e[31mhttm\e[0m"

	printf "\
$bowie is a wrapper script for $httm which displays the difference between unique snapshot versions and the live file.

USAGE:
	bowie [OPTIONS]... [file1 file2...]

OPTIONS:
	--last:
		Default mode.  Display only the difference between the last unique snapshot version and the live file.

	--all:
		Display the difference between the all unique snapshot versions and the live file.

	--help:
		Display this dialog.

	--version:
		Display script version.

" 1>&2
	exit 1
}

print_err_exit() {
	printf "%s\n" "Error: $*" 1>&2
	exit 1
}

show_all_changes() {
	local filename="$1"
	local previous_version=""

	for current_version in $(httm -n --omit-ditto "$filename"); do
	# check if initial "previous_version" needs to be set
		if [[ -z "$previous_version" ]]; then
			previous_version="$current_version"
			continue
		fi

		display_diff "$previous_version" "$current_version"

    		# set current_version to previous_version
		previous_version="$current_version"
	done
}

show_last_change() {
	local current_version="$1"
	local previous_version=""
	
	previous_version="$( httm --omit-ditto --last-snap --raw "$current_version" )"

	display_diff "$previous_version" "$current_version"
}

display_diff() {
	local previous_version="$1"
	local current_version="$2"

	# print that current version and previous version differ
	( diff -q "$previous_version" "$current_version" || true )
	# print the difference between that current version and previous_version
	( diff -T "$previous_version" "$current_version" || true )
}

exec_main() {
	local all_mode=false

	[[ "$1" != "-h" && "$1" != "--help" ]] || print_usage
	[[ "$1" != "-V" && "$1" != "--version" ]] || print_version
	[[ "$1" != "--last" ]] || shift
	[[ "$1" != "--all" ]] || ( all_mode=true; shift )

	for a; do
		[[ $a != -* && $a != --* ]] || continue

		canonical_path="$(readlink -e "$a" 2>/dev/null)"

        [[ -n "$canonical_path" ]]  || [[ -e "$canonical_path" ]] || [[ $? -eq 0 ]] || print_err_exit "Could not determine canonical path for: $a"

		if $all_mode; then
			show_all_changes "$canonical_path"
		else
			show_last_change "$canonical_path"
		fi
    done
}

exec_main "$@"