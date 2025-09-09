use intelhex_parser::IntelHex;
use std::fs;

#[test]
fn test_from_hex() {
    // Define in/out paths
    let input_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ih_example_2.hex");
    let output_path = concat!(env!("CARGO_MANIFEST_DIR"), "/build/t1/ih.hex");

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
    let input_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ih_example_2.hex");
    let output_path = concat!(env!("CARGO_MANIFEST_DIR"), "/build/t2/ih.hex");

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
