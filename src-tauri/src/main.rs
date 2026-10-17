#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if multron_lib::nativehost::is_invocation(&args) {
        multron_lib::nativehost::serve();
        return;
    }
    multron_lib::run()
}
