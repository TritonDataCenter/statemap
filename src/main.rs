/*
 * Copyright 2018 Joyent, Inc.
 */ 

/*
 * We don't want to get away with not using values that we must use.
 */
#![deny(unused_must_use)]

#[macro_use]
extern crate structopt;
use structopt::StructOpt;

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

fn parse_offset(optval: &str) -> Result<u64, String> {
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

    parse_offset_val(&optval).ok_or_else(|| {
        format!("value is not a valid expression of time: \"{}\"", optval)
    })
}

#[derive(StructOpt)]
#[structopt(name = "statemap")]
struct Opts {
    /// time offset at which to begin statemap
    #[structopt(short = "b", parse(try_from_str = "parse_offset"))]
    begin: Option<u64>,
    /// time offset at which to end statemap
    #[structopt(short = "e", parse(try_from_str = "parse_offset"))]
    end: Option<u64>,
    /// time duration of statemap
    #[structopt(short = "d", parse(try_from_str = "parse_offset"))]
    duration: Option<u64>,
    /// coalesce target
    #[structopt(short = "c")]
    coalesce: Option<u64>,
    /// state to sort by (defaults to entity name)
    #[structopt(short = "s")]
    sortby: Option<String>,
    file: String,
}

fn main() {
    let opts = Opts::from_args();
    let (begin, end) = match (opts.duration, opts.begin, opts.end) {
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

    let mut config = Config { begin: begin, end: end, .. Default::default() };
    if let Some(val) = opts.coalesce {
        config.maxrect = val;
    }

    let mut statemap = Statemap::new(&config);

    match statemap.ingest(&opts.file) {
        Err(f) => { fatal!("could not ingest {}: {}", &opts.file, f); }
        Ok(k) => { k }
    }

    let mut svgconf: StatemapSVGConfig = Default::default();

    svgconf.sortby = opts.sortby;

    match statemap.output_svg(&svgconf) {
        Err(f) => { fatal!("{}", f); }
        Ok(k) => { k }
    }
}
