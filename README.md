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

## Usage

**find_doubles** can be run in different modes, dependning on the use case:

 - just report how much space is used up by duplicate files: `find_doubles -t -d` <*path*>
 - print duplicate filenames: `find_doubles -r -d` <*path*>
 - create a list of duplicate files in CSV format: `find_doubles -c` *<list.csv>* `-d` <*path*>
 - replace all duplicates by hard links and print timing statistics: `find_doubles -tld` <*path*>

Multiple directories can be specified by repeeating the `-d` command line option; there are also options to exclude certain files or directories

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

**TBD**

**find_doubles** description to be added

## License

**find_doubles** is made available under the RPL v1.5, i.e. when changing/improving the software, give it back yo the community. And don't use it under high risk conditions. Even it is in use by myself quite frequently it is not bullet proof or ISO9xxx certified.
