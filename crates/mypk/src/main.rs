use std::process;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    process::exit(mypk::run(&args));
}
