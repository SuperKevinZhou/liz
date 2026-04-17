//! Binary entrypoint for the liz app server.

fn main() {
    let args = CliArgs::parse(std::env::args().skip(1));
    if args.show_help {
        println!("{}", CliArgs::help_text());
        return;
    }
    if !args.serve {
        println!("{}", liz_app_server::banner_line());
        return;
    }

    let server = liz_app_server::server::AppServer::from_default_layout();
    let handle = liz_app_server::server::spawn_websocket_server(server, args.bind_address.as_str())
        .unwrap_or_else(|error| {
            panic!("failed to start websocket server on {}: {error}", args.bind_address)
        });
    println!("liz-app-server listening on {}", handle.ws_url());

    loop {
        std::thread::park();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliArgs {
    serve: bool,
    show_help: bool,
    bind_address: String,
}

impl Default for CliArgs {
    fn default() -> Self {
        Self { serve: false, show_help: false, bind_address: "127.0.0.1:7777".to_owned() }
    }
}

impl CliArgs {
    fn parse<I>(args: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        let mut parsed = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--serve" => parsed.serve = true,
                "--help" | "-h" => parsed.show_help = true,
                "--bind" => {
                    if let Some(bind_address) = args.next() {
                        parsed.bind_address = bind_address;
                    }
                }
                _ => {}
            }
        }
        parsed
    }

    fn help_text() -> &'static str {
        "liz-app-server\n  --serve            Start the websocket app server\n  --bind <addr>      Bind address for websocket serving (default 127.0.0.1:7777)\n  --help             Show this message"
    }
}
