use intelhexlib::{IntelHex, IntelHexError, IntelHexErrorKind};
use std::fs;

fn compare_files(path1: &str, path2: &str) -> bool {
    // Load them in memory (small files -> OK)
    let f1 = fs::read(path1);
    let f2 = fs::read(path2);

    // Verify both are Ok and their contents match
    f1.is_ok_and(|content1| f2.is_ok_and(|content2| content1 == content2))
}

#[test]
fn test_from_and_write_hex() {
    // Define in/out paths
    let input_path = "tests/fixtures/ih_valid_2.hex";
    let output_path = "build/t1/ih.hex";

    // Load hex and check the result
    let res = IntelHex::from_hex(input_path);
    assert!(res.is_ok());

    // If loaded Ok -> write it back to the disk
    if let Ok(mut ih) = res {
        let res = ih.write_hex(output_path);
        assert!(res.is_ok());

        assert!(compare_files(input_path, output_path));
    }
}

#[test]
fn test_load_and_write_hex() {
    // Define in/out paths
    let input_path = "tests/fixtures/ih_valid_2.hex";
    let output_path = "build/t2/ih.hex";

    // Load hex and check the resut
    let mut ih = IntelHex::new();
    let res = ih.load_hex(input_path);
    assert!(res.is_ok());

    let res = ih.write_hex(output_path);
    assert!(res.is_ok());

    assert!(compare_files(input_path, output_path));
}

#[test]
fn test_from_and_write_bin() {
    // Define in/out paths
    let input_path = "tests/fixtures/ih_valid_1.bin";
    let output_path = "build/t3/ih.hex";

    // Load hex and check the result
    let base_addr = 0x1000;
    let res = IntelHex::from_bin(input_path, base_addr);
    assert!(res.is_ok());

    // If loaded Ok -> write it back to the disk
    if let Ok(mut ih) = res {
        let res = ih.write_bin(output_path, 0x00);
        assert!(res.is_ok());

        assert!(compare_files(input_path, output_path));
    }
}

#[test]
fn test_load_and_write_bin() {
    // Define in/out paths
    let input_path = "tests/fixtures/ih_valid_1.bin";
    let output_path = "build/t4/ih.bin";

    // Load hex and check the result
    let base_addr = 0x1000;
    let mut ih = IntelHex::new();
    let res = ih.load_bin(input_path, base_addr);
    assert!(res.is_ok());

    let res = ih.write_bin(output_path, 0x00);
    assert!(res.is_ok());

    assert!(compare_files(input_path, output_path));
}

#[test]
fn test_load_bin_and_write_hex() {
    // Define in/out paths
    let input_path = "tests/fixtures/ih_valid_1.bin";
    let output_path = "build/t5/ih.hex";

    // Load hex and check the result
    let base_addr = 0x1000;
    let mut ih = IntelHex::new();
    let res = ih.load_bin(input_path, base_addr);
    assert!(res.is_ok());

    let res = ih.write_hex(output_path);
    assert!(res.is_ok());
}

#[test]
fn test_load_hex_and_write_bin() {
    // Define in/out paths
    let input_path = "tests/fixtures/ih_valid_1.hex";
    let output_path = "build/t6/ih.bin";

    // Load hex and check the result
    let mut ih = IntelHex::new();
    let res = ih.load_hex(input_path);
    assert!(res.is_ok());

    let res = ih.write_bin(output_path, 0x00);
    assert!(res.is_ok());
}

#[test]
#[allow(clippy::panic)]
fn test_hex_parsing_returns_error() {
    // Define in/out paths
    let input_path = "tests/fixtures/ih_bad_checksum.hex";

    // Parse hex file
    let res = IntelHex::from_hex(input_path);

    // Check the error
    match res {
        Err(e) => {
            if let Some(ih_err) = e.downcast_ref::<IntelHexError>() {
                assert_eq!(
                    ih_err,
                    &IntelHexError::ParseRecordError(
                        IntelHexErrorKind::RecordChecksumMismatch(0x55, 0xFF),
                        1
                    )
                );
            } else {
                panic!("Error was not an IntelHexError");
            }
        }
        Ok(_) => panic!("Expected an error, but got Ok"),
    }
}
