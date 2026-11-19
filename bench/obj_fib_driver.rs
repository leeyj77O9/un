// Minimal driver for the `un obj` demo: links against `un_main`, the entry
// point exported by the object file produced from `fib.un`.
extern "C" {
    fn un_main() -> i64;
}

fn main() {
    println!("fib = {}", unsafe { un_main() });
}
