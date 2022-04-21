[![asciicast](https://asciinema.org/a/488641.svg)](https://asciinema.org/a/488641)

# `httm`

The dream of a CLI ZFS Time Machine is still alive with `httm`.

`httm` prints the size, date and corresponding locations of available unique versions (deduplicated by modify time and size) of files residing on ZFS snapshots, but can also be used *interactively* to view and restore such files.  `httm` might change the way you use ZFS snapshots (because ZFS isn't designed for finding for unique file versions) or the Time Machine concept (because `httm` is very fast!).

`httm` boasts an array of seductive features, like:

* Search for and recursively list all deleted files! Ooooooooo!
* List file snapshots from *all* local pools (`httm` automatically detects local snapshots *as well as* locally replicated snapshots)!
* List file snapshots from remote backup pools (you may designate replicated remote snapshot directories).
* For use with even `rsync`-ed non-ZFS local datasets (like ext4, APFS, or NTFS), not just ZFS.
* Specify multiple files for lookup on different datasets
* 3 native interactive modes: lookup, select and restore
* ANSI `ls` colors from your environment
* Non-blocking recursive directory walking (available in all interactive modes)
* Select from several formatting styles.  Parseable ... or not ...  oh my!

Use in combination with you favorite shell (hot keys!) for even more fun.

Inspired by the [findoid](https://github.com/jimsalterjrs/sanoid) script, [fzf](https://github.com/junegunn/fzf) and many [zsh](https://www.zsh.org) key bindings.

## Installation

The `httm` project contains only a few components:

1. The `httm` executable. To build and install:

    ```bash
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh 
    git clone kimono-koans/httm 
    cargo install --path ./httm/
    ```
2. The optional `zsh` hot-key bindings: Use `ESC+s` to select snapshots filenames to be dropped to your command line (for instance after the `cat` command), or use `ESC+m` to browse for all of a file's snapshots. After you install the `httm` binary -- to copy the hot key script to your home directory, and source that script within your zshrc:

    ```bash
    httm --install-zsh-hot-keys
    ```
3. The optional `man` page: `cargo` has no native facilities for man page installation (though it may in the future!).  You can use `manpath` to see the various directories your system uses and decide which directory works best for you.  To install, just copy it to a directory in your `man` path, like so:

    ```bash
    cp ./httm/httm.1 /usr/local/share/man/man1/
    ```

### Caveats 

Right now, you will need to use a Unix-ish-y Rust-supported platform to build and install (that is: Linux, Solaris/illumos, the BSDs, MacOS).  Note: the platform need not support ZFS.  There is no fundamental reason a non-interactive Windows version could not be built, as it once did build, but Windows platform support is not a priority for me, a non-Windows user.  Contributions from Windows users are, of course, welcome.

## Example Usage

Print all local file snapshots of your `zsh` history file:
```bash
httm ~/.histfile
```
Print all files on snapshots (no "live" versions) deleted from your home directory, recursive, newline delimited, piped to a `deleted-files.txt` file: 
```bash
httm -d -n -R --no-live ~ > deleted-files.txt
```
Browse all files in your home directory, recursively, and view versions on local snapshots:
```bash
httm -i -R ~/
```
Browse all files deleted from your home directory, recursively, and view versions on all local and alternative replicated dataset snapshots:
```bash
httm -d only -i -a -R ~/
```
Browse all files in your home directory, recursively, and view versions on local snapshots, to select and ultimately restore to your working directory:
```bash
httm -r -R ~/
```

## I know what you're thinking, but slow your roll.

![To be clear, httm is *not*...](https://i.pinimg.com/originals/23/7f/2a/237f2ab8765663c721325366406197b7.gif)

To be clear, `httm` is not a H__ T__ T___ M______.

## License

httm is licensed under the MPL 2.0 License - see the LICENSE file for more details.