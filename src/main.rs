use clap::Parser;
use std::fs;
//use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

// get inode on unix and Linux as unique file id
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// mimimum file size
    #[arg(short = 'm', long, value_name = "BYTES", default_value_t = 65536)]
    min_size: u64,

    /// maximum file size
    #[arg(short='M',long, value_name="BYTES", default_value_t = std::u64::MAX)]
    max_size: u64,

    /// Directory to be scanned, can be repeated
    #[arg(short, long)]
    directories: Vec<std::path::PathBuf>,

    /// files to be excluded from scan, GLOB syntax
    #[arg(short = 'e', long, value_name = "GLOB")]
    exclude_files: Vec<glob::Pattern>,

    /// directories to be excluded from scan, GLOB syntax
    #[arg(short = 'E', long, value_name = "GLOB")]
    exclude_dirs: Vec<glob::Pattern>,

    /// print matched files
    #[arg(short = 'p', long)]
    print_files: bool,

    /// print directories
    #[arg(short = 'P', long)]
    print_directories: bool,

    /// print elapsed times
    #[arg(short, long)]
    timings: bool,
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
        if args.exclude_dirs.is_empty() {
            args.exclude_dirs
                .push(glob::Pattern::new("WINDOWS").unwrap());
        }
        if args.exclude_files.is_empty() {
            args.exclude_files
                .push(glob::Pattern::new("unins*").unwrap());
            args.exclude_files.push(glob::Pattern::new("*.db").unwrap());
        }
    }
    if args.directories.is_empty() {
        args.directories.push(PathBuf::from("."));
    }
    /*
    println!("{:?}", args);
    if args.min_size != args.max_size {
        return;
    }
    */
    let mut files: Vec<FileInfo> = Vec::new();

    let mut all_dirs: Vec<PathBuf> = vec![];

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
    let start = Instant::now();
    files.sort_unstable_by_key(|file| file.size);
    let sort_duration = start.elapsed();
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
    println!("{} files, {} directories", files.len(), all_dirs.len());
    let start = Instant::now();
    let mut cur = 0;
    let len_1 = files.len() - 1;
	let mut files_with_equals = 0;
	let mut sets_with_equals = 0;
	let mut fully_linked = 0;
	let mut old_link_save = 0;
	let mut set_of_2 = 0;
	let mut linked = 0;
	let mut new_link_save = 0;
    while cur < len_1 {
        if files[cur].size == files[cur + 1].size {
			sets_with_equals += 1;
            // candidate for duplicate
            let refi = cur;
            #[cfg(not(windows))]
            {
                cur += 2;
            }
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
			files_with_equals += cur-refi;
             // now files[ref..cur-1] have the same size and their id (inode) is known
            // sort that range by id (inode)
           files
                .get_mut(refi..cur)
                .unwrap()
                .sort_unstable_by_key(|f| f.id);
			if files[refi].id == files[cur-1].id {
				fully_linked += 1;
				old_link_save += ((cur-refi-1) as u64)*files[refi].size;
			} else {
				if cur - refi == 2 {
					// direct compare
					set_of_2 += 1;
					if fcmp(all_dirs.get(files[cur-1].dir_index).unwrap(),
                        &files[cur-1].name,
						all_dirs.get(files[refi].dir_index).unwrap(),
                        &files[refi].name,
						files[refi].size) {
							link( all_dirs.get(files[cur-1].dir_index).unwrap(),
								 &files[cur-1].name,
								 all_dirs.get(files[refi].dir_index).unwrap(),
                                 &files[refi].name);
							linked += 1;
							new_link_save += files[refi].size;
						}
				} else {
					println!("{}: {refi}..{cur}",files[refi].size);
					for i in refi..cur {
						println!("{i} {:?}", files[i]);
					}
				}
			}

        }
        cur += 1;
    }
    let group_duration = start.elapsed();
    if args.timings {
        println!("Grouping files of equal size by id took {:?}", group_duration);
		println!("{files_with_equals} files in {sets_with_equals} sets of equal size grouped");
		println!("{fully_linked} sets were already linked, saving {old_link_save} bytes");
		println!("{set_of_2} pairs to compared, created {linked} new links saving {new_link_save} bytes");
    }
}

fn link(dir1: &PathBuf, name1: &str, dir2: &PathBuf, name2: &str) {
    let mut file_name1 = dir1.clone();
	file_name1.push(name1);
    let mut file_name2 = dir2.clone();
	file_name2.push(name2);
	println!("linking {:?} -> {:?}",file_name2 , file_name1);
}
// TODO: error handling, consider anyhow
fn fcmp(dir1: &PathBuf, name1: &str, dir2: &PathBuf, name2: &str, size: u64) -> bool {
	use std::io::Read;
	use std::fs::File;

    let mut file_name1 = dir1.clone();
	file_name1.push(name1);
    let mut file_name2 = dir2.clone();
	file_name2.push(name2);
    let buff_size: usize = if size > 65536 { 65536 } else { size as usize };
    let mut buffer1 = Vec::<u8>::with_capacity(buff_size);
    let mut buffer2 = Vec::<u8>::with_capacity(buff_size);
	let mut file1 = match File::open(file_name1) {
		Ok(stream) => stream,
		_ => { return false; },
	};
	let mut file2 = match File::open(file_name2) {
		Ok(stream) => stream,
		_ => { return false; },
	};
	let mut pending = size as usize;
	while pending > 0 {
		let target_size : usize = if pending > buff_size { buff_size } else { pending };
		buffer1.resize(target_size, 0u8);
		buffer2.resize(target_size, 0u8);
		_ = file1.read_exact(&mut buffer1);
		_ = file2.read_exact(&mut buffer2);
		if buffer1 != buffer2 {
			return false;
		}
		pending -= target_size;
	}
	true
}

// windows does not provide an inode
// hard linked files can be identified by getting FindFirstFileName on them - linked files share that property
// the following code is ugly due to the conversions needed between windows API and native Rust strings
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
    // that's awfull
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
            // call hasher here
*/
            hash as FileId
        }
        _ => 0,
    }

}

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
                        //                        name: path.file_name().unwrap().to_string_lossy(), //.to_string_lossy().into_owned(),
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
#[allow(dead_code)]
struct FileInfo {
    dir_index: usize,
    id: FileId,
    size: u64,
    name: String,
}
