
fn calculate_bits_per_block(palette_size: usize) -> usize {
    std::cmp::max((palette_size as f64).log2().ceil() as usize, 2)
}