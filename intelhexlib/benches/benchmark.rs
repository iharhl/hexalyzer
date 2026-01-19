use criterion::{Criterion, criterion_group, criterion_main};
use intelhexlib::IntelHex;

#[allow(clippy::expect_used)]
fn bench_intelhex_parsing(c: &mut Criterion) {
    let hex_1mb = "../build/random_data_1MB.hex";
    let bin_1mb = "../build/random_data_1MB.bin";
    let hex_sparse = "../build/random_data_sparse.hex";

    // IntelHex Parsing
    c.bench_function("intelhex_parse_1mb", |b| {
        let hex_bytes = std::fs::read(hex_1mb).expect("Failed to read IntelHex file");

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

    c.bench_function("intelhex_parse_sparse", |b| {
        let hex_bytes = std::fs::read(hex_sparse).expect("Failed to read IntelHex file");

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
            ih.load_hex(std::hint::black_box(&hex_1mb))
                .expect("Failed to load IntelHex file");
            std::hint::black_box(ih);
        });
    });

    c.bench_function("intelhex_load_bin", |b| {
        b.iter(|| {
            let mut ih = IntelHex::new();
            ih.load_bin(std::hint::black_box(bin_1mb), 0xF0)
                .expect("Failed to load bin file");
            std::hint::black_box(ih);
        });
    });

    c.bench_function("intelhex_search", |b| {
        let ih = IntelHex::from_hex(hex_1mb).expect("Failed to load IntelHex file");
        let pattern = vec![0xCC, 0x59, 0x6B];

        b.iter(|| {
            let addrs = ih.search_bytes(&pattern);
            std::hint::black_box(&addrs);
            std::hint::black_box(&ih);
        });
    });
}

criterion_group!(
    name = intelhexlib_benches;
    config = Criterion::default().sample_size(20);
    targets = bench_intelhex_parsing
);
criterion_main!(intelhexlib_benches);
