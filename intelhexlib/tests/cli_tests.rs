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
            && stdout.contains("239 bytes")
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
        stderr.contains("Argument '--address' is not a HEX output path"),
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
        stderr.contains("Missing '--address' flag or the value after it"),
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
        stderr.contains("Usage: ihex merge <output> <input1>[:addr]"),
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
