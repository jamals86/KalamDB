pub mod chat_realtime_bench;

use crate::benchmarks::Benchmark;

pub fn benchmarks() -> Vec<Box<dyn Benchmark>> {
    vec![Box::new(chat_realtime_bench::ChatRealtimeBench)]
}
