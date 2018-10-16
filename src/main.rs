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

#[macro_use]
extern crate serde_json;

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

fn parse_offset(matches: &getopts::Matches, opt: &str) -> u64 {
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

    /*
     * We can safely unwrap here because we should only be here if the option
     * has been set.
     */
    let optval = matches.opt_str(opt).unwrap();

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
        alias: Option<&'static str>,
    }

    let opts: &[Opt] = &[
        Opt {
            name: ("b", "begin"),
            help: "time offset at which to begin statemap",
            hint: "TIME",
            hasarg: HasArg::Yes,
            alias: None,
        },
        Opt {
            name: ("e", "end"),
            help: "time offset at which to end statemap",
            hint: "TIME",
            hasarg: HasArg::Yes,
            alias: None,
        },
        Opt {
            name: ("d", "duration"),
            help: "time duration of statemap",
            hint: "TIME",
            hasarg: HasArg::Yes,
            alias: None,
        },
        Opt {
            name: ("c", "coalesce"),
            help: "coalesce target",
            hint: "TARGET",
            hasarg: HasArg::Yes,
            alias: None,
        },
        Opt {
            name: ("?", "help"),
            help: "print this usage message",
            hint: "",
            hasarg: HasArg::No,
            alias: None,
        },
        Opt {
            name: ("s", "sortby"),
            help: "state to sort by (defaults to entity name)",
            hint: "STATE",
            hasarg: HasArg::Yes,
            alias: None,
        },
        Opt {
            name: ("i", "ignore-tags"),
            help: "ignore tags in input",
            hint: "",
            hasarg: HasArg::No,
            alias: Some("ignoreTags"),
        },
        Opt {
            name: ("h", "state-height"),
            help: "height of each state, in pixels",
            hint: "PIXELS",
            hasarg: HasArg::Yes,
            alias: Some("stateHeight"),
        },
        Opt {
            name: ("n", "dry-run"),
            help: "ingest data, but do not generate output",
            hint: "",
            hasarg: HasArg::No,
            alias: None,
        },
    ];

    let mut args: Vec<String> = env::args().collect();

    /*
     * Iterate over our arguments and options, replacing any alias we find.
     * This allows us to (silently -- and inelegantly) remain backward
     * compatible with camel-cased options while moving to snake-cased ones.
     */
    for i in 0..args.len() {
        for opt in opts {
            if let Some(alias) = opt.alias {
                if args[i].find(alias) != None {
                    args[i] = args[i].replace(alias, opt.name.1);
                }
            }
        }
    }

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

    if matches.opt_present("help") {
        usage(parser);
    }

    let mut begin: u64 = 0;
    let mut end: u64 = 0;

    let has_duration = matches.opt_present("duration");
    let has_begin = matches.opt_present("begin");
    let has_end = matches.opt_present("end");

    if has_duration {
        let duration = parse_offset(&matches, "duration");

        if has_begin {
            if has_end {
                fatal!("cannot specify all of begin, end, and duration");
            } else {
                begin = parse_offset(&matches, "begin");
                end = begin + duration;
            }
        } else {
            if has_end {
                end = parse_offset(&matches, "end");

                if duration > end {
                    fatal!("duration cannot exceed end offset");
                }

                begin = end - duration;
            } else {
                end = duration;
            }
        }
    } else {
        if has_end {
            end = parse_offset(&matches, "end")
        }

        if has_begin {
            begin = parse_offset(&matches, "begin");
            if end < begin {
                fatal!("begin offset must be less than end offset");
            }
        }
    }

    if matches.free.is_empty() {
        fatal!("must specify a data file");
    }

    let mut config = Config {
        begin: begin,
        end: end,
        notags: matches.opt_present("ignore-tags"),
        .. Default::default()
    };

    match matches.opt_str("coalesce") {
        Some(str) => match str.parse::<u64>() {
            Err(_err) => fatal!("coalesce factor must be an integer"),
            Ok(val) => config.maxrect = val
        }
        _ => {}
    }

    let mut statemap = Statemap::new(&config);
    let mut svgconf: StatemapSVGConfig = Default::default();

    svgconf.sortby = matches.opt_str("sortby");

    if let Some(str) = matches.opt_str("state-height") {
        match str.parse::<u32>() {
            Err(_err) => fatal!("state height must be an integer"),
            Ok(val) => svgconf.stripHeight = val
        }
    }

    match statemap.ingest(&matches.free[0]) {
        Err(f) => { fatal!("could not ingest {}: {}", &matches.free[0], f); }
        Ok(k) => { k }
    }

    if matches.opt_present("dry-run") {
        return;
    }

    match statemap.output_svg(&svgconf) {
        Err(f) => { fatal!("{}", f); }
        Ok(k) => { k }
    }
}
