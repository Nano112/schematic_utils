use criterion::{criterion_group, criterion_main, Criterion};
use minecraft_schematic_utils::{BlockState, Region, UniversalSchematic};

fn benchmark_schematic_creation(c: &mut Criterion) {
    c.bench_function("create schematic", |b| {
        b.iter(|| {
            let schematic = UniversalSchematic::new("Test Schematic".to_string());
            schematic
        })
    });
}

fn benchmark_block_setting(c: &mut Criterion) {
    let mut schematic = UniversalSchematic::new("Test Schematic".to_string());
    let block = BlockState::new("minecraft:stone".to_string());

    c.bench_function("set block", |b| {
        b.iter(|| {
            schematic.set_block(0, 0, 0, block.clone());
        })
    });
}


fn benchmark_big_schematic_creation(c: &mut Criterion) {
    c.bench_function("create big schematic", |b| {
        b.iter(|| {
            let mut schematic = UniversalSchematic::new("Big Schematic".to_string());
            for x in 0..100 {
                for y in 0..100 {
                    for z in 0..100 {
                        schematic.set_block(x, y, z, BlockState::new("minecraft:stone".to_string()));
                    }
                }
            }
            schematic
        })
    });
}

fn benchmark_big_schematic_creation_with_region_prealloc(c: &mut Criterion) {
    c.bench_function("create big schematic with region prealloc", |b| {
        b.iter(|| {
            let mut schematic = UniversalSchematic::new("Big Schematic".to_string());
            let region = Region::new("Main".to_string(), (0, 0, 0), (100, 100, 100));
            schematic.add_region(region);
            for x in 0..100 {
                for y in 0..100 {
                    for z in 0..100 {
                        schematic.set_block(x, y, z, BlockState::new("minecraft:stone".to_string()));
                    }
                }
            }
            schematic
        })
    });
}



criterion_group!(benches, benchmark_schematic_creation, benchmark_block_setting, benchmark_big_schematic_creation, benchmark_big_schematic_creation_with_region_prealloc);
criterion_main!(benches);
