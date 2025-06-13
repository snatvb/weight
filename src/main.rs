use anyhow::{Context, Result};
use clap::Parser;
use colored::*;
use glob::glob;
use rayon::prelude::*;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "weight")]
#[command(about = "Calculate total size of files matching glob patterns")]
#[command(version = "1.0")]
#[command(
    after_help = "EXAMPLES:\n  weight **/*.png **/*.jpg **/*.dds\n  weight -v *.png\n  weight --threads 4 **/*.rs\n\nNOTE: In Nushell, use separate patterns instead of brace expansion"
)]
struct Args {
    #[arg(required = true)]
    patterns: Vec<String>,
    #[arg(short, long)]
    threads: Option<usize>,
    #[arg(short, long)]
    verbose: bool,
    #[arg(short, long)]
    debug: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.debug {
        println!(
            "{}: {}",
            "Current directory".blue().bold(),
            env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "Unable to get current dir".to_string())
                .cyan()
        );
        println!("{}: {:?}", "Arguments".blue().bold(), args.patterns);

        match std::fs::read_dir(".") {
            Ok(_) => println!("{}: Current directory is readable", "✓".green()),
            Err(e) => {
                println!("{}: Cannot read current directory: {}", "✗".red(), e);
                return Err(anyhow::anyhow!("Cannot read current directory: {}", e));
            }
        }
    }

    if let Some(threads) = args.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global()
            .context("Failed to set thread pool size")?;
    }

    let all_candidate_paths = args.patterns.par_iter().map(|pattern| -> Result<_> {
        if args.debug {
            println!("{}: {}", "Processing pattern".yellow(), pattern.cyan());
        }

        let paths = glob(pattern).with_context(|| format!("Invalid glob pattern: {}", pattern))?;

        let mut pattern_paths = Vec::new();
        for path in paths {
            match path {
                Ok(path) => {
                    if args.debug {
                        println!("  {} {}", "Found path:".blue(), path.display());
                    }
                    pattern_paths.push(path);
                }
                Err(e) => {
                    eprintln!(
                        "{}: Error processing path: {}",
                        "Warning".yellow().bold(),
                        e
                    );
                }
            }
        }

        if args.debug {
            println!(
                "  {} {} paths from pattern: {}",
                "Found".green(),
                pattern_paths.len().to_string().cyan(),
                pattern.cyan()
            );
        }

        Ok(pattern_paths)
    });

    let all_candidate_paths: Vec<PathBuf> =
        all_candidate_paths.try_reduce(Vec::new, |mut acc, item| {
            acc.extend(item);
            Ok(acc)
        })?;

    if args.debug {
        println!(
            "{} {} candidate paths, filtering files in parallel...",
            "Total".green().bold(),
            all_candidate_paths.len().to_string().cyan()
        );
    }

    let all_files: Vec<PathBuf> = all_candidate_paths
        .par_iter()
        .filter_map(|path| {
            if path.is_file() {
                if args.debug {
                    println!("    {} {} (added)", "✓".green(), path.display());
                }
                Some(path.clone())
            } else {
                if args.debug {
                    println!("    {} {} (skipped)", "✗".red(), path.display());
                }
                None
            }
        })
        .collect();

    if all_files.is_empty() {
        println!("{}", "No files found matching the patterns".yellow());

        if args.debug {
            println!("\n{}", "Debug suggestions:".cyan().bold());
            println!(
                "• Current directory: {}",
                env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| "Unknown".to_string())
                    .yellow()
            );
            println!("• Try running from the directory where your files are located");
            println!("• Check if the file extensions are correct");
            println!(
                "• In Nushell, use separate patterns: {} instead of {}",
                "**/*.png **/*.jpg **/*.dds".green(),
                "**/*.{png,jpg,dds}".red().strikethrough()
            );
            println!(
                "• Try a simpler pattern like {} or {}",
                "*.png".green(),
                "./**/*.png".green()
            );
            println!("• Check directory permissions with: {}", "ls -la".cyan());
        } else {
            println!(
                "{} Use {} flag for debug information",
                "Tip:".blue().bold(),
                "--debug".cyan()
            );
        }

        return Ok(());
    }

    println!(
        "{} {} files, calculating sizes...",
        "Found".green().bold(),
        all_files.len().to_string().cyan().bold()
    );

    let results: Vec<Result<(PathBuf, u64)>> = all_files
        .par_iter()
        .map(|path| {
            let metadata = fs::metadata(path)
                .with_context(|| format!("Failed to read metadata for: {}", path.display()))?;
            Ok((path.clone(), metadata.len()))
        })
        .collect();

    let mut total_size = 0u64;
    let mut error_count = 0;

    for result in results {
        match result {
            Ok((path, size)) => {
                total_size += size;
                if args.verbose {
                    let size_str = format_size(size);

                    println!(
                        "{}: {}",
                        path.display().to_string().blue(),
                        size_str.green()
                    );
                }
            }
            Err(e) => {
                eprintln!("{}: {}", "Error".red().bold(), e);
                error_count += 1;
            }
        }
    }

    println!("\n{}", "--- Summary ---".cyan().bold());
    println!(
        "{}: {}",
        "Files processed".green(),
        (all_files.len() - error_count).to_string().cyan().bold()
    );

    if error_count > 0 {
        println!(
            "{}: {}",
            "Errors".red().bold(),
            error_count.to_string().red()
        );
    }

    let total_size_str = format_size(total_size);

    println!(
        "{}: {}",
        "Total size".green().bold(),
        total_size_str.magenta().bold()
    );

    Ok(())
}

fn format_size(size: u64) -> String {
    const UNITS: &[(&str, &str)] = &[
        ("B", "bright_white"),
        ("KB", "bright_blue"),
        ("MB", "bright_green"),
        ("GB", "bright_yellow"),
        ("TB", "bright_red"),
    ];
    let mut size = size as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    let (unit, _color) = UNITS[unit_index];

    if unit_index == 0 {
        format!("{} {}", size as u64, unit)
    } else {
        format!("{:.2} {}", size, unit)
    }
}
