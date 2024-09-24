# Find Doubles 

## What's About?

Under some scenarios we can have quite a bunch of duplicate files on our hard drives or SSDs consuming precious space, e.g.

 - games and their standalone addons often install a lot of common files for each installataion, e.g.
   + if you have the GoG version of SpellForce 3 and it's addons installed at the same time there are > 60 GiB of duplicate files
   + the Steam versions of King Arthur Knight's Tale and Legion IX have almost 23 GiB in common
 - when using Proton as compatibility layer for Windows games (e.g. on the Steam Deck), steam creates individual bottles for each game, each containing more or less the same files: dot.net/Mongo, direct x libraries, msvc runtime dlls etc.
 - sometimes one just wants to find out how many copies of the photographs taken on the last trip are stored on the hard drive ;)

There are quite a few GUI tools around for Windows and I used Duplicate Commander (https://www.softpedia.com/get/System/File-Management/Duplicate-Commander.shtml) for a while. But given the fact that the tool is now almost 10 years old, it is not surprising that it does not scale well on file systems with millions of files.
Also I orefer a command line tool that runsunder Windows and Linux alike and that can be run in the background as service without user interaction.

On my Steam deck it saves me ~10% of the precious internal SSD space.

**find_doubles** has been tested on Windows 10/11 (x86/64), Arch Linux (SteamOS), OpenSuSE 15.x, and Debian 12. It should compile and run on other *IX like platforms as well.

If you have a multi-boot system and cross-mounted partitions among them, it is strongly recommended to run **find_doubles** on the OS where the filesystem is native to, i.e. dedupe NTFS partitions from Windows and extX partitions from linux.

## Usage

**find_doubles** can be run in different modes, dependning on the use case:

 - just report how much space is used up by duplicate files: `find_doubles -t -d` <*path*>
 - print duplicate filenames: `find_doubles -r -d` <*path*>
 - create a list of duplicate files in CSV format: `find_doubles -c` *<list.csv>* `-d` <*path*>
 - replace all duplicates by hard links and print timing statistics: `find_doubles -tld` <*path*>

Multiple directories can be specified by repeeating the `-d` command line option; there are also options to exclude certain files or directories

When a directory contains a file named `.keep_duplicates` **find_doubles** skips this directory and all directories below it.

On Windows, an implicit file exclude pattern is used when no explicit is specified with the `-e` switch: all files starting with `unins` will not be linked. The reason behind this are the GoG uninstallers. The uninstallers for the main game and the addons are identical. But due to Windows file locking semantics the uninstallation would break when deinstalling the main game.

```
Usage: find_doubles.exe [OPTIONS]

Options:
  -m, --min-size <BYTES>           minimum file size [default: 65536]
  -M, --max-size <BYTES>           maximum file size [default: 18446744073709551615]
  -H, --peek-hash <BYTES>          length of initial segment to hash when more than 2 files have the same length [default: 4096]
  -d, --directories <DIRECTORIES>  directory to be scanned, can be repeated
  -e, --exclude-files <GLOB>       files to be excluded from scan, GLOB syntax
  -E, --exclude-dirs <GLOB>        directories to be excluded from scan, GLOB syntax
  -c, --csv-export <FILE.csv>      write list of duplicates to CSV file
  -r, --report-duplicates          report duplicate files
  -p, --print-files                print files that matched filter
  -P, --print-directories          print directories
  -t, --timings                    print elapsed times
  -l, --link-duplicates            replace duplicates by hard links
  -h, --help                       Print help
  -V, --version                    Print version
```

## Algorithm

**find_doubles** takes a couple of measures to save memory and minimise I/O operations. The goal is to detect differences between files with as little read operations as possible and not using more RAM than needed for that.

 - unavoidable: it scans the directories specified by `-d` and stores all matching files. Paths are stored independent of file names to save memory. All subsequent steps work insitu on this collect file list (a Vec), no copies are made.
 - next step is to group the files according to their size
 - when there are two or more files of the sanm size, it ries to determine if they are already linked. On linux it uses the inode (already acquired during initial scan). On Windows, the inodes are not usable. Instead Windows provides an API that returns a list of all files hard-linked to each other. That list has the property that the first file name (obtained via FindFirstFileNameW) is identical for all files in a linked set. **find_doubles** then uses the murmur3 hash of that name as inode replacement.
 - if there are more than two files with the same size that are linked, **find_doubles** computes the murmur3 hash of the initial 4096 bytes (configurable with `-H` option)
 - when there are more than two files with the same length and the same murmur3 hash, **find_doubles** computes the blake3 hash over the whole file content. When file length, initial murmur3 hash, and blake3 hash match, the files are considered dupliactes (and replaced by har-lins with the `-l` option)
 - when there are just two files matching during file length or murmur3 comparison, their content is compared until a difference is reached or they considered equal

## License

**find_doubles** is made available under the RPL v1.5, i.e. when changing/improving the software, give it back yo the community. And don't use it under high risk conditions. Even it is in use by myself quite frequently it is not bullet proof or ISO9xxx certified.
