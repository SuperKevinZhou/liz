//! Binary entrypoint for the liz CLI reference client.

use liz_cli::tui::{run_tui, CliArgs};

fn main() {
    let args = CliArgs::parse(std::env::args().skip(1));
    if args.show_help {
        println!("{}", CliArgs::help_text());
        return;
    }
    if args.banner_only {
        println!("{}", liz_cli::banner_line());
        return;
    }

    if let Err(error) = run_tui(&args.server_url) {
        eprintln!("liz-cli failed: {error}");
        std::process::exit(1);
    }
}
