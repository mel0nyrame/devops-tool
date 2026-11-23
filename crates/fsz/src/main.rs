use comfy_table::{
    presets::UTF8_FULL_CONDENSED,
    Cell, CellAlignment, Color, ContentArrangement, Table,
};
use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};

mod directory_scan;
mod scan_status;

use directory_scan::{scan_directory, EntryInfo, EntryKind};
use scan_status::ScanStatus;

const BAR_WIDTH: usize = 18;

fn main() {
    let mut args = env::args_os();

    // Skip executable name.
    args.next();

    // `fsz` == `fsz .`
    let input = args.next().unwrap_or_else(|| ".".into());

    // Only one positional argument is supported.
    if args.next().is_some() {
        print_usage();
    }

    let path = Path::new(&input);

    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,

        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            eprintln!(
                "fsz: '{}': No such file or directory",
                path.display()
            );

            if let Some(suggestion) = find_similar_path(path) {
                eprintln!(
                    "Did you mean '{}'?",
                    suggestion.display()
                );
            }

            std::process::exit(1);
        }

        Err(err) => {
            exit_with_error(path, err);
        }
    };

    if metadata.file_type().is_symlink() {
        match fs::metadata(path) {
            Ok(target_metadata) if target_metadata.is_file() => {
                println!("{}", format_size(target_metadata.len()));
            }

            Ok(target_metadata) if target_metadata.is_dir() => {
                if let Err(err) = print_directory_overview(path) {
                    exit_with_error(path, err);
                }
            }

            Ok(_) => {
                eprintln!(
                    "fsz: '{}': unsupported file type",
                    path.display()
                );
                std::process::exit(1);
            }

            Err(err) => {
                exit_with_error(path, err);
            }
        }

        return;
    }

    if metadata.is_file() {
        println!("{}", format_size(metadata.len()));
        return;
    }

    if metadata.is_dir() {
        if let Err(err) = print_directory_overview(path) {
            exit_with_error(path, err);
        }

        return;
    }

    eprintln!(
        "fsz: '{}': unsupported file type",
        path.display()
    );

    std::process::exit(1);
}

fn print_usage() -> ! {
    eprintln!("usage: fsz [file|directory]");
    std::process::exit(1);
}

fn exit_with_error(path: &Path, err: io::Error) -> ! {
    eprintln!(
        "fsz: '{}': {}",
        path.display(),
        err
    );

    std::process::exit(1);
}

impl EntryInfo {
    fn display_name(&self) -> String {
        if self.is_dir() {
            format!("{}/", self.name)
        } else {
            self.name.clone()
        }
    }
}

fn print_directory_overview(path: &Path) -> io::Result<()> {
    let entries = {
        let stderr = io::stderr();
        if stderr.is_terminal() {
            let mut status = ScanStatus::new(stderr.lock());
            scan_directory(path, &mut || status.visit())?
        } else {
            scan_directory(path, &mut || {})?
        }
    };
    print_directory_entries(path, entries);
    Ok(())
}

fn print_directory_entries(path: &Path, mut entries: Vec<EntryInfo>) {
    /*
     * Sort:
     *
     *   1. Measured entries first
     *   2. Largest -> smallest
     *   3. Equal sizes -> alphabetical
     *   4. Permission-denied entries at the bottom
     */
    entries.sort_by(|a, b| {
        match (a.measured_size(), b.measured_size()) {
            (Some(a_size), Some(b_size)) => {
                b_size
                    .cmp(&a_size)
                    .then_with(|| {
                        a.name
                            .to_lowercase()
                            .cmp(&b.name.to_lowercase())
                    })
            }

            (Some(_), None) => std::cmp::Ordering::Less,

            (None, Some(_)) => std::cmp::Ordering::Greater,

            (None, None) => {
                a.name
                    .to_lowercase()
                    .cmp(&b.name.to_lowercase())
            }
        }
    });

    let total = entries
        .iter()
        .filter_map(EntryInfo::measured_size)
        .fold(0_u64, |acc, size| {
            acc.saturating_add(size)
        });

    println!("{}", path.display());
    println!();

    let mut table = Table::new();

    /*
     * UTF8_FULL_CONDENSED:
     *
     * Keeps:
     *   - outer border
     *   - header separator
     *   - columns
     *
     * Removes:
     *   - separator between every data row
     */
    table
        .load_style(
            UTF8_FULL_CONDENSED.with_rounded_corners()
        )
        .set_content_arrangement(
            ContentArrangement::Dynamic
        );

    table.set_header(vec![
        "NAME",
        "SIZE",
        "SHARE",
        "USAGE",
    ]);

    /*
     * Right-align numeric columns.
     */
    if let Some(column) = table.column_mut(1) {
        column.set_cell_alignment(
            CellAlignment::Right
        );
    }

    if let Some(column) = table.column_mut(2) {
        column.set_cell_alignment(
            CellAlignment::Right
        );
    }

    for entry in &entries {
        let name_cell = if entry.is_dir() {
            Cell::new(entry.display_name())
                .fg(Color::Blue)
        } else {
            Cell::new(entry.display_name())
        };

        match entry.kind {
            EntryKind::Exact { size, .. } => {
                let share =
                    calculate_share(size, total);

                table.add_row(vec![
                    name_cell,

                    Cell::new(
                        format_size(size)
                    ),

                    Cell::new(
                        format_percentage(
                            share * 100.0
                        )
                    ),

                    Cell::new(
                        make_bar(share),
                    )
                    .fg(Color::Green),
                ]);
            }

            EntryKind::Partial { size, .. } => {
                let share =
                    calculate_share(size, total);

                /*
                 * `~` means this is only the size that
                 * could be measured successfully.
                 */
                table.add_row(vec![
                    name_cell,

                    Cell::new(format!(
                        "~{}",
                        format_size(size)
                    ))
                    .fg(Color::Yellow),

                    Cell::new(
                        format_percentage(
                            share * 100.0
                        )
                    ),

                    Cell::new(
                        make_bar(share)
                    )
                    .fg(Color::Green),
                ]);
            }

            EntryKind::PermissionDenied { .. } => {
                table.add_row(vec![
                    name_cell,

                    Cell::new("—"),

                    Cell::new("—"),

                    Cell::new(
                        "permission denied"
                    )
                    .fg(Color::Yellow),
                ]);
            }
        }
    }

    println!("{table}");

    println!(
        "TOTAL  {}",
        format_size(total)
    );

    let measured_count = entries
        .iter()
        .filter(|entry| {
            entry.measured_size().is_some()
        })
        .count();

    let denied_count = entries.len() - measured_count;

    if denied_count == 0 {
        println!(
            "{} item{}",
            measured_count,
            if measured_count == 1 {
                ""
            } else {
                "s"
            }
        );
    } else {
        println!(
            "{} item{} · {} denied",
            measured_count,
            if measured_count == 1 {
                ""
            } else {
                "s"
            },
            denied_count,
        );
    }
}

fn calculate_share(size: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        size as f64 / total as f64
    }
}

fn find_similar_path(
    path: &Path,
) -> Option<PathBuf> {
    let file_name =
        path.file_name()?.to_string_lossy();

    let parent = path
        .parent()
        .unwrap_or_else(|| Path::new("."));

    let entries =
        fs::read_dir(parent).ok()?;

    let mut best_match:
        Option<(usize, PathBuf)> = None;

    for entry in entries.flatten() {
        let candidate_name =
            entry.file_name();

        let candidate =
            candidate_name.to_string_lossy();

        let distance =
            damerau_levenshtein(
                &file_name,
                &candidate,
            );

        match &best_match {
            None => {
                best_match = Some((
                    distance,
                    entry.path(),
                ));
            }

            Some((best_distance, _))
                if distance < *best_distance =>
            {
                best_match = Some((
                    distance,
                    entry.path(),
                ));
            }

            _ => {}
        }
    }

    let (distance, candidate) =
        best_match?;

    if distance >
        allowed_distance(&file_name)
    {
        return None;
    }

    /*
     * Preserve how the user wrote the path:
     *
     * .flods       -> .folds
     * src/mian.rs  -> src/main.rs
     */
    if parent == Path::new("")
        || parent == Path::new(".")
    {
        Some(PathBuf::from(
            candidate.file_name()?
        ))
    } else {
        Some(
            parent.join(
                candidate.file_name()?
            )
        )
    }
}

fn allowed_distance(name: &str) -> usize {
    match name.chars().count() {
        0..=3 => 1,
        4..=7 => 2,
        _ => 3,
    }
}

fn damerau_levenshtein(
    a: &str,
    b: &str,
) -> usize {
    let a: Vec<char> =
        a.chars().collect();

    let b: Vec<char> =
        b.chars().collect();

    let mut matrix = vec![
        vec![0; b.len() + 1];
        a.len() + 1
    ];

    for (i, row) in
        matrix.iter_mut().enumerate()
    {
        row[0] = i;
    }

    for j in 0..=b.len() {
        matrix[0][j] = j;
    }

    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(
                a[i - 1] != b[j - 1]
            );

            matrix[i][j] =
                (matrix[i - 1][j] + 1)
                    .min(
                        matrix[i][j - 1] + 1
                    )
                    .min(
                        matrix[i - 1][j - 1]
                            + cost
                    );

            /*
             * Adjacent transposition:
             *
             * flods
             * folds
             *
             * lo -> ol
             */
            if i > 1
                && j > 1
                && a[i - 1] == b[j - 2]
                && a[i - 2] == b[j - 1]
            {
                matrix[i][j] =
                    matrix[i][j].min(
                        matrix[i - 2][j - 2]
                            + 1
                    );
            }
        }
    }

    matrix[a.len()][b.len()]
}

fn make_bar(share: f64) -> String {
    let share =
        share.clamp(0.0, 1.0);

    let filled =
        (share * BAR_WIDTH as f64)
            .round() as usize;

    let filled =
        filled.min(BAR_WIDTH);

    let empty =
        BAR_WIDTH - filled;

    format!(
        "{}{}",
        "█".repeat(filled),
        "░".repeat(empty),
    )
}

fn format_percentage(
    percent: f64,
) -> String {
    if percent == 0.0 {
        "0.0%".to_string()
    } else if percent < 0.1 {
        "<0.1%".to_string()
    } else if percent < 10.0 {
        format!("{percent:.1}%")
    } else {
        format!("{percent:.0}%")
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;

    if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format_value(
            bytes,
            KB,
            "KB"
        )
    } else if bytes < GB {
        format_value(
            bytes,
            MB,
            "MB"
        )
    } else if bytes < TB {
        format_value(
            bytes,
            GB,
            "GB"
        )
    } else {
        format_value(
            bytes,
            TB,
            "TB"
        )
    }
}

fn format_value(
    bytes: u64,
    unit: u64,
    suffix: &str,
) -> String {
    let value =
        bytes as f64 / unit as f64;

    if value >= 100.0 {
        format!(
            "{value:.0} {suffix}"
        )
    } else if value >= 10.0 {
        format!(
            "{value:.1} {suffix}"
        )
    } else {
        format!(
            "{value:.2} {suffix}"
        )
    }
}