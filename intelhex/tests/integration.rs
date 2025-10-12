use intelhex::{IntelHex, IntelHexError};
use std::fs;


#[test]
fn test_from_hex() {
    // Define in/out paths
    let input_path = "tests/fixtures/ih_example_2.hex";
    let output_path = "build/t1/ih.hex";

    // Load hex and generate a new one
    let mut ih = IntelHex::from_hex(input_path).unwrap();
    ih.write_hex(output_path).unwrap();

    // Load them in memory (small files -> OK)
    let f1 = fs::read(input_path).unwrap();
    let f2 = fs::read(output_path).unwrap();

    // Assert contents (loaded as Vec) is the same
    assert_eq!(f1, f2);
}

#[test]
fn test_load_hex() {
    // Define in/out paths
    let input_path = "tests/fixtures/ih_example_2.hex";
    let output_path = "build/t2/ih.hex";

    // Load hex and generate a new one
    let mut ih = IntelHex::new();
    ih.load_hex(input_path).unwrap();
    ih.write_hex(output_path).unwrap();

    // Load them in memory (small files -> OK)
    let f1 = fs::read(input_path).unwrap();
    let f2 = fs::read(output_path).unwrap();

    // Assert contents (loaded as Vec) is the same
    assert_eq!(f1, f2);
}

#[test]
fn test_hex_parsing_returns_error() {
    // Define in/out paths
    let input_path = "tests/fixtures/ih_bad_checksum.hex";

    // Parse hex file
    let ih = IntelHex::from_hex(input_path);

    // Assert that the Result is Err
    if let Some(my_err) = ih.unwrap_err().downcast_ref::<IntelHexError>() {
        assert!(matches!(my_err, IntelHexError::RecordChecksumMismatch(0x55, 0xFF)));
    } else {
        assert!(false, "Should have failed with error...");
    }
}

