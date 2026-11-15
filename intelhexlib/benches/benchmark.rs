//! Benchmarks for `intelhexlib`.
//!
//! All test data is generated in memory from a fixed seed, so runs are reproducible
//! and no fixture files are needed. Files for the I/O group go to a temporary directory.
//!
//! Run everything:      `cargo bench -p intelhexlib --features benchmarking`
//! Run one group:       `cargo bench -p intelhexlib --features benchmarking -- search`
//! Compare to baseline: `... -- --save-baseline before`, then `... -- --baseline before`
//!
//! Groups:
//! - `parse`:  parsing Intel HEX text (sequential, descending segments, many small chunks)
//! - `io`:     loading and writing `.hex` / `.bin` files
//! - `search`: byte and regex search, including the all-`0xFF` worst cases
//! - `edit`:   merge, relocate, update and reads across gaps

#![allow(clippy::expect_used)]

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use intelhexlib::IntelHex;
use rand::rngs::StdRng;
use rand::{Rng, RngCore, SeedableRng};
use std::hint::black_box;
use std::path::PathBuf;

const KIB: usize = 1024;
const MIB: usize = 1024 * KIB;

/// Image sizes for size-parameterized benchmarks.
/// Two sizes make non-linear scaling visible (e.g. 16x data taking 100x time).
const SIZES: [usize; 2] = [MIB, 16 * MIB];

/// Seed for all generated data
const SEED: u64 = 0x4845_5841_4C59_5A52;

/// Bytes per data record in generated hex text (same as the library's default)
const RECORD_LEN: usize = 16;

/// Segment size used for the descending-order case
const SEGMENT_LEN: usize = 64 * KIB;

/// Chunk length and stride (chunk + gap) used for the many-chunks case
const CHUNK_LEN: usize = 16;
const CHUNK_STRIDE: usize = 32;

// =====================  DATA GENERATION  =====================

fn random_bytes(len: usize, seed: u64) -> Vec<u8> {
    let mut data = vec![0u8; len];
    StdRng::seed_from_u64(seed).fill_bytes(&mut data);
    data
}

/// Append one Intel HEX record (with checksum and newline) to `out`.
fn push_record(out: &mut Vec<u8>, rtype: u8, addr: u16, data: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    #[allow(clippy::cast_possible_truncation)]
    let len = data.len() as u8;
    let [addr_hi, addr_lo] = addr.to_be_bytes();
    let header = [len, addr_hi, addr_lo, rtype];

    let mut sum = 0u8;
    out.push(b':');
    for &b in header.iter().chain(data) {
        sum = sum.wrapping_add(b);
        out.push(HEX[usize::from(b >> 4)]);
        out.push(HEX[usize::from(b & 0x0F)]);
    }
    let checksum = sum.wrapping_neg();
    out.push(HEX[usize::from(checksum >> 4)]);
    out.push(HEX[usize::from(checksum & 0x0F)]);
    out.push(b'\n');
}

/// Encode `(address, data)` chunks as Intel HEX text, in the given order.
/// Records never cross a 64 KiB boundary; ELA records are emitted whenever
/// the upper 16 address bits change.
fn hex_text(chunks: &[(usize, &[u8])]) -> Vec<u8> {
    let payload: usize = chunks.iter().map(|(_, d)| d.len()).sum();
    // ~44 chars per 16-byte record, plus some slack for address records
    let mut out = Vec::with_capacity(payload * 3 + chunks.len() * 64);
    let mut upper: Option<usize> = None;

    for &(start, data) in chunks {
        let mut addr = start;
        let mut rest = data;

        while !rest.is_empty() {
            let addr_upper = addr >> 16;
            if upper != Some(addr_upper) {
                #[allow(clippy::cast_possible_truncation)]
                push_record(&mut out, 0x04, 0, &(addr_upper as u16).to_be_bytes());
                upper = Some(addr_upper);
            }

            let to_boundary = 0x1_0000 - (addr & 0xFFFF);
            let len = rest.len().min(RECORD_LEN).min(to_boundary);

            #[allow(clippy::cast_possible_truncation)]
            push_record(&mut out, 0x00, (addr & 0xFFFF) as u16, &rest[..len]);

            addr += len;
            rest = &rest[len..];
        }
    }

    push_record(&mut out, 0x01, 0, &[]);
    out
}

/// One contiguous image at address 0, records in ascending order.
fn hex_sequential(data: &[u8]) -> Vec<u8> {
    hex_text(&[(0, data)])
}

/// Same image, but emitted as 64 KiB segments in descending address order.
fn hex_descending_segments(data: &[u8]) -> Vec<u8> {
    let segments: Vec<(usize, &[u8])> = data
        .chunks(SEGMENT_LEN)
        .enumerate()
        .rev()
        .map(|(i, seg)| (i * SEGMENT_LEN, seg))
        .collect();
    hex_text(&segments)
}

/// `data` split into small chunks separated by gaps (`CHUNK_LEN` data every `CHUNK_STRIDE`).
/// `phase` shifts every chunk, so two images with phases 0 and `CHUNK_LEN` interleave exactly.
fn disjoint_chunks(data: &[u8], phase: usize) -> Vec<(usize, &[u8])> {
    data.chunks(CHUNK_LEN)
        .enumerate()
        .map(|(i, chunk)| (i * CHUNK_STRIDE + phase, chunk))
        .collect()
}

/// Parse generated hex text, checking that it is valid and holds `expected_len` bytes.
/// Done once outside the measured loop, so a broken generator cannot be timed as a fast error.
fn parse_checked(hex: &[u8], expected_len: usize) -> IntelHex {
    let mut ih = IntelHex::new();
    IntelHex::bench_priv_parse(&mut ih, hex).expect("Generated Intel HEX must parse");
    let len: usize = ih.iter().map(|(_, d)| d.len()).sum();
    assert_eq!(len, expected_len, "Parsed payload size mismatch");
    ih
}

fn size_label(size: usize) -> String {
    if size >= MIB {
        format!("{}MiB", size / MIB)
    } else {
        format!("{}KiB", size / KIB)
    }
}

/// Temporary directory for I/O benchmarks.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("intelhexlib-bench-{}", std::process::id()));
        std::fs::create_dir_all(&path).expect("Failed to create temp dir");
        Self(path)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// =====================  BENCHMARKS  =====================

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse");

    for size in SIZES {
        let data = random_bytes(size, SEED);
        let label = size_label(size);
        group.throughput(Throughput::Bytes(size as u64));

        let cases = [
            ("sequential", hex_sequential(&data)),
            ("descending_segments", hex_descending_segments(&data)),
            ("disjoint_chunks", hex_text(&disjoint_chunks(&data, 0))),
        ];

        for (name, hex) in &cases {
            parse_checked(hex, size);

            group.bench_with_input(BenchmarkId::new(*name, &label), hex, |b, hex| {
                b.iter(|| {
                    let mut ih = IntelHex::new();
                    let res = IntelHex::bench_priv_parse(&mut ih, black_box(hex));
                    black_box((res, ih))
                });
            });
        }
    }

    group.finish();
}

fn bench_io(c: &mut Criterion) {
    let mut group = c.benchmark_group("io");
    let dir = TempDir::new();

    for size in SIZES {
        let data = random_bytes(size, SEED);
        let label = size_label(size);
        group.throughput(Throughput::Bytes(size as u64));

        let hex_in = dir.file(&format!("in_{label}.hex"));
        let bin_in = dir.file(&format!("in_{label}.bin"));
        let hex_out = dir.file(&format!("out_{label}.hex"));
        let bin_out = dir.file(&format!("out_{label}.bin"));
        std::fs::write(&hex_in, hex_sequential(&data)).expect("Failed to write hex input");
        std::fs::write(&bin_in, &data).expect("Failed to write bin input");

        group.bench_function(BenchmarkId::new("load_hex", &label), |b| {
            b.iter(|| {
                let mut ih = IntelHex::new();
                ih.load_hex(black_box(&hex_in))
                    .expect("Failed to load hex file");
                black_box(ih)
            });
        });

        group.bench_function(BenchmarkId::new("load_bin", &label), |b| {
            b.iter(|| {
                let mut ih = IntelHex::new();
                ih.load_bin(black_box(&bin_in), 0)
                    .expect("Failed to load bin file");
                black_box(ih)
            });
        });

        // Writes are atomic: temp file + fsync + rename, all included in the timing
        let mut ih = IntelHex::from_hex(&hex_in).expect("Failed to load hex file");

        group.bench_function(BenchmarkId::new("write_hex", &label), |b| {
            b.iter(|| {
                ih.write_hex(black_box(&hex_out))
                    .expect("Failed to write hex");
            });
        });

        group.bench_function(BenchmarkId::new("write_bin", &label), |b| {
            b.iter(|| {
                ih.write_bin(black_box(&bin_out), 0xFF)
                    .expect("Failed to write bin");
            });
        });
    }

    group.finish();
}

fn bench_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("search");

    for size in SIZES {
        let label = size_label(size);
        group.throughput(Throughput::Bytes(size as u64));

        // Random image: few hits, measures the scan itself
        let data = random_bytes(size, SEED);
        let ih = parse_checked(&hex_sequential(&data), size);

        group.bench_function(BenchmarkId::new("bytes_3", &label), |b| {
            b.iter(|| ih.search_bytes(black_box(&[0xCC, 0x59, 0x6B])));
        });

        group.bench_function(BenchmarkId::new("ascii_regex", &label), |b| {
            b.iter(|| ih.search_ascii(black_box(r"[A-Z][a-z]{2}\d"), true));
        });

        // All-0xFF image (erased flash)
        let ff = vec![0xFFu8; size];
        let ih_ff = parse_checked(&hex_sequential(&ff), size);

        // A hit at every address
        group.bench_function(BenchmarkId::new("ff_every_byte", &label), |b| {
            b.iter(|| ih_ff.search_bytes(black_box(&[0xFF])));
        });

        // Long pattern that matches everywhere
        let long_pattern = vec![0xFFu8; 256];
        group.bench_function(BenchmarkId::new("ff_long_pattern_256", &label), |b| {
            b.iter(|| ih_ff.search_bytes(black_box(&long_pattern)));
        });
    }

    group.finish();
}

fn bench_edit(c: &mut Criterion) {
    let mut group = c.benchmark_group("edit");

    for size in SIZES {
        let label = size_label(size);
        group.throughput(Throughput::Bytes(size as u64));

        let data_a = random_bytes(size, SEED);
        let data_b = random_bytes(size, SEED + 1);
        let ih_a = parse_checked(&hex_text(&[(0, &data_a)]), size);

        // Two fragmented images whose chunks interleave and fuse into one chunk
        let half = size / 2;
        let ih_even = parse_checked(&hex_text(&disjoint_chunks(&data_a[..half], 0)), half);
        let ih_odd = parse_checked(
            &hex_text(&disjoint_chunks(&data_b[..half], CHUNK_LEN)),
            half,
        );

        group.bench_function(BenchmarkId::new("merge_safe_interleaved", &label), |b| {
            b.iter_batched(
                || ih_even.clone(),
                |mut ih| {
                    ih.merge_safe(black_box(&ih_odd)).expect("Merge failed");
                    ih
                },
                BatchSize::LargeInput,
            );
        });

        // Relocating a fragmented image rebuilds the whole chunk map
        group.bench_function(BenchmarkId::new("relocate_fragmented", &label), |b| {
            b.iter_batched(
                || ih_even.clone(),
                |mut ih| {
                    ih.relocate(black_box(0x1000)).expect("Relocate failed");
                    ih
                },
                BatchSize::LargeInput,
            );
        });

        // Reading a span that is half data, half gaps (as the GUI does for a selection)
        let span = ih_even.get_max_addr().expect("Image is empty") + 1;
        group.bench_function(BenchmarkId::new("read_range_safe_gaps", &label), |b| {
            b.iter(|| ih_even.read_range_safe(black_box(0), black_box(span)));
        });

        // Scattered single-byte edits (e.g. a paste or a patch file)
        let mut rng = StdRng::seed_from_u64(SEED + 2);
        let updates: Vec<(usize, u8)> = (0..4096)
            .map(|_| (rng.random_range(0..size), rng.random()))
            .collect();
        let mut ih_upd = ih_a;

        // Throughput here is updates per second, not bytes
        group.throughput(Throughput::Elements(updates.len() as u64));
        group.bench_function(BenchmarkId::new("update_slice_4096", &label), |b| {
            b.iter(|| {
                ih_upd
                    .update_slice(black_box(&updates))
                    .expect("Update failed");
            });
        });
    }

    group.finish();
}

criterion_group!(
    name = intelhexlib_benches;
    config = Criterion::default().sample_size(20);
    targets = bench_parse, bench_io, bench_search, bench_edit
);
criterion_main!(intelhexlib_benches);
