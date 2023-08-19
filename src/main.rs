use std::fs;
use std::path::PathBuf;
use std::io::Write;
use clap::Parser;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt; // FIXME
// globset and regex later
//extern crate glob;
//use glob::Pattern;

#[derive(Parser,Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
	/// mimimum file size
	#[arg(short='m',long, value_name="BYTES", default_value_t = 65536)]
	min_size: u64,
	
	/// maximum file size
	#[arg(short='M',long, value_name="BYTES", default_value_t = std::u64::MAX)]
	max_size: u64,

	/// Directory to be scanned, can be repeated
	#[arg(short,long)]
	directories: Vec<std::path::PathBuf>,

	/// files to be excluded from scan, GLOB syntax
	#[arg(short='e',long, value_name="GLOB")]
	exclude_files: Vec<glob::Pattern>,
	
	/// directories to be excluded from scan, GLOB syntax
	#[arg(short='E',long, value_name="GLOB")]
	exclude_dirs: Vec<glob::Pattern>,
	
	/// print matched files
	#[arg(short='p',long)]
	print_files : bool,
	
	/// print directories
	#[arg(short='P',long)]
	print_directories : bool,
	
	/// print elapsed times
	#[arg(short,long)]
    timings : bool,
}

fn main() {
	let mut args = Args::parse();
	#[cfg(windows)]
	{
		if args.exclude_dirs.is_empty() {
			args.exclude_dirs.push(glob::Pattern::new("WINDOWS").unwrap());
		}
		if args.exclude_files.is_empty() {
			args.exclude_files.push(glob::Pattern::new("unins*").unwrap());
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
        find_files(&dir, &mut all_dirs, &mut files, args.min_size, args.max_size, &args.exclude_files, &args.exclude_dirs);
    }
	let scan_duration = start.elapsed();
    let start = Instant::now();
    files.sort_unstable_by_key(|file| file.size);
	let sort_duration = start.elapsed();
	if args.print_files {
    for file in &files {
      println!("{:?}",file);
//      println!("{} {} {} {}</>{}",file.id,file.nlink,file.size,all_dirs[file.dir_index].to_str().unwrap(), file.name);
    }
	}
	if args.print_directories {
    for dir in &all_dirs {
      println!("{:?}",dir);
    }
	}
	if args.timings {
    println!("Scanning of directories took {:?}", scan_duration);
    println!("Sorting of files took {:?}", sort_duration);
	}
    println!("{} files, {} directories", files.len(), all_dirs.len());
	// find files of common size
    files.sort_unstable_by_key(|f| f.size );
	let mut cur = 0;
	let len_1 = files.len()-1;
	while cur < len_1 {
		if files[cur].size == files[cur+1].size {
			// candidate for duplicate
			let refi = cur;
			#[cfg(not(windows))]
			{
			  cur += 2;
			}
			while cur <= len_1 && files[cur].size == files[refi].size {
				#[cfg(windows)]
				{
			      files[cur].id = windows_id(all_dirs.get(files[cur].dir_index).unwrap(), &files[cur].name);
				}
				cur += 1;
			}
			// now files[ref..cur-1] have the same size and their id (inode) is known
			// sort that range by id (inode)
			files.get_mut(refi..cur).unwrap().sort_unstable_by_key(|f| f.id );
		}
		cur += 1;
	}
}

// windows does not provide an inode
// hard linked files can be identified by getting FindFirstFileName on them - linked files share that property
// the following code is ugly due to the conversions needed between windows API and native Rust strings
#[cfg(windows)]
fn windows_id(dir: &PathBuf, name : &str) -> u64 {
use windows::{core::*,
              Win32::Storage::FileSystem::{FindFirstFileNameW,FindClose},
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
	let wide_name : Vec<u16> = OsStr::new(full_name.to_str().unwrap()).encode_wide().chain(once(0)).collect();
	match unsafe { FindFirstFileNameW(PCWSTR::from_raw(wide_name.as_ptr()), 0, &mut cb_buffer, lp_buffer) } {
		Ok(handle) => {
        let buffer = unsafe { std::slice::from_raw_parts(lp_buffer.0, cb_buffer as usize - 1) };
    let len = buffer.len().checked_mul(2).unwrap();
    let ptr: *const u8 = buffer.as_ptr().cast();

	let byte_buffer =     unsafe { std::slice::from_raw_parts(ptr, len) };
		let hash = fastmurmur3::hash( byte_buffer );

        // And convert from UTF-16 to Rust's native encoding
        let file_first_name = String::from_utf16_lossy(buffer);
		println!("{cb_buffer} {} {:?}",hash as u64, full_name);
        println!("File first name: {}", file_first_name);
	    _ = unsafe { FindClose(handle) };
	    // call hasher here
		hash as u64
		},
		_ => 0,
	}
			
/*	
	let handle = unsafe { FindFirstFileNameW(PCWSTR::from_raw(wide_name.as_ptr()), 0, &mut cb_buffer, lp_buffer) };
	
        let buffer = unsafe { std::slice::from_raw_parts(lp_buffer.0, cb_buffer as usize - 1) };

        // And convert from UTF-16 to Rust's native encoding
        let file_first_name = String::from_utf16_lossy(buffer);
		println!("{cb_buffer} {:?}",full_name);
        println!("File first name: {}", file_first_name);
	// call hasher here
	unsafe { FindClose(handle.unwrap()) };
		0
		*/
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

    'entries:    for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                let metadata = match fs::symlink_metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(_) => continue,
                };
                // do not follow symbolic links, junctions or mount points
                if metadata.is_symlink() {
					println!("skipping symlink {}",path.display());
                    continue;
                }

		for ignore_pattern in exclude_files {
		  if ignore_pattern.matches(&path.file_name().unwrap().to_string_lossy().into_owned()) {
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
                        id: metadata.ino(),
#[cfg(windows)]
	                id: 0, // we defer computation of uniq id on windows as it is costly and we only need it for duplicate candidates
                    };
                    files.push(file_info);
                } else if metadata.is_dir() {
		// recurse here
		for ignore_pattern in exclude_dirs {
		  if ignore_pattern.matches(&path.file_name().unwrap().to_string_lossy().into_owned()) {
		     continue 'entries;
		  }
		}
		// check for ignore mark
		let mut ignore_path=path.clone();
		ignore_path.push(".keep_duplicates");
		if let Ok(_) = fs::symlink_metadata(&ignore_path) {
					println!("skipping {} - has .keep_duplicates",path.display());
		} else {
        find_files(&path, all_dirs, files, min_size, max_size, &exclude_files, &exclude_dirs);
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
    id: u64,
    size: u64,
    name: String,
}
