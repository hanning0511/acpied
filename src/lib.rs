pub mod term;
pub mod web;

use clap::{value_parser, Arg, Command};

pub fn run() -> anyhow::Result<()> {
    let args = Command::new("ACPI Editor")
        .author("Han, Ning <ning.han@intel.com>")
        .version("0.1")
        .arg(
            Arg::new("mode")
                .value_parser(value_parser!(String))
                .short('m')
                .long("mode")
                .default_value("term")
                .value_names(vec!["term", "web"]),
        )
        .arg(
            Arg::new("port")
                .value_parser(value_parser!(usize))
                .short('p')
                .long("port")
                .default_value("8000"),
        )
        .get_matches();

    let mode = args.get_one::<String>("mode").unwrap();

    match mode.as_str() {
        "term" => term::run(),
        "web" => web::run(),
        _ => {
            // let port = args.get_one::<usize>("port").unwrap();
            println!("unsupported mode");
            Ok(())
        }
    }
}
