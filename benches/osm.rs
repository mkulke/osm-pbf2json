use criterion::{criterion_group, criterion_main, Criterion};
use osm_pbf2json::{filter, objects, streets};
use std::fs::File;

pub fn process_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("alexanderplatz");
    group.sample_size(10);
    let groups = filter::parse("amenity");
    group.bench_function("process", |b| {
        b.iter(|| {
            let file = File::open("./tests/data/alexanderplatz.pbf").unwrap();
            objects(file, &groups).unwrap();
        })
    });
    group.finish();
}

pub fn streets_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("alexanderplatz");
    group.sample_size(10);
    group.bench_function("streets", |b| {
        b.iter(|| {
            let file = File::open("./tests/data/alexanderplatz.pbf").unwrap();
            streets(file, None, None).unwrap();
        })
    });
    group.finish();
}

criterion_group!(benches, process_bench, streets_bench);
criterion_main!(benches);
