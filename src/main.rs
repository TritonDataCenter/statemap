/*
 * Copyright 2018 Joyent, Inc.
 */ 

/*
 * We don't want to get away with not using values that we must use.
 */
#![deny(unused_must_use)]

extern crate getopts;
use getopts::Options;
use getopts::HasArg;
use std::env;

#[macro_use]
extern crate serde_derive;

mod statemap;

use statemap::*;

macro_rules! fatal {
    ($fmt:expr) => ({
        eprint!(concat!("statemap: ", $fmt, "\n"));
        ::std::process::exit(1);
    });
    ($fmt:expr, $($arg:tt)*) => ({
        eprint!(concat!("statemap: ", $fmt, "\n"), $($arg)*);
        ::std::process::exit(1);
    });
}

fn usage(opts: Options) {
    println!("{}", opts.usage("Usage: statemap [options] FILE"));
    ::std::process::exit(0);
}

fn parse_offset(optval: &str, opt: &str) -> u64 {
    fn parse_offset_val(val: &str) -> Option<u64> {
        let mut mult: u64 = 1;
        let mut num = val;

        let suffixes: &[(&'static str, u64)] = &[
            ("ns", 1), ("us", 1_000), ("ms", 1_000_000),
            ("s", 1_000_000_000), ("sec", 1_000_000_000)
        ];

        for suffix in suffixes {
            if val.ends_with(suffix.0) {
                mult = suffix.1;
                num = &val[..val.len() - suffix.0.len()];
                break;
            }
        }

        /*
         * First attempt to parse our number as an integer, falling back
         * on parsing it as floating point if that fails (and being sure
         * to not allow some joker to specify "NaNms").
         */
        match num.parse::<u64>() {
            Err(_err) => {
                match num.parse::<f64>() {
                    Err(_err) => None,
                    Ok(val) => {
                        if val.is_nan() {
                            None
                        } else {
                            Some((val * mult as f64) as u64)
                        }
                    }
                }
            },
            Ok(val) => Some(val * mult)
        }
    }

    match parse_offset_val(&optval) {
        Some(val) => val,
        None => fatal!(concat!("value for {} is not a valid ",
            "expression of time: \"{}\""), opt, optval)
    }
}

fn main() {
    struct Opt {
        name: (&'static str, &'static str),
        help: &'static str,
        hint: &'static str,
        hasarg: HasArg,
    }

    let opts: &[Opt] = &[
        Opt {
            name: ("b", "begin"),
            help: "time offset at which to begin statemap",
            hint: "TIME",
            hasarg: HasArg::Yes
        },
        Opt {
            name: ("e", "end"),
            help: "time offset at which to end statemap",
            hint: "TIME",
            hasarg: HasArg::Yes
        },
        Opt {
            name: ("d", "duration"),
            help: "time duration of statemap",
            hint: "TIME",
            hasarg: HasArg::Yes
        },
        Opt {
            name: ("c", "coalesce"),
            help: "coalesce target",
            hint: "TARGET",
            hasarg: HasArg::Yes
        },
        Opt {
            name: ("h", "help"),
            help: "print this usage message",
            hint: "",
            hasarg: HasArg::No
        },
        Opt {
            name: ("s", "sortby"),
            help: "state to sort by (defaults to entity name)",
            hint: "STATE",
            hasarg: HasArg::Yes
        },
    ];

    let args: Vec<String> = env::args().collect();
    let mut parser = Options::new();

    /*
     * Load the parser with our options.
     */
    for opt in opts {
        parser.opt(opt.name.0, opt.name.1,
            opt.help, opt.hint, opt.hasarg, getopts::Occur::Optional);
    }

    let matches = match parser.parse(&args[1..]) {
        Ok(m) => { m }
        Err(f) => { fatal!("{}", f) }
    };

    if matches.opt_present("h") {
        usage(parser);
    }

    let bounds = (
        matches.opt_str("duration").map(|v| parse_offset(&v, "duration")),
        matches.opt_str("begin").map(|v| parse_offset(&v, "begin")),
        matches.opt_str("end").map(|v| parse_offset(&v, "end")),
    );

    let (begin, end) = match bounds {
        (Some(_), Some(_), Some(_)) => {
            fatal!("cannot specify all of begin, end, and duration")
        },
        (Some(duration), None, None) => {
            (0, duration)
        },
        (Some(duration), Some(begin), None) => {
            (begin, begin + duration)
        },
        (Some(duration), None, Some(end)) => {
            if duration > end {
                fatal!("duration cannot exceed end offset");
            }
            (end - duration, end)
        },
        (None, b, e) => {
            let (begin, end) = (b.unwrap_or(0), e.unwrap_or(0));
            if e < b {
                fatal!("begin offset must be less than end offset");
            }
            (begin, end)
        }
    };

    if matches.free.is_empty() {
        fatal!("must specify a data file");
    }

    let mut config = Config { begin: begin, end: end, .. Default::default() };

    match matches.opt_str("coalesce") {
        Some(str) => match str.parse::<u64>() {
            Err(_err) => fatal!("coalesce factor must be an integer"),
            Ok(val) => config.maxrect = val
        }
        _ => {}
    }

    let mut statemap = Statemap::new(&config);

    match statemap.ingest(&matches.free[0]) {
        Err(f) => { fatal!("could not ingest {}: {}", &matches.free[0], f); }
        Ok(k) => { k }
    }

    let mut svgconf: StatemapSVGConfig = Default::default();

    svgconf.sortby = matches.opt_str("sortby");

    match statemap.output_svg(&svgconf) {
        Err(f) => { fatal!("{}", f); }
        Ok(k) => { k }
    }
}
