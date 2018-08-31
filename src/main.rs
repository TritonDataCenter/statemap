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

    parse_offset_val(&optval).unwrap_or_else(|| {
        fatal!(concat!("value for {} is not a valid ",
            "expression of time: \"{}\""), opt, optval)
    })
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

    let matches = parser.parse(&args[1..]).unwrap_or_else(|f| {
        fatal!("{}", f)
    });

    if matches.opt_present("h") {
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

    let mut config = Config { begin: begin, end: end, .. Default::default() };

    if let Some(str) = matches.opt_str("coalesce") {
        match str.parse::<u64>() {
            Err(_err) => fatal!("coalesce factor must be an integer"),
            Ok(val) => config.maxrect = val
        }
    }

    let mut statemap = Statemap::new(&config);

    if let Err(f) = statemap.ingest(&matches.free[0]) {
        fatal!("could not ingest {}: {}", &matches.free[0], f);
    }

    let mut svgconf: StatemapSVGConfig = Default::default();

    svgconf.sortby = matches.opt_str("sortby");

    if let Err(f) = statemap.output_svg(&svgconf) {
        fatal!("{}", f);
    }
}
