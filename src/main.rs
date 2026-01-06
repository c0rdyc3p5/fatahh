use clap::Parser;
use rayon::prelude::*;
use std::{env, fs};
use std::path::PathBuf;
use std::time::Instant;
use tabled::{Table, Tabled};
use tabled::settings::object::Columns;
use tabled::settings::{Alignment, Style};

#[derive(Debug, Clone)]
struct FileData {
    path: String,
    size: u64,
}

#[derive(Clone)]
struct FileCollection {
    files: Vec<FileData>,
    max_size: usize,
}

impl FileCollection {
    pub fn new(max_size: usize) -> Self {
        Self {
            files: Vec::with_capacity(max_size),
            max_size,
        }
    }

    pub fn smart_insert(&mut self, file: FileData) {
        let pos = self.files.partition_point(|f| f.size >= file.size);
        if pos < self.max_size {
            self.files.insert(pos, file);
            if self.files.len() > self.max_size {
                self.files.pop();
            }
        }
    }

    pub fn merge(&mut self, other: FileCollection) {
        for f in other.files {
            self.smart_insert(f);
        }
    }
}

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long, default_value = ".")]
    path: String,

    #[arg(short, long, default_value_t = 100)]
    count: usize,
}

#[derive(Tabled)]
struct FileDataTable {
    #[tabled(rename = "Path")]
    path: String,

    #[tabled(rename = "Size")]
    size: String,
}

fn format_size(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut i = 0;

    while size >= 1024.0 && i < units.len() - 1 {
        size /= 1024.0;
        i += 1;
    }

    format!("{:.2} {}", size, units[i])
}

fn walk_dir_parallel(path: PathBuf, max_count: usize) -> FileCollection {
    let mut local = FileCollection::new(max_count);

    let entries = match fs::read_dir(&path) {
        Ok(e) => e,
        Err(_) => return local,
    };

    let mut subdirs = Vec::new();

    for entry in entries.flatten() {
        if let Ok(ft) = entry.file_type() {
            if ft.is_dir() {
                subdirs.push(entry.path());
            } else if let Ok(meta) = entry.metadata() {
                let size = meta.len();
                if size > 0 {
                    local.smart_insert(FileData {
                        path: entry.path().to_string_lossy().into_owned(),
                        size,
                    });
                }
            }
        }
    }

    let merged = subdirs
        .into_par_iter()
        .map(|dir| walk_dir_parallel(dir, max_count))
        .reduce(
            || FileCollection::new(max_count),
            |mut a, b| {
                a.merge(b);
                a
            },
        );

    local.merge(merged);
    local
}

fn main() {
    let args = Args::parse();

    let abs_path: PathBuf = if args.path.is_empty() {
        env::current_dir().expect("Failed to get the current directory")
    } else {
        PathBuf::from(&args.path)
    };

    println!("Scanning: {}\n", abs_path.display());

    let start = Instant::now();

    let result = walk_dir_parallel(abs_path, args.count);

    if result.files.is_empty() {
        return;
    }

    let rows: Vec<FileDataTable> = result
        .files
        .iter()
        .map(|f| FileDataTable {
            path: f.path.clone(), // raw path as-is
            size: format_size(f.size),
        })
        .collect();

    let table = Table::new(rows)
        .with(Style::psql())
        .modify(Columns::first(), Alignment::left())
        .modify(Columns::last(), Alignment::right())
        .to_string();
    println!("{table}");

    println!(
        "\nFound the fattest {} files in {:.4}s",
        result.files.len(),
        start.elapsed().as_secs_f64()
    );
}
