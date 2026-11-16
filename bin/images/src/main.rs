#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod entities;
mod features;
mod pages;
mod shared;
mod widgets;

fn main() -> std::process::ExitCode {
    app::run()
}
