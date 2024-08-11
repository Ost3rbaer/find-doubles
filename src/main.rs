use clap::Parser;
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

// get inode on unix and Linux as unique file id
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// minimum file size
    #[arg(short = 'm', long, value_name = "BYTES", default_value_t = 65536)]
    min_size: u64,

    /// maximum file size
    #[arg(short='M',long, value_name="BYTES", default_value_t = std::u64::MAX)]
    max_size: u64,

    /// Directory to be scanned, can be repeated
    #[arg(short, long)]
    directories: Vec<PathBuf>,

    /// files to be excluded from scan, GLOB syntax
    #[arg(short = 'e', long, value_name = "GLOB")]
    exclude_files: Vec<glob::Pattern>,

    /// directories to be excluded from scan, GLOB syntax
    #[arg(short = 'E', long, value_name = "GLOB")]
    exclude_dirs: Vec<glob::Pattern>,

    /// write list of duplicates to CSV file
    #[arg(short, long, value_name = "FILE.csv")]
    csv_export: Option<PathBuf>,

    /// report duplicate files
    #[arg(short, long)]
    report_duplicates: bool,

    /// print files that matched filter
    #[arg(short = 'p', long)]
    print_files: bool,

    /// print directories
    #[arg(short = 'P', long)]
    print_directories: bool,

    /// print elapsed times
    #[arg(short, long)]
    timings: bool,

    /// replace duplicates by hard links
    #[arg(short, long)]
    link_duplicates: bool,
}

// under Windows we use a 128bit murmur3 hash to distinguish actual physical files
#[cfg(windows)]
type FileId = u128;

// unix already has the  numerical inode for this which is just 64bit wide
#[cfg(not(windows))]
type FileId = u64;

fn main() {
    let mut args = Args::parse();
    #[cfg(windows)]
    {
        // a couple of files/dirs to exclude on Windows platforms by default
        // the windows directory is full of shadow copies, won't save anything and might mess with the OS
        if args.exclude_dirs.is_empty() {
            args.exclude_dirs
                .push(glob::Pattern::new("WINDOWS").unwrap());
        }
        // GOG uninstallers have a stupid locking mechanism that cause a +deadlock during
        // uninstall when hard linked
        if args.exclude_files.is_empty() {
            args.exclude_files
                .push(glob::Pattern::new("unins*").unwrap());
            args.exclude_files.push(glob::Pattern::new("*.db").unwrap());
        }
    }
    // use current directory when no dirs were specified
    if args.directories.is_empty() {
        args.directories.push(PathBuf::from("."));
    }
    let mut files: Vec<FileInfo> = Vec::new();
    let mut all_dirs: Vec<PathBuf> = vec![];

    let mut csv_file: Option<File> = if let Some(csv_path) = args.csv_export {
        match File::create(&csv_path) {
            Ok(file) => Some(file),
            Err(e) => {
                println!("{:?} creating {:?}, CSV output is not written", e, csv_path);
                None
            }
        }
    } else {
        None
    };
    if let Some(ref mut f) = csv_file {
        writeln!(f, "File,Size,Duplicate").expect("csv write");
    }
    let start = Instant::now();
    for dir in args.directories {
        find_files(
            &dir,
            &mut all_dirs,
            &mut files,
            args.min_size,
            args.max_size,
            &args.exclude_files,
            &args.exclude_dirs,
        );
    }
    let scan_duration = start.elapsed();
    let sort_start = Instant::now();
    files.sort_unstable_by_key(|file| file.size);
    let sort_duration = sort_start.elapsed();
    if args.print_files {
        for file in &files {
            println!("{:?}", file);
            //      println!("{} {} {} {}</>{}",file.id,file.nlink,file.size,all_dirs[file.dir_index].to_str().unwrap(), file.name);
        }
    }
    if args.print_directories {
        for dir in &all_dirs {
            println!("{:?}", dir);
        }
    }
    if args.timings {
        println!("Scanning of directories took {:?}", scan_duration);
        println!("Sorting of files took {:?}", sort_duration);
    }
    let mut total_size = 0;
    files.iter().for_each(|f| total_size += f.size);
    println!(
        "total {} files, {} directories, {} data",
        files.len(),
        all_dirs.len(),
        kmgt(total_size)
    );
    let mut cur = 0;
    let len_1 = files.len() - 1;
    let mut files_with_equals = 0;
    let mut sets_with_equals = 0;
    let mut fully_linked = 0;
    let mut old_link_save = 0;
    let mut set_of_2 = 0;
    let mut linked = 0;
    let mut new_link_save = 0;
    let mut file_compares = 0;
    let mut compare_time = Duration::new(0, 0);
    let mut peek_hash_time = Duration::new(0, 0);
    let mut full_hash_time = Duration::new(0, 0);
    #[cfg(windows)]
    let mut link_test_time = Duration::new(0, 0);
    let mut peek_hashes = 0;
    let mut full_hashes = 0;
    let mut full_hash_size = 0;
    let mut set_merges = 0;
    let mut merged_files = 0;
    let mut processed_size = 0;
    let mut last_size_percent = 0;
    let mut last_file_percent = 0;
    macro_rules! process_duplicate {
        ($dir : expr, $file : expr, $index : expr) => {
            if let Some(ref mut f) = csv_file {
                writeln!(
                    f,
                    "\"{}/{}\",{},\"{}/{}\"",
                    $dir.display(),
                    $file,
                    &files[$index].size,
                    all_dirs.get(files[$index].dir_index).unwrap().display(),
                    &files[$index].name,
                )
                .expect("csv write");
            }
            if args.report_duplicates {
                println!(
                    "\"{}/{}\" => \"{}/{}\"",
                    $dir.display(),
                    $file,
                    all_dirs.get(files[$index].dir_index).unwrap().display(),
                    &files[$index].name,
                );
            }
            if args.link_duplicates {
                link(
                    $dir,
                    $file,
                    all_dirs.get(files[$index].dir_index).unwrap(),
                    &files[$index].name,
                );
            }
        };
    }
    // merge tow runs of hard-linked files, all files of $merge_run are linked to $ref_run_start
    macro_rules! merge_runs {
        ($ref_run_start : expr, $merge_run_start : expr, $len : expr) => {
            assert!(files[$ref_run_start].size == files[$merge_run_start].size);
            merged_files += $len;
            set_merges += 1;
            new_link_save += files[$ref_run_start].size;
            let dir = all_dirs.get(files[$ref_run_start].dir_index).unwrap();
            let file = &files[$ref_run_start].name;
            #[cfg(debug_assertions)]
            println!(
                "merging runs at {:?}/{file}[{}] and {}[{}]",
                dir, $ref_run_start, $merge_run_start, $len
            );
            if args.link_duplicates {
                for i in $merge_run_start..$merge_run_start + $len {
                    process_duplicate!(&dir, &file, i);
                }
            }
            linked += $len;
        };
    }
    // compare 2 files, hard link them if they match
    macro_rules! fcmp_link {
        ($file1_i : expr, $file2_i : expr) => {
            assert!(files[$file1_i].size == files[$file2_i].size);
            let compare_start = Instant::now();
            if fcmp(
                all_dirs.get(files[$file1_i].dir_index).unwrap(),
                &files[$file1_i].name,
                all_dirs.get(files[$file2_i].dir_index).unwrap(),
                &files[$file2_i].name,
                files[$file1_i].size,
            ) {
                process_duplicate!(
                    all_dirs.get(files[$file1_i].dir_index).unwrap(),
                    &files[$file1_i].name,
                    $file2_i
                );
                linked += 1;
                new_link_save += files[$file1_i].size;
            }
            compare_time += compare_start.elapsed();
            file_compares += 1;
        };
    }
    while cur < len_1 {
        // TODO: improve progress reporting, search on crates.io
        let file_percent = 100 * cur / len_1;
        let size_percent = 100 * processed_size / total_size;
        if file_percent != last_file_percent || size_percent != last_size_percent {
            print!(
                "progress: current size {}, {file_percent}% ({cur}/{len_1}) files, {size_percent}% ({}/{}) data    \r",
				kmgt(files[cur].size),
                kmgt(processed_size),
                kmgt(total_size)
            );
			_ = std::io::stdout().flush();
            last_file_percent = file_percent;
            last_size_percent = size_percent;
        }
        if files[cur].size != files[cur + 1].size {
            processed_size += files[cur].size;
            cur += 1;
            continue;
        }
        sets_with_equals += 1;
        // candidate for duplicate
        let refi = cur;
        #[cfg(not(windows))]
        {
            cur += 2;
        }
        #[cfg(windows)]
        let link_test_start = Instant::now();
        while cur <= len_1 && files[cur].size == files[refi].size {
            #[cfg(windows)]
            {
                files[cur].id = windows_id(
                    all_dirs.get(files[cur].dir_index).unwrap(),
                    &files[cur].name,
                );
            }
            cur += 1;
        }
        #[cfg(windows)]
        {
            link_test_time += link_test_start.elapsed();
        }
        files_with_equals += cur - refi;
		processed_size += ((cur-refi) as u64)*files[refi].size;
        #[cfg(debug_assertions)]
        println!("{refi}..{cur}@{:}", files[refi].size);
        // now files[ref..cur-1] have the same size and their id (inode) is known
        // sort that range by id (inode)
        files
            .get_mut(refi..cur)
            .unwrap()
            .sort_unstable_by_key(|f| f.id);
        if files[refi].id == files[cur - 1].id {
            fully_linked += 1;
            old_link_save += ((cur - refi - 1) as u64) * files[refi].size;
            #[cfg(debug_assertions)]
            println!("run {refi}..{cur} is fully linked");
            continue;
        }
        if cur - refi == 2 {
            #[cfg(debug_assertions)]
            println!("set of 2");
            // just 2 files
            // direct compare
            set_of_2 += 1;
            fcmp_link!(cur - 1, refi);
            continue;
        }
        // group runs of same file id (inode)
        #[derive(Debug)]
        struct FileRun {
            first: usize,
            len: usize,
            peek_hash: u128,
        }
        let mut runs: Vec<FileRun> = Vec::new();
        let mut run_start = refi;
        for i in refi..cur {
            if files[i].id != files[run_start].id {
                runs.push(FileRun {
                    first: run_start,
                    len: i - run_start,
                    peek_hash: 0,
                });
                run_start = i;
            }
        }

        runs.push(FileRun {
            first: run_start,
            len: cur - run_start,
            peek_hash: 0,
        });
        if runs.len() == 2 {
            #[cfg(debug_assertions)]
            println!("2 runs of same inode");
            let compare_start = Instant::now();
            if fcmp(
                all_dirs.get(files[runs[0].first].dir_index).unwrap(),
                &files[runs[0].first].name,
                all_dirs.get(files[runs[1].first].dir_index).unwrap(),
                &files[runs[1].first].name,
                files[refi].size,
            ) {
                #[cfg(debug_assertions)]
                println!(
                    "merging pair runs with same {:?} and {:?}",
                    runs[0], runs[1]
                );
                if runs[0].len > runs[1].len {
                    merge_runs!(runs[0].first, runs[1].first, runs[1].len);
                } else {
                    merge_runs!(runs[1].first, runs[0].first, runs[0].len);
                }
            }
            compare_time += compare_start.elapsed();
            file_compares += 1;
            continue;
        }
        #[cfg(debug_assertions)]
        println!("computing peek hashes");
        // peek hash first
        peek_hashes += runs.len();
        let hash_start = Instant::now();
        runs.iter_mut().for_each(|r| {
            r.peek_hash = match peek_hash(
                all_dirs.get(files[r.first].dir_index).unwrap(),
                &files[r.first].name,
                if files[r.first].size > 4096 {
                    4096
                } else {
                    files[r.first].size as usize
                },
            ) {
                Ok(hash) => hash,
                _ => 0,
            }
        });
        runs.sort_unstable_by(|a, b| {
            if a.peek_hash == b.peek_hash {
                b.len.cmp(&a.len)
            } else {
                a.peek_hash.cmp(&b.peek_hash)
            }
        });
        peek_hash_time += hash_start.elapsed();
        // identify runs of same peek_hash
        let len_1 = runs.len() - 1;
        let mut i = 0;
        // skip runs with  hashes that could not be computed due to i/o errors
        while i < len_1 && runs[i].peek_hash == 0 {
            i += 1;
        }
        #[cfg(debug_assertions)]
        println!("grouping runs by peek {len_1}");
        while i < len_1 {
            if runs[i].peek_hash == runs[i + 1].peek_hash {
                if i + 1 == len_1 || runs[i].peek_hash != runs[i + 2].peek_hash {
                    // just 2 runs with the same peek_hash -> direct compare
                    let f_ref = runs[i].first;
                    let compare_start = Instant::now();
                    if fcmp(
                        all_dirs.get(files[f_ref].dir_index).unwrap(),
                        &files[f_ref].name,
                        all_dirs.get(files[runs[i + 1].first].dir_index).unwrap(),
                        &files[runs[i + 1].first].name,
                        files[f_ref].size,
                    ) {
                        // comparison function ensured that the first run is the longest
                        merge_runs!(f_ref, runs[i + 1].first, runs[i + 1].len);
                    }
                    compare_time += compare_start.elapsed();
                    i += 2;
                    continue;
                }
                // we have a sequence of three or more runs with the same
                // peek_hash - try to distinguish them by full hashing algorithm
                #[derive(Debug)]
                struct RunRun {
                    first: usize,
                    len: usize,
                    hash: FullHash,
                }
                let mut run_runs = Vec::<RunRun>::new();
                let ref_hash = runs[i].peek_hash;
                let full_hash_start = Instant::now();
                while i <= len_1 && runs[i].peek_hash == ref_hash {
                    full_hash_size += files[runs[i].first].size;
                    match full_hash(
                        all_dirs.get(files[runs[i].first].dir_index).unwrap(),
                        &files[runs[i].first].name,
                    ) {
                        Ok(hash) => run_runs.push(RunRun {
                            first: runs[i].first,
                            len: runs[i].len,
                            hash,
                        }),
                        _ => (),
                    }

                    i += 1;
                }
                full_hashes += 1;
                full_hash_time += full_hash_start.elapsed();
                if run_runs.len() > 1 {
                    // need stable sort here to keep longest run first
                    run_runs.sort_by_key(|r| r.hash);
                    // last sprint: check for run_runs with same hash
                    // these files have same size, same peek_hash and same full hash
                    // let's merge them
                    //println!("{:?}", run_runs);
                    let mut i = 1;
                    let mut refi = 0;
                    while i < run_runs.len() {
                        while i < run_runs.len() && run_runs[i].hash == run_runs[refi].hash {
                            merge_runs!(run_runs[refi].first, run_runs[i].first, run_runs[i].len);
                            i += 1;
                        }
                        refi = i;
                        i += 1;
                    }
                }
            }
            i += 1;
        }
    }
    // skip progress report line
    println!();
    if args.timings {
        #[cfg(windows)]
        println!("spent {:?} to get unique file ids", link_test_time);
        println!("{files_with_equals} files in {sets_with_equals} sets of equal size grouped");
        println!(
            "{fully_linked} sets were already linked, saving {}",
            kmgt(old_link_save)
        );
        println!(
            "{set_of_2} pairs compared, created {linked} new links saving {}",
            kmgt(new_link_save)
        );
        println!(
            "spent {:?} comparing {file_compares} file pairs",
            compare_time
        );
        println!(
            "spent {:?} computing {peek_hashes} peek hashes",
            peek_hash_time
        );
        println!(
            "spent {:?} computing {full_hashes} full hashes, ({})",
            full_hash_time,
            kmgt(full_hash_size)
        );
        println!("merged {merged_files} int {set_merges} existing sets");
        println!("Total time spent {:?}", start.elapsed());
    }
}

/// nicely format number of bytes into human-readable form
fn kmgt(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    if bytes < 1024 * 1024 {
		let mag = 1024;
        let f = (bytes % mag) * 10 / mag;
        return format!("{}.{f} kiB", bytes / mag);
    }
    if bytes < 1024 * 1024 * 1024 {
		let mag = 1024 * 1024;
        let f = (bytes % mag) * 10 / mag;
        return format!("{}.{f} MiB", bytes / mag);
    }
    if bytes < 1024 * 1024 * 1024 * 1024 {
		let mag = 1024 * 1024 * 1024;
        let f = (bytes % mag) * 10 / mag;
        return format!("{}.{f} GiB", bytes / mag);
    }
	let mag = 1024 * 1024 * 1024 * 1024;
    let f = (bytes % mag) * 10 / mag;
    format!("{}.{f} TiB", bytes / mag)
}

// type FullHash has to match digest used in full_hash()
// and has to implement Ord, PartialOrd, and Eq for sorting
type FullHash = [u8; 32];

/// compute full hash of file
fn full_hash(dir: &PathBuf, name: &str) -> Result<FullHash, std::io::Error> {
    let mut hasher = blake3::Hasher::new();
    let mut file_name = dir.clone();
    file_name.push(name);
	hasher.update_mmap(file_name)?;
	Ok(*hasher.finalize().as_bytes())
}

// type PeekHash has to match digest used in peek_hash()
// and has to implement Ord, PartialOrd, and Eq for sorting
type PeekHash = u128;

/// compute hash of the first size bytes of file
fn peek_hash(dir: &PathBuf, name: &str, size: usize) -> Result<PeekHash, std::io::Error> {
    // fastmurmur3 does not implement digest, hence we read all data in the buffer
    let mut buffer = Vec::<u8>::with_capacity(size);
    unsafe { buffer.set_len(size) };
    let mut file_name = dir.clone();
    file_name.push(name);
    #[cfg(debug_assertions)]
    println!("computing peek hash of {:?}", file_name);
    let mut file = File::open(file_name)?;
    file.read_exact(&mut buffer)?;
    Ok(fastmurmur3::hash(&buffer))
}

/// link file1 to file2, replacing file2
fn link(dir1: &PathBuf, name1: &str, dir2: &PathBuf, name2: &str) {
    let mut file_name1 = dir1.clone();
    file_name1.push(name1);
    let mut file_name2 = dir2.clone();
    file_name2.push(name2);
    let mut tmp_name2 = dir2.clone();
    tmp_name2.push(name2.to_owned() + ".dbl");
    #[cfg(debug_assertions)]
    println!("linking {:?} -> {:?}", file_name1, file_name2);
    _ = match fs::hard_link(file_name1, &tmp_name2) {
        Ok(_) => fs::rename(tmp_name2, file_name2),
        Err(e) => {
            _ = std::fs::remove_file(tmp_name2);
            Err(e)
        }
    }
}

/// compare two files
// TODO: error handling, consider anyhow
fn fcmp(dir1: &PathBuf, name1: &str, dir2: &PathBuf, name2: &str, size: u64) -> bool {
    let mut file_name1 = dir1.clone();
    file_name1.push(name1);
    let mut file_name2 = dir2.clone();
    file_name2.push(name2);
    #[cfg(debug_assertions)]
    println!("comparing {:?} and {:?} @{size}", file_name1, file_name2);
    let buff_size: usize = if size > 65536 { 65536 } else { size as usize };
    let mut buffer1 = Vec::<u8>::with_capacity(buff_size);
    let mut buffer2 = Vec::<u8>::with_capacity(buff_size);
    let mut file1 = match File::open(file_name1) {
        Ok(stream) => stream,
        _ => {
            return false;
        }
    };
    let mut file2 = match File::open(file_name2) {
        Ok(stream) => stream,
        _ => {
            return false;
        }
    };
    let mut pending = size as usize;
    while pending > 0 {
        let target_size: usize = if pending > buff_size {
            buff_size
        } else {
            pending
        };
        unsafe { buffer1.set_len(target_size) };
        unsafe { buffer2.set_len(target_size) };
        _ = file1.read_exact(&mut buffer1);
        _ = file2.read_exact(&mut buffer2);
        if buffer1 != buffer2 {
            return false;
        }
        pending -= target_size;
    }
    #[cfg(debug_assertions)]
    println!("{} matched", kmgt(size));
    true
}

/// provide a replacement for inodes as unique ids on windows
// windows does not provide an inode
// hard linked files can be identified by getting FindFirstFileName on them - linked files share that property
// the following code is ugly due to the conversions needed between Windows API and native Rust strings
#[cfg(windows)]
fn windows_id(dir: &PathBuf, name: &str) -> FileId {
    use windows::{
        core::*,
        Win32::Storage::FileSystem::{FindClose, FindFirstFileNameW},
    };
    let mut cb_buffer = 2048_u32;
    use std::ffi::OsStr;
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;

    let mut full_name = dir.clone();
    let mut buffer = Vec::<u16>::with_capacity(cb_buffer as usize);
    let lp_buffer = PWSTR(buffer.as_mut_ptr());
    full_name.push(name);
    // that's awful
    let wide_name: Vec<u16> = OsStr::new(full_name.to_str().unwrap())
        .encode_wide()
        .chain(once(0))
        .collect();
    match unsafe {
        FindFirstFileNameW(
            PCWSTR::from_raw(wide_name.as_ptr()),
            0,
            &mut cb_buffer,
            lp_buffer,
        )
    } {
        Ok(handle) => {
            _ = unsafe { FindClose(handle) };
            let buffer = unsafe { std::slice::from_raw_parts(lp_buffer.0, cb_buffer as usize - 1) };
            let len = buffer.len().checked_mul(2).unwrap();
            let ptr: *const u8 = buffer.as_ptr().cast();

            let byte_buffer = unsafe { std::slice::from_raw_parts(ptr, len) };
            let hash = fastmurmur3::hash(byte_buffer);

            /*
                        // And convert from UTF-16 to Rust's native encoding
                        let file_first_name = String::from_utf16_lossy(buffer);
                        println!("{cb_buffer} {} {:?}", hash as FileId, full_name);
                        println!("File first name: {}", file_first_name);
            */
            hash as FileId
        }
        // fileid 0 indicates I/O error -> file will be excluded from further processing
        _ => 0,
    }
}

/// find all files with min_size <= size <= max_size below dir
fn find_files(
    dir: &PathBuf,
    all_dirs: &mut Vec<PathBuf>,
    files: &mut Vec<FileInfo>,
    min_size: u64,
    max_size: u64,
    exclude_files: &Vec<glob::Pattern>,
    exclude_dirs: &Vec<glob::Pattern>,
) {
    if let Ok(entries) = fs::read_dir(dir) {
        let dir_index = all_dirs.len();
        // TODO: postpone saving of directory path on stack, only store it when we also store files
        // requires BFS which we can't guarantee
        all_dirs.push(dir.clone());

        'entries: for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                let metadata = match fs::symlink_metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(_) => continue,
                };
                // do not follow symbolic links, junctions or mount points
                if metadata.is_symlink() {
                    #[cfg(debug_assertions)]
                    println!("skipping symlink {}", path.display());
                    continue;
                }

                for ignore_pattern in exclude_files {
                    if ignore_pattern
                        .matches(&path.file_name().unwrap().to_string_lossy().into_owned())
                    {
                        continue 'entries;
                    }
                }
                if metadata.is_file() {
                    if metadata.len() < min_size || metadata.len() > max_size {
                        continue;
                    }
                    let file_info = FileInfo {
                        name: path.file_name().unwrap().to_string_lossy().into_owned(),
                        dir_index,
                        size: metadata.len(),
                        #[cfg(unix)]
                        id: metadata.ino() as FileId,
                        #[cfg(windows)]
                        id: 0, // we defer computation of uniq id on windows as it is costly and we only need it for duplicate candidates
                    };
                    files.push(file_info);
                } else if metadata.is_dir() {
                    // recurse here
                    for ignore_pattern in exclude_dirs {
                        if ignore_pattern
                            .matches(&path.file_name().unwrap().to_string_lossy().into_owned())
                        {
                            continue 'entries;
                        }
                    }
                    // check for ignore mark
                    let mut ignore_path = path.clone();
                    ignore_path.push(".keep_duplicates");
                    if let Ok(_) = fs::symlink_metadata(&ignore_path) {
                        println!("skipping {} - has .keep_duplicates", path.display());
                    } else {
                        find_files(
                            &path,
                            all_dirs,
                            files,
                            min_size,
                            max_size,
                            &exclude_files,
                            &exclude_dirs,
                        );
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
struct FileInfo {
    dir_index: usize,
    id: FileId,
    size: u64,
    name: String,
}
