use clap::Parser;
use serde::{Deserialize, Serialize};

pub fn parse_cli<'a, T>() -> T
where
    T: Parser + Deserialize<'a> + Serialize,
{
    let args = std::env::args();
    let args = args.into_iter().collect::<Vec<String>>();
    let config_pos_maybe = args.iter().position(|x| x == "config");

    if let Some(config_pos) = config_pos_maybe {
        let fp = args
            .get(config_pos + 1)
            .expect("No config path provided for config operation");

        let config = config::Config::builder()
            .add_source(config::File::new(&fp, config::FileFormat::Yaml))
            .build()
            .expect("Failed to build config");
        return config
            .try_deserialize::<T>()
            .expect("Failed to deserialize config");
    } else {
        let args = T::parse();
        return args;
    }
}
