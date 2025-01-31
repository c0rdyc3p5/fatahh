#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::path::Path;
use std::env;
use std::time::Instant;
use clap::Parser;
use walkdir::WalkDir;
use tabled::{
    settings::{
        object::{Columns}, Alignment, Style,
    },
    Tabled,
    Table
};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The path to search for files, default is the current directory
    #[arg(short, long, default_value_t = String::from(""))]
    path: String,

    /// Number of files to display, default is 100
    #[arg(short, long, default_value_t = 100)]
    count: usize,
}

struct FileData {
    path: String,
    size: u64,
}

impl FileData {
    fn new(path: String, size: u64) -> FileData {
        FileData { path, size }
    }
}

struct FileCollection {
    files: Vec<FileData>,
    max_size: usize,
}

impl FileCollection {
    fn new(max_size: usize) -> Self {
        FileCollection {
            files: Vec::new(),
            max_size,
        }
    }

    fn smart_insert(&mut self, file: FileData) {
        if self.files.len() < self.max_size {
            self.files.push(file);
            if self.files.len() == self.max_size {
                // Sort once at full capacity
                self.files.sort_by(|a, b| b.size.cmp(&a.size));
            }
        } else {
            // Perform binary search and insert if collection is full
            if let Some(index) = self.find_insert_position(&file.size) {
                self.files.insert(index, file);
                self.files.pop(); // Remove the smallest file to maintain size limit
            }
        }
    }

    fn find_insert_position(&self, target_size: &u64) -> Option<usize> {
        // Return None if the size is smaller than the smallest file
        if self.files.is_empty() || *target_size < self.files[self.files.len() - 1].size {
            return None;
        }

        // Use binary search for efficiency
        match self.files.binary_search_by(|file| file.size.cmp(target_size).reverse()) {
            Ok(pos) | Err(pos) => Some(pos),
        }
    }
}

#[derive(Tabled)]
struct FileDataTable {
    #[tabled(rename = "Path")]
    path: String,
    #[tabled(rename = "Size")]
    size: String
}

impl FileDataTable {
    fn new(path: String, size: String) -> FileDataTable {
        FileDataTable { path, size }
    }
}

const UNITS: [&str; 9] = ["Bytes", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];

fn format_size(bytes: usize, with_decimals: bool) -> String {
    // Define size units and their corresponding suffixes
    let mut size = bytes as f64; // Convert to f64 for division
    let mut suffix_index = 0;

    while size >= 1024.0 && suffix_index < UNITS.len() - 1 {
        size /= 1024.0;
        suffix_index += 1;
    }

    // Format the size string with appropriate decimal places
    let size_str = if !with_decimals {
        format!("{:.0}", size) // No decimal places if it's a whole number
    } else {
        format!("{:.2}", size) // Two decimal places otherwise
    };

    format!("{} {}", size_str, UNITS[suffix_index])
}

fn main() {
    let args = Args::parse();

    // If no path is set (empty string), use the current directory
    let path_str = if args.path.is_empty() {
        env::current_dir() // Get the current directory
            .expect("Failed to get the current directory") // Handle potential errors
            .to_string_lossy()
            .into_owned() // Convert to owned String
    } else {
        args.path
    };

    let path = Path::new(&path_str);

    // Verify if the path exists
    if !path.exists() {
        eprintln!("Error: The path '{}' does not exist.", path_str);
        return;
    }

    if !path.is_dir() {
        eprintln!("Error: The path '{}' is not a directory.", path_str);
        return;
    }

    let runtime_start = Instant::now();
    let mut files: Vec<FileData> = Vec::new();
    for entry in WalkDir::new(&path_str) {
        if let Ok(entry) = entry {
            if !entry.file_type().is_file() {
                continue;
            }

            let metadata = if let Ok(metadata) = entry.metadata() {
                metadata
            } else {
                continue;
            };

            let len = metadata.len();

            if len == 0 {
                continue;
            }

            let file_data = FileData::new(entry.path().to_string_lossy().to_string(), len);
            files.push(file_data);
        }
    }

    // Get memory usage of the vec files
    #[cfg(debug_assertions)]
    {
        let vec_size = size_of_val(&files); // Size of the Vec structure
        let elements_size: usize = files.iter().map(|file| size_of::<FileData>() + file.path.len()).sum(); // Size of all FileData instances
        let total_memory = vec_size + elements_size; // Total memory usage
        println!("- Debug Information -");
        println!("Path: {}", path_str);
        println!("Memory used by files: {}", format_size(total_memory, false));
        println!("Number of files: {}", files.len());
        println!("---------------------")
    }

    let mut biggest_files = FileCollection::new(args.count);
    for file in files {
        biggest_files.smart_insert(file);
    }

    let tabled_files: Vec<FileDataTable> = biggest_files
        .files
        .into_iter() // Use into_iter to consume the vector and move ownership
        .map(|file_data| {
            FileDataTable::new(
                file_data.path,
                format_size(file_data.size as usize, true)
            )
        })
        .collect();
    let runtime_end = runtime_start.elapsed();

    let table = Table::new(&tabled_files)
        .with(Style::psql())
        .modify(Columns::first(), Alignment::left())
        .modify(Columns::last(), Alignment::right())
        .to_string();

    println!("{}", table);

    let end_message = format!("Found the fattest {} files in {:.2}s", args.count, runtime_end.as_secs_f64());
    println!("{}", end_message);
}