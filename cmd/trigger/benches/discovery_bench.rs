use criterion::{criterion_group, criterion_main, Criterion};
use trigger::{discovery::Discoverer, platform::Platform};

fn bench_platform_detection(c: &mut Criterion) {
    c.bench_function("Platform::detect", |b| b.iter(Platform::detect));
}

fn bench_platform_get_cached(c: &mut Criterion) {
    c.bench_function("Platform::get (cached singleton)", |b| {
        b.iter(Platform::get)
    });
}

fn bench_discover_apps(c: &mut Criterion) {
    let discoverer = Discoverer::default();
    c.bench_function("Discoverer::discover_apps", |b| {
        b.iter(|| discoverer.discover_apps())
    });
}

fn bench_discover_handlers(c: &mut Criterion) {
    let discoverer = Discoverer::default();
    c.bench_function("Discoverer::discover_handlers", |b| {
        b.iter(|| discoverer.discover_handlers())
    });
}

criterion_group!(
    benches,
    bench_platform_detection,
    bench_platform_get_cached,
    bench_discover_apps,
    bench_discover_handlers,
);
criterion_main!(benches);
