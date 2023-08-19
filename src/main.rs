use std::fs;
use std::path::PathBuf;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt; // FIXME
// globset and regex later
//extern crate glob;
//use glob::Pattern;

fn main() {
    let mut files: Vec<FileInfo> = Vec::new();
    let min_size = 64*1024; // replace with your desired minimum size
    let max_size = std::u64::MAX; // replace with your desired maximum size
    let hard_linked = false; // replace with your desired value
    let exclude_pattern: Option<glob::Pattern> = Some(glob::Pattern::new("unins*").unwrap()); // replace with your desired exclude pattern

    let mut source_dirs: Vec<PathBuf> = vec![]; // replace with your desired directories
    let mut all_dirs: Vec<PathBuf> = vec![]; // replace with your desired directories

    source_dirs.push(PathBuf::from("."));
    for dir in source_dirs {
        find_files(&dir, &mut all_dirs, &mut files, min_size, max_size, hard_linked, &exclude_pattern);
    }
    files.sort_unstable_by_key(|file| file.size);
	/*
    for file in &files {
      println!("{:?}",file);
//      println!("{} {} {} {}</>{}",file.id,file.nlink,file.size,all_dirs[file.dir_index].to_str().unwrap(), file.name);
    }
	*/
    println!("{} files, {} directories", files.len(), all_dirs.len());
    let common_finder=CommonFinder::new(files, |f| f.size);
}

fn find_files(
    dir: &PathBuf,
    all_dirs: &mut Vec<PathBuf>,
    files: &mut Vec<FileInfo>,
    min_size: u64,
    max_size: u64,
    hard_linked: bool,
    exclude_pattern: &Option<glob::Pattern>,
) {
    if let Ok(entries) = fs::read_dir(dir) {
    let dir_index = all_dirs.len();
 //   all_dirs.push(PathBuf::from(dir.to_str().unwrap()));
    all_dirs.push(dir.clone());

        for entry in entries {
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

		if let Some(ignore_pattern) = exclude_pattern {
		  if ignore_pattern.matches(&path.file_name().unwrap().to_string_lossy().into_owned()) {
		     continue;
		  }
		}
                if metadata.is_file() {
                    //&& metadata.permissions().readonly()
                    if metadata.len() < min_size || metadata.len() > max_size {
						 continue;
					 }
#[cfg(unix)]
                if !(hard_linked || metadata.nlink() == 1) {
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
		// check for ignore mark
		let mut ignore_path=path.clone();
		ignore_path.push(".keep_duplicates");
		if let Ok(_) = fs::symlink_metadata(&ignore_path) {
					println!("skipping {} - has .keep_duplicates",path.display());
		} else {
        find_files(&path, all_dirs, files, min_size, max_size, hard_linked, &exclude_pattern);
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
