//! Recursion depth limit for the VM and JIT.

use super::WORLD;

pub fn max_recursion_depth() -> usize {
    WORLD.with(|w| w.borrow().max_recursion_depth)
}
pub fn set_max_recursion_depth(depth: usize) {
    WORLD.with(|w| w.borrow_mut().max_recursion_depth = depth);
}
