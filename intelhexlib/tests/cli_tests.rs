#![cfg(feature = "cli")]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use intelhexlib::IntelHex;
use std::path::PathBuf;
use std::process::Command;

const HEXCLI_EXE: &str = env!("CARGO_BIN_EXE_hexcli");

#[test]
fn test_ihex_shows_help() {
    // Act
    let output = Command::new(HEXCLI_EXE)
        .arg("--help")
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Usage"),
        "stdout did not look like help text:\n{stdout}"
    );

    // Act
    let output = Command::new(HEXCLI_EXE)
        .arg("help")
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Usage"),
        "stdout did not look like help text:\n{stdout}"
    );

    // Act
    let output = Command::new(HEXCLI_EXE)
        .arg("-h")
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Usage"),
        "stdout did not look like help text:\n{stdout}"
    );
}

#[test]
fn test_ihex_shows_info_valid() {
    // Arrange
    let path_str = "tests/fixtures/ih_valid_1.hex";

    // Act
    let output = Command::new(HEXCLI_EXE)
        .args(["info", path_str])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let abs_path = std::fs::canonicalize(PathBuf::from(path_str))
        .unwrap_or_else(|_| panic!("Error during retrieval of absolute file path: {path_str}"));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(abs_path.to_string_lossy().as_ref())
            && stdout.contains("68 bytes")
            && stdout.contains("0x0000_0000 - 0x0001_C23F"),
        "stdout did not look like info text:\n{stdout}"
    );
}

#[test]
fn test_ihex_shows_info_invalid() {
    // Arrange
    let path_str = "tests/cli_tests.rs";

    // Act
    let output = Command::new(HEXCLI_EXE)
        .args(["info", path_str])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(!output.status.success());

    let abs_path = std::fs::canonicalize(PathBuf::from(path_str))
        .unwrap_or_else(|_| panic!("Error during retrieval of absolute file path: {path_str}"));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("File type not supported")
            && stderr.contains(abs_path.to_string_lossy().as_ref()),
        "stderr did not contain expected error text:\n{stderr}"
    );
}

#[test]
fn test_ihex_relocate_valid() {
    // Arrange
    let in_path_str = "tests/fixtures/ih_valid_2.hex";
    let out_path_str = "build/t1-cli/ih.hex";

    // Act
    let output = Command::new(HEXCLI_EXE)
        .args(["relocate", in_path_str, out_path_str, "--address", "0xFF00"])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let abs_path = std::fs::canonicalize(PathBuf::from(in_path_str))
        .expect("Error during retrieval of absolute file path: {path_str}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("to 0xFF00") && stdout.contains(abs_path.to_string_lossy().as_ref()),
        "stdout did not look like relocation text:\n{stdout}"
    );

    let ih = IntelHex::from_hex(PathBuf::from(out_path_str)).unwrap_or_default();
    assert_eq!(ih.get_min_addr().unwrap_or_default(), 0xFF00);
    assert_eq!(ih.get_max_addr().unwrap_or_default(), 0xFF3F);
}

#[test]
fn test_ihex_relocate_invalid() {
    // Arrange
    let in_path_str = "tests/fixtures/ih_valid_2.hex";

    // Act - missing input path
    let output = Command::new(HEXCLI_EXE)
        .args(["relocate"])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Missing input path"),
        "stderr did not contain expected error text:\n{stderr}"
    );

    // Act - missing output path
    let output = Command::new(HEXCLI_EXE)
        .args(["relocate", in_path_str])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Missing output path"),
        "stderr did not contain expected error text:\n{stderr}"
    );

    // Act - missing output path but address flag present
    let output = Command::new(HEXCLI_EXE)
        .args(["relocate", in_path_str, "--address", "0xFF00"])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Missing output path"),
        "stderr did not contain expected error text:\n{stderr}"
    );

    // Act - output file is BIN
    let output = Command::new(HEXCLI_EXE)
        .args(["relocate", in_path_str, "build/t2-cli/ih.bin"])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Argument 'build/t2-cli/ih.bin' is not a HEX output path"),
        "stderr did not contain expected error text:\n{stderr}"
    );

    // Act - no address flag given
    let output = Command::new(HEXCLI_EXE)
        .args(["relocate", in_path_str, "build/t2-cli/ih.hex"])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Missing '--address' flag or the value after it"),
        "stderr did not contain expected error text:\n{stderr}"
    );

    // Act - no value given after address flag
    let output = Command::new(HEXCLI_EXE)
        .args(["relocate", in_path_str, "build/t2-cli/ih.hex", "--address"])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Missing value after '--address'"),
        "stderr did not contain expected error text:\n{stderr}"
    );
}

#[test]
fn test_ihex_convert_valid() {
    // Arrange
    let in_path_str = "tests/fixtures/ih_valid_3.hex";
    let out_path_str = "build/t2-cli/ih1.bin";

    // Act
    let output = Command::new(HEXCLI_EXE)
        .args(["convert", in_path_str, out_path_str])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let abs_path_in = std::fs::canonicalize(PathBuf::from(in_path_str))
        .unwrap_or_else(|_| panic!("Failed retrieving absolute file path: {in_path_str}"));
    let abs_path_out = std::fs::canonicalize(PathBuf::from(out_path_str))
        .unwrap_or_else(|_| panic!("Failed retrieving absolute file path: {out_path_str}"));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(abs_path_in.to_string_lossy().as_ref())
            && stdout.contains(abs_path_out.to_string_lossy().as_ref()),
        "stdout did not look like convert text:\n{stdout}"
    );

    // Arrange
    let out_path_str = "build/t2-cli/ih2.bin";

    // Act
    let output = Command::new(HEXCLI_EXE)
        .args(["convert", in_path_str, out_path_str, "--gap-fill", "0xFF"])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let abs_path_out = std::fs::canonicalize(PathBuf::from(out_path_str))
        .unwrap_or_else(|_| panic!("Failed retrieving absolute file path: {out_path_str}"));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(abs_path_in.to_string_lossy().as_ref())
            && stdout.contains(abs_path_out.to_string_lossy().as_ref()),
        "stdout did not look like convert text:\n{stdout}"
    );

    // Arrange
    let in_path_str = "tests/fixtures/ih_valid_1.bin";
    let out_path_str = "build/t2-cli/ih.hex";

    // Act
    let output = Command::new(HEXCLI_EXE)
        .args(["convert", in_path_str, out_path_str, "--address", "0x0"])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let abs_path_in = std::fs::canonicalize(PathBuf::from(in_path_str))
        .unwrap_or_else(|_| panic!("Failed retrieving absolute file path: {in_path_str}"));
    let abs_path_out = std::fs::canonicalize(PathBuf::from(out_path_str))
        .unwrap_or_else(|_| panic!("Failed retrieving absolute file path: {out_path_str}"));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(abs_path_in.to_string_lossy().as_ref())
            && stdout.contains(abs_path_out.to_string_lossy().as_ref()),
        "stdout did not look like convert text:\n{stdout}"
    );
}

#[test]
fn test_ihex_convert_invalid() {
    // Arrange
    let in_path_str = "tests/fixtures/ih_valid_3.hex";
    let out_path_str = "build/t3-cli/ih.rl";

    // Act - unsupported output file type
    let output = Command::new(HEXCLI_EXE)
        .args(["convert", in_path_str, out_path_str])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Input or output files are of unsupported type"),
        "stderr did not contain expected error text:\n{stderr}"
    );

    // Arrange
    let out_path_str = "build/t3-cli/ih.hex";

    // Act - output file type same as input
    let output = Command::new(HEXCLI_EXE)
        .args(["convert", in_path_str, out_path_str])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Cannot convert between the same file type"),
        "stderr did not contain expected error text:\n{stderr}"
    );

    // Arrange
    let out_path_str = "build/t3-cli/ih.bin";

    // Act - provided address flag for HEX to BIN conversion
    let output = Command::new(HEXCLI_EXE)
        .args(["convert", in_path_str, out_path_str, "--address", "0x0"])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Base address '--address' is only supported for BIN to HEX conversion"),
        "stderr did not contain expected error text:\n{stderr}"
    );

    // Arrange
    let out_path_str = "build/t3-cli/ih.bin";

    // Act - provided address flag for HEX to BIN conversion
    let output = Command::new(HEXCLI_EXE)
        .args(["convert", in_path_str, out_path_str, "--address", "0x0"])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Base address '--address' is only supported for BIN to HEX conversion"),
        "stderr did not contain expected error text:\n{stderr}"
    );

    // Arrange
    let in_path_str = "tests/fixtures/ih_valid_1.bin";
    let out_path_str = "build/t3-cli/ih.hex";

    // Act - not provided address flag for BIN to HEX conversion
    let output = Command::new(HEXCLI_EXE)
        .args(["convert", in_path_str, out_path_str])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Base address '--address' is required for BIN to HEX conversion"),
        "stderr did not contain expected error text:\n{stderr}"
    );

    // Act - provided gap fill flag for BIN to HEX conversion
    let output = Command::new(HEXCLI_EXE)
        .args([
            "convert",
            in_path_str,
            out_path_str,
            "--address",
            "0x00",
            "--gap-fill",
            "0x00",
        ])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Gap fill '--gap-fill' is only supported for HEX to BIN conversion"),
        "stderr did not contain expected error text:\n{stderr}"
    );

    // Act - provided gap fill flag for BIN to HEX conversion
    let output = Command::new(HEXCLI_EXE)
        .args(["convert", in_path_str, out_path_str, "--address", "zz"])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Invalid address"),
        "stderr did not contain expected error text:\n{stderr}"
    );
}

#[test]
fn test_ihex_merge_valid() {
    // Arrange
    let in_path_str_1 = "tests/fixtures/ih_valid_1.hex";
    let in_path_str_2 = "tests/fixtures/ih_valid_3.hex";
    let out_path_str = "build/t3-cli/ih1.hex";

    // Act
    let output = Command::new(HEXCLI_EXE)
        .args(["merge", out_path_str, in_path_str_1, in_path_str_2])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let abs_path_out = std::fs::canonicalize(PathBuf::from(out_path_str))
        .unwrap_or_else(|_| panic!("Failed retrieving absolute file path: {out_path_str}"));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(abs_path_out.to_string_lossy().as_ref()),
        "stdout did not look like merge text:\n{stdout}"
    );

    // Arrange
    let in_path_str_1 = "tests/fixtures/ih_valid_1.hex";
    let in_path_str_2 = "tests/fixtures/ih_valid_3.hex";
    let out_path_str = "build/t3-cli/ih2.hex";

    // Act
    let output = Command::new(HEXCLI_EXE)
        .args([
            "merge",
            out_path_str,
            format!("{in_path_str_1}:0x1000").as_str(),
            format!("{in_path_str_2}:0xFF00").as_str(),
        ])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let abs_path_out = std::fs::canonicalize(PathBuf::from(out_path_str))
        .unwrap_or_else(|_| panic!("Failed retrieving absolute file path: {out_path_str}"));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(abs_path_out.to_string_lossy().as_ref()),
        "stdout did not look like merge text:\n{stdout}"
    );

    // Arrange
    let in_path_str_1 = "tests/fixtures/ih_valid_1.hex";
    let in_path_str_2 = "tests/fixtures/ih_valid_2.hex";
    let in_path_str_3 = "tests/fixtures/ih_valid_1.bin";
    let out_path_str = "build/t3-cli/ih1.bin";

    // Act
    let output = Command::new(HEXCLI_EXE)
        .args([
            "merge",
            out_path_str,
            in_path_str_1,
            format!("{in_path_str_2}:0x3000").as_str(),
            format!("{in_path_str_3}:0xFFF0").as_str(),
            "--gap-fill",
            "0x00",
            "--overwrite",
        ])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let abs_path_out = std::fs::canonicalize(PathBuf::from(out_path_str))
        .unwrap_or_else(|_| panic!("Failed retrieving absolute file path: {out_path_str}"));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(abs_path_out.to_string_lossy().as_ref()),
        "stdout did not look like merge text:\n{stdout}"
    );
}

#[test]
fn test_ihex_merge_invalid() {
    // Arrange
    let in_path_str = "tests/fixtures/ih_valid_1.hex";
    let out_path_str = "build/t4-cli/ih.hex";

    // Act - not enough input args
    let output = Command::new(HEXCLI_EXE)
        .args(["merge", out_path_str, in_path_str])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage: hexcli merge <output> <input1>[:addr]"),
        "stderr did not contain expected error text:\n{stderr}"
    );

    // Arrange
    let in_path_str_1 = "tests/fixtures/ih_valid_1.hex";
    let in_path_str_2 = "tests/fixtures/ih_valid_2.hex";

    // Act - invalid address
    let output = Command::new(HEXCLI_EXE)
        .args([
            "merge",
            out_path_str,
            format!("{in_path_str_1}:0xZZ").as_str(),
            in_path_str_2,
        ])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Invalid address"),
        "stderr did not contain expected error text:\n{stderr}"
    );

    // Arrange
    let in_path_str_2 = "tests/fixtures/ih_valid_1.bin";

    // Act - no base addr for BIN file
    let output = Command::new(HEXCLI_EXE)
        .args(["merge", out_path_str, in_path_str_1, in_path_str_2])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Base address required for binary file")
            && stderr.contains("ih_valid_1.bin"),
        "stderr did not contain expected error text:\n{stderr}"
    );

    // Arrange
    let in_path_str_2 = "tests/cli_tests.rs";

    // Act - unsupported file type
    let output = Command::new(HEXCLI_EXE)
        .args(["merge", out_path_str, in_path_str_1, in_path_str_2])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Unsupported file type") && stderr.contains("cli_tests.rs"),
        "stderr did not contain expected error text:\n{stderr}"
    );
}

#[test]
fn test_ihex_flags_any_position() {
    // ih_valid_2.hex covers 0x100..=0x13F and ih_valid_3.hex is moved to 0x200..=0x233,
    // so a merged BIN is 0x134 bytes long with a gap at 0x140..=0x1FF
    fn assert_merged_bin(path: &str, gap_fill: u8) {
        let bytes = std::fs::read(path).unwrap_or_else(|_| panic!("Missing output: {path}"));
        assert_eq!(bytes.len(), 0x134, "an input is missing from {path}");
        assert!(
            bytes[0x40..0x100].iter().all(|&b| b == gap_fill),
            "gap in {path} is not filled with 0x{gap_fill:02X}"
        );
    }

    let in_path_str_1 = "tests/fixtures/ih_valid_2.hex";
    let in_path_str_2 = "tests/fixtures/ih_valid_3.hex:0x200";

    // Act - merge with the flag before the output path
    let out_path_str = "build/t5-cli/flag_first.bin";
    let output = Command::new(HEXCLI_EXE)
        .args([
            "merge",
            "--gap-fill",
            "0x00",
            out_path_str,
            in_path_str_1,
            in_path_str_2,
        ])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_merged_bin(out_path_str, 0x00);

    // Act - merge with the flag between the inputs
    let out_path_str = "build/t5-cli/flag_between.bin";
    let output = Command::new(HEXCLI_EXE)
        .args([
            "merge",
            out_path_str,
            in_path_str_1,
            "--gap-fill",
            "0x11",
            in_path_str_2,
        ])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_merged_bin(out_path_str, 0x11);

    // Act - convert with the flag before the paths
    let out_path_str = "build/t5-cli/convert.bin";
    let output = Command::new(HEXCLI_EXE)
        .args(["convert", "--gap-fill", "0x00", in_path_str_1, out_path_str])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let len = std::fs::metadata(out_path_str).map_or(0, |m| m.len());
    assert_eq!(len, 0x40);

    // Act - relocate with the flag before the paths
    let out_path_str = "build/t5-cli/relocate.hex";
    let output = Command::new(HEXCLI_EXE)
        .args([
            "relocate",
            "--address",
            "0xFF00",
            in_path_str_1,
            out_path_str,
        ])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ih = IntelHex::from_hex(PathBuf::from(out_path_str)).unwrap_or_default();
    assert_eq!(ih.get_min_addr().unwrap_or_default(), 0xFF00);

    // Act - everything after a bare `--` is a path, not a flag
    let out_path_str = "build/t5-cli/after_dashes.bin";
    let output = Command::new(HEXCLI_EXE)
        .args([
            "merge",
            "--gap-fill",
            "0x22",
            "--",
            out_path_str,
            in_path_str_1,
            in_path_str_2,
        ])
        .output()
        .expect("Failed to run ihex");

    // Assert
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_merged_bin(out_path_str, 0x22);
}

#[test]
fn test_ihex_flags_invalid() {
    fn assert_fails_with(args: &[&str], expected: &str) {
        let output = Command::new(HEXCLI_EXE)
            .args(args)
            .output()
            .expect("Failed to run ihex");

        assert!(!output.status.success(), "command succeeded: {args:?}");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "stderr did not contain '{expected}' for {args:?}:\n{stderr}"
        );
    }

    let in_path_str_1 = "tests/fixtures/ih_valid_2.hex";
    let in_path_str_2 = "tests/fixtures/ih_valid_3.hex:0x200";
    let out_bin_str = "build/t6-cli/ih.bin";
    let out_hex_str = "build/t6-cli/ih.hex";
    std::fs::remove_file(out_bin_str).unwrap_or_default();
    std::fs::remove_file(out_hex_str).unwrap_or_default();

    // Act & Assert - unknown flag
    assert_fails_with(
        &["convert", in_path_str_1, out_bin_str, "--bogus"],
        "Unknown option '--bogus'",
    );

    // Act & Assert - unknown flag between merge inputs (used to drop the next input)
    assert_fails_with(
        &[
            "merge",
            out_bin_str,
            in_path_str_1,
            "--bogus",
            in_path_str_2,
        ],
        "Unknown option '--bogus'",
    );

    // Act & Assert - flag not supported by the command
    assert_fails_with(
        &[
            "relocate",
            in_path_str_1,
            out_hex_str,
            "--address",
            "0xFF00",
            "--gap-fill",
            "0x00",
        ],
        "Option '--gap-fill' is not supported by 'relocate'",
    );

    // Act & Assert - repeated flag
    assert_fails_with(
        &[
            "merge",
            out_bin_str,
            in_path_str_1,
            in_path_str_2,
            "--gap-fill",
            "0x00",
            "--gap-fill",
            "0x11",
        ],
        "Option '--gap-fill' is given more than once",
    );

    // Act & Assert - extra positional argument
    assert_fails_with(
        &["convert", in_path_str_1, out_bin_str, "extra.bin"],
        "Unexpected argument 'extra.bin'",
    );

    // Act & Assert - switch not supported by the command
    assert_fails_with(
        &["convert", in_path_str_1, out_bin_str, "--overwrite"],
        "Option '--overwrite' is not supported by 'convert'",
    );

    // Act & Assert - repeated switch
    assert_fails_with(
        &[
            "merge",
            out_hex_str,
            in_path_str_1,
            in_path_str_2,
            "--overwrite",
            "--overwrite",
        ],
        "Option '--overwrite' is given more than once",
    );

    // None of the failed commands should have written an output file
    assert!(!PathBuf::from(out_bin_str).exists());
    assert!(!PathBuf::from(out_hex_str).exists());
}

// ================================ MERGE CONFLICTS ================================

/// Output directory of the merge conflict tests.
const MERGE_OUT_DIR: &str = "build/t7-cli";

/// Hex file whose start address record (0xAABBCCDD) differs from the one in `ih_valid_2.hex`.
const OTHER_START_ADDR_HEX: &str = ":04000005AABBCCDDE9\n:01200000AB34\n:00000001FF\n";

/// Run `hexcli` with `args`.
fn run_hexcli(args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new(HEXCLI_EXE).args(args).output()
}

/// Create the output directory and return the path of `name` inside it,
/// with any file left there by a previous run removed.
fn merge_out_path(name: &str) -> std::io::Result<String> {
    std::fs::create_dir_all(MERGE_OUT_DIR)?;
    let path = format!("{MERGE_OUT_DIR}/{name}");
    std::fs::remove_file(&path).unwrap_or_default();
    Ok(path)
}

/// Write a hex file whose start address record differs from the one in `ih_valid_2.hex`.
fn write_other_start_addr_hex(name: &str) -> std::io::Result<String> {
    let path = merge_out_path(name)?;
    std::fs::write(&path, OTHER_START_ADDR_HEX)?;
    Ok(path)
}

#[test]
fn test_ihex_merge_overlap_fails_by_default() -> Result<(), Box<dyn std::error::Error>> {
    // Arrange: both files have data at 0x100
    let out_path_str = merge_out_path("overlap_default.hex")?;

    // Act
    let output = run_hexcli(&[
        "merge",
        &out_path_str,
        "tests/fixtures/ih_valid_2.hex",
        "tests/fixtures/ih_valid_3.hex",
    ])?;

    // Assert
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ih_valid_3.hex overlaps an earlier input at 0x100")
            && stderr.contains("--overwrite"),
        "stderr did not contain expected error text:\n{stderr}"
    );
    assert!(!PathBuf::from(&out_path_str).exists());
    Ok(())
}

#[test]
fn test_ihex_merge_overlap_with_overwrite() -> Result<(), Box<dyn std::error::Error>> {
    // Arrange: both files have data at 0x100
    let out_path_str = merge_out_path("overlap_overwrite.hex")?;

    // Act
    let output = run_hexcli(&[
        "merge",
        &out_path_str,
        "tests/fixtures/ih_valid_2.hex",
        "tests/fixtures/ih_valid_3.hex",
        "--overwrite",
    ])?;

    // Assert: the later input wins at 0x100
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let merged = IntelHex::from_hex(&out_path_str)?;
    let later = IntelHex::from_hex("tests/fixtures/ih_valid_3.hex")?;
    assert_eq!(merged.read_byte(0x100), later.read_byte(0x100));
    Ok(())
}

#[test]
fn test_ihex_merge_differing_start_addr_fails_by_default() -> Result<(), Box<dyn std::error::Error>>
{
    // Arrange
    let other_path_str = write_other_start_addr_hex("start_addr_default_in.hex")?;
    let out_path_str = merge_out_path("start_addr_default.hex")?;

    // Act
    let output = run_hexcli(&[
        "merge",
        &out_path_str,
        "tests/fixtures/ih_valid_2.hex",
        &other_path_str,
    ])?;

    // Assert
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("start_addr_default_in.hex has a start address record that differs")
            && stderr.contains("--overwrite"),
        "stderr did not contain expected error text:\n{stderr}"
    );
    assert!(!PathBuf::from(&out_path_str).exists());
    Ok(())
}

#[test]
fn test_ihex_merge_differing_start_addr_with_overwrite() -> Result<(), Box<dyn std::error::Error>> {
    // Arrange
    let other_path_str = write_other_start_addr_hex("start_addr_overwrite_in.hex")?;
    let out_path_str = merge_out_path("start_addr_overwrite.hex")?;

    // Act
    let output = run_hexcli(&[
        "merge",
        &out_path_str,
        "tests/fixtures/ih_valid_2.hex",
        &other_path_str,
        "--overwrite",
    ])?;

    // Assert: the later input's start address record is kept
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let merged = IntelHex::from_hex(&out_path_str)?;
    let later = IntelHex::from_hex(&other_path_str)?;
    assert!(later.start_addr.is_some());
    assert_eq!(merged.start_addr, later.start_addr);
    Ok(())
}

#[test]
fn test_ihex_merge_identical_start_addr() -> Result<(), Box<dyn std::error::Error>> {
    // Arrange: the same file twice, the second copy moved so that the data does not overlap
    let out_path_str = merge_out_path("start_addr_identical.hex")?;

    // Act
    let output = run_hexcli(&[
        "merge",
        &out_path_str,
        "tests/fixtures/ih_valid_2.hex",
        "tests/fixtures/ih_valid_2.hex:0x2000",
    ])?;

    // Assert
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let merged = IntelHex::from_hex(&out_path_str)?;
    let input = IntelHex::from_hex("tests/fixtures/ih_valid_2.hex")?;
    assert_eq!(merged.start_addr, input.start_addr);
    Ok(())
}

#[test]
fn test_ihex_merge_relocated_start_addr_warns() -> Result<(), Box<dyn std::error::Error>> {
    // Arrange: ih_valid_2.hex has a start address record and its data starts at 0x100
    let out_path_str = merge_out_path("relocated_warns.hex")?;

    // Act
    let output = run_hexcli(&[
        "merge",
        &out_path_str,
        "tests/fixtures/ih_valid_2.hex:0x2000",
        "tests/fixtures/ih_valid_3.hex",
    ])?;

    // Assert: the merge still succeeds
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Warning: ")
            && stderr.contains("ih_valid_2.hex has a start address record that is not relocated"),
        "stderr did not contain expected warning:\n{stderr}"
    );
    Ok(())
}

#[test]
fn test_ihex_merge_unmoved_start_addr_no_warning() -> Result<(), Box<dyn std::error::Error>> {
    // Arrange: ih_valid_2.hex data already starts at 0x100, ih_valid_3.hex has no start record
    let out_path_str = merge_out_path("unmoved_no_warning.hex")?;

    // Act
    let output = run_hexcli(&[
        "merge",
        &out_path_str,
        "tests/fixtures/ih_valid_2.hex:0x100",
        "tests/fixtures/ih_valid_3.hex:0x2000",
    ])?;

    // Assert
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("Warning"),
        "stderr contained an unexpected warning:\n{stderr}"
    );
    Ok(())
}
