use std::fs;
use std::path::PathBuf;
use std::io::Write;
use clap::Parser;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt; // FIXME
// globset and regex later
//extern crate glob;
//use glob::Pattern;

#[derive(Parser,Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
	/// mimimum file size
//	#[arg(short='m',long, name="FILE_MIN_SIZE", default_value_t = 65536)]
	#[arg(short='m',long, value_name="BYTES", default_value_t = 65536)]
	min_size: u64,
	
	/// maximum file size
//	#[arg(short='M',long, name="FILE_MAX_SIZE", default_value_t = std::u64::MAX)]
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

    for dir in args.directories {
        find_files(&dir, &mut all_dirs, &mut files, args.min_size, args.max_size, &args.exclude_files, &args.exclude_dirs);
    }
    files.sort_unstable_by_key(|file| file.size);
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
    println!("{} files, {} directories", files.len(), all_dirs.len());
    let common_finder=CommonFinder::new(files, |f| f.size);
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
 //   all_dirs.push(PathBuf::from(dir.to_str().unwrap()));
    all_dirs.push(dir.clone());

    'entries:    for entry in entries {
//    println!("{:?}", entry);
            if let Ok(entry) = entry {
                let path = entry.path();
                let metadata = match fs::symlink_metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(_) => continue,
                };
//    println!("{:?}", metadata);
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
                    //&& metadata.permissions().readonly()
                    if metadata.len() < min_size || metadata.len() > max_size {
						 continue;
					 }
#[cfg(unix)]
                if !(true || metadata.nlink() == 1) {
		continue;
		}
                    let file_info = FileInfo {
                        name: path.file_name().unwrap().to_string_lossy().into_owned(),
//                        name: path.file_name().unwrap().to_string_lossy(), //.to_string_lossy().into_owned(),
			dir_index,
                        size: metadata.len(),
//                        name: path.to_str().unwrap().to_string(),
#[cfg(unix)]
                        id: metadata.ino(),
#[cfg(windows)]
	                id: 0, // we defer computation of uniq id on windows as it is costly and we only need it for duplicate candidates
                    };
//		    println!("{:?}",file_info);
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

struct CommonFinder<T,K,F>
where
    F: Fn(&T) -> K,
    K: Ord {
    accessor :F,
    cursor: usize,
    len: usize,
    refindex: usize,
    data: Vec<T>,
	}


impl<T,K,F> CommonFinder<T,K,F> where
    F: Fn(&T) -> K,
    K: Ord {
 pub fn new(mut data:Vec<T>, accessor: F) -> Self {
   data.sort_unstable_by_key(|elem| accessor(elem));
   Self{ cursor:0, refindex:0, len:data.len(), accessor, data }
 }
 pub fn has_duplicates(&mut self) -> bool {
   while self.cursor + 1 < self.len {
    if (self.accessor)(&self.data[self.cursor]) == (self.accessor)(&self.data[self.cursor+1]) {
      self.refindex = self.cursor;
      return true
    }
    self.cursor += 1;
   }
   false
 }
}

/*
impl<T,K,F> Iterator<Item = &T> for CommonFinder<T,K,F> where
    F: Fn(&T) -> K,
    K: Ord {
    fn next(&mut self) -> Option<Self::Item> {
      if (self.accessor)(&self.data[self.cursor]) != (self.accessor)(&self.data[self.refindex]) {
        return None;
      }
      self.cursor += 1;
      Some(&self.data[self.cursor-1])
   }
}
*/

#[derive(Debug)]
#[allow(dead_code)]
struct FileInfo {
    dir_index: usize,
    id: u64,
    size: u64,
    name: String,
}
