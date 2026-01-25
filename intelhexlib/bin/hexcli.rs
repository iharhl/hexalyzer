use intelhexlib::IntelHex;
use std::cmp::PartialEq;
use std::env;
use std::path::{Path, PathBuf};
use std::process;

#[derive(PartialEq, Eq)]
enum FileType {
    Bin,
    Hex,
    Other,
}

fn print_usage() {
    let version = env!("CARGO_PKG_VERSION");

    println!(" ----------------------------------------------------------------");
    println!("|  Intel HEX Utility  | v{version} - Copyright (c) 2026 Ihar Hlukhau |");
    println!(" ----------------------------------------------------------------");
    println!("\nUsage:");
    println!("  hexcli info <input>");
    println!("  hexcli relocate <input> <output> [options]");
    println!("  hexcli convert <input> <output> [options]");
    println!("  hexcli merge <output> <input1>[:addr] ... <inputN>[:addr]");
    println!("\nOptions:");
    println!("  --address <val>    Base address for relocate / convert from BIN to HEX");
    println!(
        "  --gap-fill <val>   Byte to fill gaps when converting / merging to BIN (default: 0xFF)"
    );
    println!("\nExamples:");
    println!("  hexcli info firmware.hex");
    println!("  hexcli relocate firmware.hex firmware_shifted.hex --address 0x1000");
    println!("  hexcli convert firmware.hex firmware.bin --gap-fill 0x00");
    println!("  hexcli merge final.hex firmware1.hex firmware2.bin:0xFF00");
}

fn main() {
    let args: Vec<String> = env::args().collect();

    println!();

    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    let command = &args[1];

    // Dispatch and immediately handle results
    if let Err(e) = run_dispatch(command, &args) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

#[allow(clippy::too_many_lines)]
fn run_dispatch(cmd: &str, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        "help" | "-h" | "--help" => {
            print_usage();
            Ok(())
        }
        "info" => {
            // Guard: Check args count
            let path_str = args.get(2).ok_or("Missing input file path")?;

            // Guard: File must exist
            let abs_path =
                validate_exists(path_str).map_err(|_| format!("File not found: {path_str}"))?;

            run_info(&abs_path)
        }
        "relocate" => {
            // Guard: Check file path arguments given
            let in_path_str = args.get(2).ok_or("Missing input path")?;
            let out_path_str = args.get(3).ok_or("Missing output path")?;

            // Guard: Check input exists
            let in_abs_path = validate_exists(in_path_str)?;

            // Guard: Check input is a hex path
            if get_file_type(&in_abs_path) != FileType::Hex {
                return Err("Relocation is only supported for HEX input files".into());
            }

            // Guard: Check output is a hex path
            let out_path = PathBuf::from(out_path_str);
            if get_file_type(&out_path) != FileType::Hex {
                return Err(format!("Argument '{out_path_str}' is not a HEX output path").into());
            }

            // Get relocation address
            let addr_str = get_flag_value(args, "--address");
            let addr = if let Some(addr) = addr_str {
                parse_hex_str(&addr).map_err(|_e| format!("Invalid address: {addr}"))?
            } else {
                return Err("Missing '--address' flag or the value after it".into());
            };

            run_relocate(&in_abs_path, &out_path, addr)
        }
        "convert" => {
            // Guard: Check file paths arguments given
            let in_path_str = args.get(2).ok_or("Missing input path")?;
            let out_path_str = args.get(3).ok_or("Missing output path")?;

            // Guard: Check input exists
            let in_abs_path = validate_exists(in_path_str)?;

            let out_path = PathBuf::from(out_path_str);
            let in_file_type = get_file_type(&in_abs_path);
            let out_file_type = get_file_type(&out_path);

            // Guard: Check files are of a supported type
            if in_file_type == FileType::Other || out_file_type == FileType::Other {
                return Err("Input or output files are of unsupported type".into());
            }

            // Guard: Check input files are of a diff type
            if in_file_type == out_file_type {
                return Err("Cannot convert between the same file type".into());
            }

            let addr_str = get_flag_value(args, "--address");
            let gap_fill_str = get_flag_value(args, "--gap-fill");

            // Guard: Check address is provided ONLY if converting FROM bin
            if addr_str.is_some() && in_file_type != FileType::Bin {
                return Err(
                    "Base address '--address' is only supported for BIN to HEX conversion".into(),
                );
            } else if addr_str.is_none() && in_file_type == FileType::Bin {
                return Err(
                    "Base address '--address' is required for BIN to HEX conversion".into(),
                );
            }

            let base_addr = if let Some(addr) = addr_str {
                Some(parse_hex_str(&addr).map_err(|_e| format!("Invalid address: {addr}"))?)
            } else {
                None
            };

            // Guard: Handle optional gap fill ONLY if converting TO bin
            if gap_fill_str.is_some() && in_file_type != FileType::Hex {
                return Err(
                    "Gap fill '--gap-fill' is only supported for HEX to BIN conversion".into(),
                );
            }
            let gap_fill = if let Some(gap_fill) = gap_fill_str {
                u8::try_from(
                    parse_hex_str(&gap_fill)
                        .map_err(|_e| format!("Invalid gap fill: {gap_fill}"))?,
                )?
            } else {
                0xFF
            };

            run_convert(&in_abs_path, &out_path, base_addr, gap_fill)
        }
        "merge" => {
            if args.len() < 5 {
                return Err(
                    "Usage: hexcli merge <output> <input1>[:addr] ... <inputN>[:addr]".into(),
                );
            }

            // Guard: Check output file path argument given
            let out_path_str = args.get(2).ok_or("Missing output path")?;
            let out_path = PathBuf::from(out_path_str);

            // Collect input file paths and optional base addresses
            let mut inputs: Vec<(PathBuf, Option<usize>)> = Vec::new();
            for arg in &args[3..] {
                if arg.starts_with("--") {
                    break; // stop at flags
                }

                let parts: Vec<&str> = arg.split(':').collect();
                let in_abs_path = validate_exists(parts[0])?;
                let addr = if parts.len() > 1 {
                    Some(
                        parse_hex_str(parts[1])
                            .map_err(|_e| format!("Invalid address: {}", parts[1]))?,
                    )
                } else {
                    None
                };
                inputs.push((in_abs_path, addr));
            }

            let gap_fill_str = get_flag_value(args, "--gap-fill");
            let gap_fill = if let Some(gap_fill) = gap_fill_str {
                u8::try_from(
                    parse_hex_str(&gap_fill)
                        .map_err(|_e| format!("Invalid gap fill: {gap_fill}"))?,
                )?
            } else {
                0xFF
            };

            run_merge(inputs, &out_path, gap_fill)
        }
        _ => {
            print_usage();
            process::exit(1);
        }
    }
}

fn run_info(path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    fn format_addr(addr: usize) -> String {
        let s = format!("{addr:08X}");
        format!("0x{}_{}", &s[0..4], &s[4..8])
    }

    fn format_with_commas(n: usize) -> String {
        let s = n.to_string();
        s.as_bytes()
            .rchunks(3)
            .rev()
            .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
            .collect::<Vec<_>>()
            .join(",")
    }

    let ih = if get_file_type(path) == FileType::Hex {
        IntelHex::from_hex(path)?
    } else if get_file_type(path) == FileType::Bin {
        IntelHex::from_bin(path, 0x0)?
    } else {
        return Err(format!("File type not supported: {}", path.display()).into());
    };

    println!("File Path:   {}", path.display());
    println!("Data Size:   {} bytes", format_with_commas(ih.size));
    println!(
        "Range:       {} - {}",
        format_addr(ih.get_min_addr().unwrap_or(0)),
        format_addr(ih.get_max_addr().unwrap_or(0)),
    );
    Ok(())
}

fn run_convert(
    in_path: &PathBuf,
    out_path: &PathBuf,
    addr: Option<usize>,
    gap_fill: u8,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut ih = match addr {
        Some(base) => IntelHex::from_bin(in_path, base)?,
        None => IntelHex::from_hex(in_path)?,
    };

    if get_file_type(out_path) == FileType::Bin {
        ih.write_bin(out_path, gap_fill)?;
    } else {
        ih.write_hex(out_path)?;
    }

    // Validate output file was written
    let out_abs_path = validate_exists(&out_path.to_string_lossy())?;

    println!(
        "Converted {} -> {}",
        in_path.display(),
        out_abs_path.display()
    );
    Ok(())
}

fn run_relocate(
    in_path: &PathBuf,
    out_path: &PathBuf,
    new_addr: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut ih = IntelHex::from_hex(in_path)?;
    ih.relocate(new_addr)?;
    ih.write_hex(out_path)?;

    // Validate output file was written
    let out_abs_path = validate_exists(&out_path.to_string_lossy())?;

    println!(
        "Relocated {} to 0x{new_addr:X} -> {}",
        in_path.display(),
        out_abs_path.display()
    );
    Ok(())
}

fn run_merge(
    inputs: Vec<(PathBuf, Option<usize>)>,
    out_path: &PathBuf,
    gap_fill: u8,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut master_ih = IntelHex::new();

    for (path, addr) in inputs {
        let ih = match get_file_type(&path) {
            FileType::Bin => {
                let base_addr = addr.ok_or_else(|| {
                    format!("Base address required for binary file: {}", path.display())
                })?;
                IntelHex::from_bin(&path, base_addr)?
            }
            FileType::Hex => {
                let mut ih = IntelHex::from_hex(&path)?;
                if let Some(new_addr) = addr {
                    ih.relocate(new_addr)?;
                }
                ih
            }
            FileType::Other => {
                return Err(format!("Unsupported file type: {}", path.display()).into());
            }
        };

        master_ih.merge(&ih); // todo: use safe or not safe merge?
    }

    if get_file_type(out_path) == FileType::Bin {
        master_ih.write_bin(out_path, gap_fill)?;
    } else {
        master_ih.write_hex(out_path)?;
    }

    // Validate output file was written
    let out_abs_path = validate_exists(&out_path.to_string_lossy())?;

    println!("Successfully merged files into {}", out_abs_path.display());
    Ok(())
}

// =============================== HELPER FUNCTIONS ===============================

/// Parse a string as a hex number (with optional 0x prefix)
fn parse_hex_str(s: &str) -> Result<usize, std::num::ParseIntError> {
    let s = s.trim();

    // Handle explicit 0x prefix
    if let Some(hex_str) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return usize::from_str_radix(hex_str, 16);
    }

    // Parse as hex without prefix
    usize::from_str_radix(s, 16)
}

/// Determine `FileType` based on the file's extension (case-insensitive)
fn get_file_type(path: &Path) -> FileType {
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("hex"))
    {
        return FileType::Hex;
    } else if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("bin"))
    {
        return FileType::Bin;
    }
    FileType::Other
}

/// Validate that a path exists and is a file. Returns absolute path.
fn validate_exists(path_str: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = PathBuf::from(path_str);
    if !path.exists() {
        return Err(format!("File not found: {path_str}").into());
    }
    if !path.is_file() {
        return Err(format!("Path is not a file: {path_str}").into());
    }
    // Return absolute path
    Ok(std::fs::canonicalize(path)?)
}

/// Find the value after a specific flag (e.g., "--gap-fill 0xFF")
fn get_flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|pos| args.get(pos + 1))
        .cloned()
}
