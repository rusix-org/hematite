//! Inspect actual QEMU trees with the same safe parser linked into the kernel.
#![forbid(unsafe_code)]

use hm_kernel::cpu_topology::{Error, parse};

fn main() {
    let paths: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(paths.len(), 3);
    for (index, path) in paths.iter().enumerate() {
        let cpus = index + 1;
        let bytes = std::fs::read(path).expect("read generated DTB");
        let parsed = parse(&bytes);
        if cpus <= 2 {
            let topology = parsed.expect("supported QEMU topology");
            assert_eq!(topology.cpu_count(), cpus);
            assert_eq!(topology.affinities(), &([0, 1][..cpus]));
            assert!(topology.psci().is_some_and(|psci| psci.advertised_1_0));
        } else {
            assert_eq!(parsed, Err(Error::Unsupported), "excess CPUs must not be truncated");
        }
    }
    println!("HMTOPOLOGY HOST PASS profiles=3");
}
