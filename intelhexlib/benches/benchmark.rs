use criterion::{Criterion, criterion_group, criterion_main};
use intelhexlib::IntelHex;

#[allow(clippy::expect_used)]
fn bench_intelhex_parsing(c: &mut Criterion) {
    let input_path = "../build/random_data_1MB.hex";

    // IntelHex Parsing
    c.bench_function("intelhex_parse_1mb", |b| {
        let hex_bytes = std::fs::read(input_path).expect("Failed to read IntelHex file");

        b.iter(|| {
            #[cfg(feature = "benchmarking")]
            let mut ih = IntelHex::new();
            IntelHex::bench_priv_parse(
                std::hint::black_box(&mut ih),
                std::hint::black_box(&hex_bytes),
            );
            std::hint::black_box(&ih);
        });
    });

    c.bench_function("intelhex_load_hex", |b| {
        b.iter(|| {
            let mut ih = IntelHex::new();
            ih.load_hex(std::hint::black_box(&input_path))
                .expect("Failed to load IntelHex file");
            std::hint::black_box(ih);
        });
    });

    c.bench_function("intelhex_load_bin", |b| {
        b.iter(|| {
            let mut ih = IntelHex::new();
            ih.load_bin(std::hint::black_box("tests/fixtures/ih_valid_1.bin"), 0xF0)
                .expect("Failed to load bin file");
            std::hint::black_box(ih);
        });
    });
}

criterion_group!(
    name = intelhexlib_benches;
    config = Criterion::default().sample_size(20);
    targets = bench_intelhex_parsing
);
criterion_main!(intelhexlib_benches);
