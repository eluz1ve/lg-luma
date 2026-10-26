//! The widget authors' tool (docs/widgets.md §9).
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--widget-transform") {
        luma_widgets::sandbox::child_main();
    }
    eprintln!("usage: luma-widget check|run|test|pack|sign|keygen …");
    std::process::exit(2);
}
