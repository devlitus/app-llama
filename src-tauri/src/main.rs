#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(error) = app_llama_lib::run() {
        eprintln!("error al ejecutar app-llama: {error}");
        std::process::exit(1);
    }
}
