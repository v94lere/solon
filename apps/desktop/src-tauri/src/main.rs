// Pas de console sur Windows en release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    solon_lib::run()
}
