use intelhex_parser::IntelHex;

fn main() {
    // Example 1
    // let mut ih1 = IntelHex::new();
    // ih1.load_hex("src/ex.hex").unwrap();
    // ih1.write_hex("src/ih1_example_regen.hex").unwrap();

    // Example 2
    let mut ih2 = IntelHex::from_hex("src/ih_example.hex").unwrap();
    ih2.write_hex("src/ih2_example_regen.hex").unwrap();
}
