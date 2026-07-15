fn main() {
    if let Err(error) = serverus_lib::generate_typescript_bindings() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
