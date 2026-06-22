use criterion::{criterion_group, criterion_main, Criterion};
use benchmarks::setup_benchmark_scheduler;

fn bench_scheduler(c: &mut Criterion) {
    let mut sched = setup_benchmark_scheduler(5);
    
    c.bench_function("scheduler_schedule_tick", |b| {
        b.iter(|| {
            sched.schedule(true);
        })
    });
}

criterion_group!(benches, bench_scheduler);
criterion_main!(benches);
