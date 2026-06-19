fn main() {
    let config = slint_build::CompilerConfiguration::new().with_debug_info(true);
    slint_build::compile_with_config("ui/app.slint", config)
        .expect("failed to compile Dais Desk Slint UI");
}
