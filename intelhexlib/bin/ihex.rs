use intelhexlib::IntelHex;
use std::cmp::PartialEq;
use std::env;
use std::path::{Path, PathBuf};
use std::process;

// TODO: merge -> in + out + (base addr for bin 1) + (base addr for bin 2) + (fill gaps for bins)
// TODO: dump (dumps content for provided addr range to terminal)

#[derive(PartialEq, Eq)]
enum FileType {
    Bin,
    Hex,
    Other,
}

fn print_usage() {
    println!(" ------------------");
    println!("| Intel HEX Utility |");
    println!(" ------------------");
    println!("\nUsage:");
    println!("  ihex info <input>");
    println!("  ihex relocate <input> <output> [options]");
    println!("  ihex convert <input> <output> [options]");
    println!("  ihex merge <output> <input1>[:addr] ... <inputN>[:addr]");
    println!("\nOptions:");
    println!("  --address <val>     Base address for relocate or convert from .bin to .hex");
    println!("  --gap-fill <val>    Byte to fill gaps when converting to .bin (default: 0xFF)");
    println!("\nExamples:");
    println!("  ihex info firmware.hex");
    println!("  ihex relocate firmware.hex firmware_shifted.hex --address 0x1000");
    println!("  ihex convert firmware.hex firmware.bin --gap-fill 0x00");
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

fn run_dispatch(cmd: &str, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        "help" | "-h" | "--help" => {
            print_usage();
            Ok(())
        }
        "info" => {
            // Guard: Check args count
            let path_str = args.get(2).ok_or("Missing input file path")?;

            // Guard: File must exist. Get absolute path.
            let path =
                validate_exists(path_str).map_err(|_| format!("File not found: {path_str}"))?;

            run_info(&path)
        }
        "relocate" => {
            // Guard: Check file paths arguments given
            let in_str = args.get(2).ok_or("Missing 1st argument - input path")?;
            let out_str = args.get(3).ok_or("Missing 2nd argument - output path")?;

            // Guard: Check input exists
            let in_path = validate_exists(in_str)?;

            // Guard: Check output is a hex path
            let out_path = PathBuf::from(out_str);
            if get_file_type(&out_path) != FileType::Hex {
                return Err(format!(
                    "Argument '{out_str}' must be an output path with a .hex extension"
                )
                .into());
            }

            // Guard: Check input is a hex path
            if get_file_type(&in_path) != FileType::Hex {
                return Err("Relocation is only supported for HEX input files".into());
            }

            let addr_str = get_flag_value(args, "--address");

            let addr = if let Some(s) = addr_str {
                parse_address(&s).ok_or_else(|| format!("Invalid address: {s}"))?
            } else {
                return Err("Missing --address flag or value after it".into());
            };

            run_relocate(&in_path, &out_path, addr)
        }
        "convert" => {
            // Guard: Check file paths arguments given
            let in_str = args.get(2).ok_or("Missing input path")?;
            let out_str = args.get(3).ok_or("Missing output path")?;

            // Guard: Check input exists
            let in_path = validate_exists(in_str)?;

            let out_path = PathBuf::from(out_str);
            let in_file_type = get_file_type(&in_path);
            let out_file_type = get_file_type(&out_path);

            // Guard: Check files are of a supported type
            if in_file_type == FileType::Other || out_file_type == FileType::Other {
                return Err("Input or Output file are of unsupported type".into());
            }

            // Guard: Check input files are of a diff type
            if in_file_type == out_file_type {
                return Err("Cannot convert between the same file type".into());
            }

            let addr_str = get_flag_value(args, "--address");
            let gapfill_str = get_flag_value(args, "--gap-fill");

            // Guard: Check address is provided ONLY if converting FROM bin
            if addr_str.is_some() && in_file_type != FileType::Bin {
                return Err(
                    "Base address (--address) is only supported for BIN to HEX conversion".into(),
                );
            } else if addr_str.is_none() && in_file_type == FileType::Bin {
                return Err(
                    "Base address (--address) is required for BIN to HEX conversion".into(),
                );
            }
            let base_addr = addr_str.and_then(|s| parse_address(&s));

            // Guard: Handle optional gap fill ONLY if converting TO bin
            if gapfill_str.is_some() && in_file_type != FileType::Hex {
                return Err(
                    "Gap fill (--gap-fill) is only supported for HEX to BIN conversion".into(),
                );
            }
            let gap_fill =
                u8::try_from(gapfill_str.and_then(|s| parse_address(&s)).unwrap_or(0xFF))?;

            run_convert(&in_path, &out_path, base_addr, gap_fill)
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
        return Err(format!("File type not supported: {}", path.to_string_lossy()).into());
    };

    println!("File Path:   {}", path.to_string_lossy());
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

    let abs_out_path = validate_exists(&out_path.to_string_lossy())?;

    println!(
        "Converted {} -> {}",
        in_path.to_string_lossy(),
        abs_out_path.to_string_lossy()
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

    println!(
        "Relocated {} to 0x{new_addr:X} -> {}",
        in_path.to_string_lossy(),
        out_path.to_string_lossy()
    );
    Ok(())
}

fn parse_address(s: &str) -> Option<usize> {
    let s = s.trim();

    // Handle explicit 0x prefix
    if let Some(hex_str) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return usize::from_str_radix(hex_str, 16).ok();
    }

    // Parse as hex without prefix
    usize::from_str_radix(s, 16).ok()
}

// =============================== HELPER FUNCTIONS ===============================

/// Checks if the file has a .hex extension (case-insensitive)
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

/// Validates that a path exists and is a file
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

/// Finds the value after a specific flag (e.g., "--gap-fill 0xFF")
fn get_flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|pos| args.get(pos + 1))
        .cloned()
}
