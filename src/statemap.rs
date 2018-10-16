/*
 * Copyright 2018 Joyent, Inc. and other contributors
 */ 

extern crate memmap;
extern crate serde;
extern crate serde_json;
extern crate natord;
extern crate palette;
extern crate rand;

/*
 * The StatemapInput* types denote the structure of the concatenated JSON
 * in the input file.
 */
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct StatemapInputState {
    color: Option<String>,                  // color for state, if any
    value: usize,                           // value for state
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct StatemapInputDatum {
    #[serde(deserialize_with = "datum_time_from_string")]
    time: u64,                              // time of this datum
    entity: String,                         // name of entity
    state: u32,                             // state entity is in at time
    tag: Option<String>,                    // tag for this state, if any
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct StatemapInputDescription {
    entity: String,                         // name of entity
    description: String,                    // description of entity
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
#[serde(deny_unknown_fields)]
struct StatemapInputMetadata {
    start: Vec<u64>,
    title: String,
    host: Option<String>,
    entityKind: Option<String>,
    states: HashMap<String, StatemapInputState>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct StatemapInputEvent {
    time: String,                           // time of this datum
    entity: String,                         // name of entity
    event: String,                          // type of event
    target: Option<String>,                 // target for event, if any
}

#[derive(Deserialize, Debug)]
struct StatemapInputTag {
    state: u32,                             // state for this tag
    tag: String,                            // tag itself
}

#[derive(Copy,Clone,Debug)]
pub struct Config {
    pub maxrect: u64,
    pub begin: u64,
    pub end: u64,
    pub notags: bool,
}

/*
 * These fields are dropped directly into the SVG.
 */
#[derive(Debug,Serialize)]
#[allow(non_snake_case)]
pub struct StatemapSVGConfig {
    pub stripHeight: u32,
    pub legendWidth: u32,
    pub tagWidth: u32,
    pub stripWidth: u32,
    pub background: String,
    pub sortby: Option<String>,
}

#[derive(Copy,Clone,Debug)]
struct StatemapColor {
    color: Color,                           // underlying color
}

#[derive(Debug)]
struct StatemapRect {
    start: u64,                             // nanosecond offset
    duration: u64,                          // nanosecond duration
    weight: u64,                            // my weight + neighbors
    states: Vec<u64>,                       // time spent in each state
    prev: Option<u64>,                      // previous rectangle
    next: Option<u64>,                      // next rectangle
    tags: Option<HashMap<usize, u64>>,      // tags, if any
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct StatemapRectWeight {
    weight: u64,                            // weight for this rect
    start: u64,                             // start time for this rect
    entity: usize,                          // entity for this rect
}

#[derive(Default,Clone,Debug,Serialize)]
struct StatemapState {
    name: String,                           // name of this state
    value: usize,                           // value for this state
    color: Option<String>,                  // color of this state, if any
}

#[derive(Debug)]
struct StatemapEntity {
    name: String,                           // name of this entity
    id: usize,                              // identifier
    description: Option<String>,            // description, if any
    last: Option<u64>,                      // last start time
    start: Option<u64>,                     // current start time
    state: Option<u32>,                     // current state
    tag: Option<usize>,                     // current tag, if any
    rects: HashMap<u64, RefCell<StatemapRect>>, // rectangles for this entity
}

#[derive(Debug)]
pub struct Statemap {
    config: Config,                         // configuration
    metadata: Option<StatemapInputMetadata>, // in-stream metadata
    nrecs: u64,                             // number of records
    nevents: u64,                           // number of events
    entities: HashMap<String, StatemapEntity>, // hash of entities
    states: Vec<StatemapState>,             // vector of valid states
    byid: Vec<String>,                      // entities by ID
    byweight: BTreeSet<StatemapRectWeight>, // rectangles by weight
    tags: HashMap<(u32, String), (Value, usize)>, // tags, if any
}

#[derive(Debug)]
pub struct StatemapError {
    errmsg: String
}

#[derive(Serialize)]
#[allow(non_snake_case)]
struct StatemapSVGGlobals<'a> {
    begin: u64,
    end: u64,
    entityPrefix: String,
    pixelHeight: u32,
    pixelWidth: u32,
    totalHeight: u32,
    timeWidth: u64,
    lmargin: u32,
    tmargin: u32,
    states: &'a Vec<StatemapState>,
    start: &'a Vec<u64>,
    entityKind: &'a str,
}

use std::fs::File;
use std::str;
use std::error::Error;
use std::fmt;
use std::collections::HashMap;
use std::collections::BTreeSet;
use std::str::FromStr;
use std::cell::RefCell;
use std::cmp;

#[cfg(test)]
use std::collections::HashSet;

use self::memmap::MmapOptions;
use self::palette::{Srgb, Color, Mix};
use self::serde_json::Value;

impl Default for Config {
    fn default() -> Config {
        Config { 
            maxrect: 25000,
            begin: 0,
            end: 0,
            notags: false,
        }
    }
}

impl Default for StatemapSVGConfig {
    fn default() -> StatemapSVGConfig {
        StatemapSVGConfig {
            stripHeight: 10,
            legendWidth: 138,
            stripWidth: 862,
            tagWidth: 250,
            background: "#f0f0f0".to_string(),
            sortby: None,
        }
    }
}

impl StatemapError {
    fn new(msg: &str) -> StatemapError {
        StatemapError { errmsg: msg.to_string() }
    }
}

impl fmt::Display for StatemapError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.errmsg)
    }
}

impl Error for StatemapError {
    fn description(&self) -> &str {
        &self.errmsg
    }
}

impl FromStr for StatemapColor {
    type Err = StatemapError;

    fn from_str(name: &str) -> Result<StatemapColor, StatemapError> {
        let named = palette::named::from_str(name);

        match named {
            Some(color) => {
                let rgb = Srgb::<f32>::from_format(color);

                return Ok(StatemapColor {
                    color: rgb.into_format().into_linear().into()
                });
            }
            None => {}
        }

        if name.len() == 7 && name.chars().next().unwrap() == '#' {
            let r = u8::from_str_radix(&name[1..3], 16);
            let g = u8::from_str_radix(&name[3..5], 16);
            let b = u8::from_str_radix(&name[5..7], 16);

            if r.is_ok() && g.is_ok() && b.is_ok() {
                let rgb = Srgb::new(r.unwrap(), g.unwrap(), b.unwrap());

                return Ok(StatemapColor {
                    color: rgb.into_format().into_linear().into()
                });
            }
        }

        return Err(StatemapError { 
            errmsg: format!("\"{}\" is not a valid color", name)
        });
    }
}

impl fmt::Display for StatemapColor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let rgb = Srgb::from_linear(self.color.into()).into_components();

        write!(f, "rgb({}, {}, {})", (rgb.0 * 256.0) as u8,
            (rgb.1 * 256.0) as u8, (rgb.2 * 256.0) as u8)
    }
}

impl StatemapColor {
    fn random() -> Self {
        let rgb = Srgb::new(rand::random::<u8>(), rand::random::<u8>(),
            rand::random::<u8>());

        StatemapColor {
            color: rgb.into_format().into_linear().into()
        }
    }

    fn _mix(&self, other: &Self, ratio: f32) -> Self {
        StatemapColor {
            color: self.color.mix(&other.color, ratio)
        }
    }

    fn mix_nonlinear(&self, other: &Self, ratio: f32) -> Self {
        let lhs = Srgb::from_linear(self.color.into()).into_components();
        let rhs = Srgb::from_linear(other.color.into()).into_components();

        let recip = 1.0 - ratio;

        let rgb = Srgb::<f32>::new(lhs.0 as f32 * recip + rhs.0 as f32 * ratio,
            lhs.1 as f32 * recip + rhs.1 as f32 * ratio,
            lhs.2 as f32 * recip + rhs.2 as f32 * ratio);

        StatemapColor {
            color: rgb.into_format().into_linear().into()
        }
    }
}

impl StatemapRect {
    fn new(start: u64, duration: u64, state: u32, nstates: u32) -> Self {
        let mut r = StatemapRect {
            start: start,
            duration: duration,
            states: vec![0; nstates as usize],
            prev: None,
            next: None,
            weight: duration,
            tags: None,
        };

        r.states[state as usize] = duration;
        r
    }
}

fn subsume_tags(stags: &mut HashMap<usize, u64>,
    vtags: &mut HashMap<usize, u64>)
{
    for (id, duration) in vtags.drain() {
        if let Some(d) = stags.get_mut(&id) {
            *d += duration;
            continue;
        }

        stags.insert(id, duration);
    }
}

impl StatemapEntity {
    fn new(name: &str, id: usize) -> Self {
        StatemapEntity {
            name: name.to_string(),
            start: None,
            description: None,
            last: None,
            state: None,
            tag: None,
            rects: HashMap::new(),
            id: id,
        }
    }

    fn newrect(&mut self, end: u64, nstates: u32)
        -> (Option<(u64, u64, u64)>, (u64, u64))
    {
        let start = self.start.unwrap();
        let state = self.state.unwrap();
        let lhs: Option<(u64, u64, u64)>;
        let rhs: (u64, u64);
        let mut rect = StatemapRect::new(start, end - start, state, nstates);

        match self.tag {
            Some(id) => {
                let mut hash: HashMap<usize, u64> = HashMap::new();
                hash.insert(id, end - start);
                rect.tags = Some(hash);
            }
            _ => {}
        }

        rect.prev = self.last;

        match self.last {
            Some(last) => {
                let mut lrect = self.rects.get(&last).unwrap().borrow_mut();
                let old = lrect.weight;

                lrect.next = Some(start);
                rect.weight += lrect.duration;
                lrect.weight += rect.duration;

                lhs = Some((lrect.start, old, lrect.weight));
            }
            _ => { lhs = None; }
        }

        rhs = (rect.start, rect.weight);
        self.rects.insert(start, RefCell::new(rect));
        (lhs, rhs)
    }

    fn addto(&mut self, rect: u64, delta: u64) -> u64 {
        let mut r = self.rects.get(&rect).unwrap().borrow_mut();
        let old = r.weight;

        r.weight += delta;
        old
    }

    fn subsume(&mut self, victim: u64)
        -> ((Option<u64>, u64), (u64, u64), (u64, u64), ((Option<u64>, u64)))
    {
        let mut last = self.last;
        let subsumed: u64;
        let rval;

        /*
         * We return three weights that need to be adjusted: that of the
         * rectangle to the left (post-subsume), that of the rectangle to
         * the right (post-subsume) and that of the center rectangle.  Each
         * of these adjustments is described as a start plus a weight to be
         * added -- and all three are returned as a tuple that also includes
         * the subsumed rectangle that needs to be removed.
         */
        let ldelta: (Option<u64>, u64);
        let cdelta: (u64, u64);
        let rdelta: (Option<u64>, u64);

        /*
         * We create a scope here to help out the borrow checker in terms of
         * knowing that our immutable borrow of self.rects is being dropped
         * before our mutable borrow of it, below.
         */
        {
            let left: &RefCell<StatemapRect>;
            let right: &RefCell<StatemapRect>;

            /*
             * We create a scope here to allow the borrow of the victim
             * cell fall out of scope as we may need it to be mutable, below.
             */
            {
                let vcell = self.rects.get(&victim).unwrap();
                let v = vcell.borrow();

                match (v.prev, v.next) {
                    (None, None) => panic!("nothing to subsume"),
                    (Some(prev), None) => {
                        left = self.rects.get(&prev).unwrap();
                        right = vcell;

                        let lref = left.borrow();
                        ldelta = (lref.prev, v.duration);
                        cdelta = (lref.start, 0);
                        rdelta = (None, 0);
                    }
                    (None, Some(next)) => {
                        left = vcell;
                        right = self.rects.get(&next).unwrap();

                        /*
                         * We want the weight of the remaining (center)
                         * rectangle to be the weight of our right rectangle;
                         * to express this as a delta, we express it as the
                         * difference between the two.
                         */
                        let rref = right.borrow();
                        ldelta = (None, 0);
                        cdelta = (v.start, rref.weight - v.weight);
                        rdelta = (rref.next, v.duration);
                    }
                    (Some(prev), Some(next)) => {
                        /*
                         * We want whichever of our neighboring rectangles is
                         * shorter to subsume us.
                         */
                        let l = self.rects.get(&prev).unwrap();
                        let r = self.rects.get(&next).unwrap();

                        let lref = l.borrow();
                        let rref = r.borrow();

                        if lref.duration < rref.duration {
                            left = l;
                            right = vcell;

                            ldelta = (lref.prev, v.duration);
                            cdelta = (lref.start, v.weight -
                                (lref.duration + v.duration));
                            rdelta = (Some(rref.start), lref.duration);
                        } else {
                            left = vcell;
                            right = r;

                            ldelta = (Some(lref.start), rref.duration);
                            cdelta = (v.start, rref.weight -
                                (rref.duration + v.duration));
                            rdelta = (rref.next, v.duration);
                        }
                    }
                }
            }

            let mut s = left.borrow_mut();
            let mut v = right.borrow_mut();

            s.next = v.next;

            /*
             * Set our subsumed next rectangle's previous to point back to us
             * rather than the subsumed rectangle.
             */
            match s.next {
                Some(next) => {
                    self.rects.get(&next).unwrap()
                        .borrow_mut().prev = Some(s.start);
                }
                None => {
                    last = Some(s.start);
                }
            }

            /*
             * Add our duration, and then sum the value in each of the states.
             */
            s.duration += v.duration;

            for i in 0..v.states.len() {
                s.states[i] += v.states[i];
            }

            /*
             * If our victim has tags, we need to fold them in.
             */
            if v.tags.is_some() && s.tags.is_none() {
                s.tags = Some(HashMap::new());
            }

            match s.tags {
                Some(ref mut stags) => {
                    match v.tags {
                        Some(ref mut vtags) => { subsume_tags(stags, vtags); },
                        None => {}
                    }
                },
                None => {}
            }

            subsumed = v.start;
            rval = (v.start, v.weight);
        }

        /*
         * Okay, we're done subsuming! We can remove the subsumed rectangle.
         */
        self.rects.remove(&subsumed);
        self.last = last;

        (ldelta, cdelta, rval, rdelta)
    }

    #[must_use]
    fn apply(&mut self, deltas: ((Option<u64>, u64),
        (u64, u64), (u64, u64), ((Option<u64>, u64)))) ->
        Vec<(u64, u64, Option<u64>)>
    {
        let mut updates: Vec<(u64, u64, Option<u64>)> = vec![];

        /*
         * Handle the left delta.
         */
        match (deltas.0).0 {
            Some(rect) => {
                let delta = (deltas.0).1;
                updates.push((rect, self.addto(rect, delta), Some(delta)));
            }
            None => {}
        }

        /*
         * Handle the center delta.
         */
        let rect = (deltas.1).0;
        let delta = (deltas.1).1;
        updates.push((rect, self.addto(rect, delta), Some(delta)));

        /*
         * Handle the subsumed rectangle by pushing a delta update of None.
         */
        updates.push(((deltas.2).0, (deltas.2).1, None));

        /*
         * And finally, the right delta.
         */
        match (deltas.3).0 {
            Some(rect) => {
                let delta = (deltas.3).1;
                updates.push((rect, self.addto(rect, delta), Some(delta)));
            }
            None => {}
        }

        updates
    }

    fn output_svg(&self, config: &StatemapSVGConfig,
        globals: &StatemapSVGGlobals,
        colors: &Vec<StatemapColor>, y: u32) -> Vec<String>
    {
        let rect_width = |rect: &StatemapRect| -> f64 {
            /*
             * We add a fuzz factor to our width to assure it will always be
             * nearly (but not quite!) half a pixel wider than it should be.
             * This assures that none of the background (which is deliberately a
             * bright color) comes through at the border of rectangles, without
             * losing any accuracy (the next rectangle will tile over ours at
             * an unadjusted offset).
             */
            ((rect.duration as f64 / globals.timeWidth as f64) *
                globals.pixelWidth as f64) + 0.4 as f64
        };

        let output_tags = |rect: &StatemapRect, datum: &mut String| {
            /*
             * If we have tags, we emit them in ID order.
             */
            if let Some(ref tags) = rect.tags {
                let mut g: Vec<(usize, u64)>;

                datum.push_str(", g: {");

                g = tags.iter()
                    .map(|(&id, &duration)| { (id, duration) })
                    .collect();

                g.sort_unstable();

                for j in 0..g.len() {
                    let ratio = g[j].1 as f64 / rect.duration as f64;
                    datum.push_str(&format!("'{}': {:.3}{}", g[j].0, ratio,
                        if j < g.len() - 1 { "," } else { "" }));
                }

                datum.push_str("}");
            }
        };

        let mut x: f64;
        let mut map: Vec<u64>;
        let mut data: Vec<String> = vec![];

        map = self.rects.values().map(|r| r.borrow().start).collect();
        map.sort();

        if map.len() > 0 {
            x = ((map[0] - globals.begin) as f64 /
                globals.timeWidth as f64) * globals.pixelWidth as f64;
        } else {
            x = globals.pixelWidth as f64;
        }
            
        println!(r##"<rect x="0" y="{}" width="{}"
            height="{}" style="fill:{}" />"##, y, x, config.stripHeight,
            config.background);

        println!(r##"<g id="{}{}"><title>{} {}</title>"##,
            globals.entityPrefix, self.name, globals.entityKind, self.name);

        for i in 0..map.len() {
            let rect = self.rects.get(&map[i]).unwrap().borrow();
            let mut state = None;
            let mut blended = false;

            x = ((map[i] - globals.begin) as f64 /
                globals.timeWidth as f64) * globals.pixelWidth as f64;

            for j in 0..rect.states.len() {
                if rect.states[j] != 0 {
                    match state {
                        None => { state = Some(j) },
                        Some(_s) => {
                            blended = true;
                            break;
                        }
                    }
                }
            }

            if !blended {
                assert!(state.is_some());

                let mut datum = format!("{{ t: {}, s: {}", rect.start,
                    state.unwrap());

                output_tags(&rect, &mut datum);
                datum.push_str("}");
                data.push(datum);

                println!(concat!(r##"<rect x="{}" y="{}" width="{}" "##,
                    r##"height="{}" onclick="mapclick(evt, {})" "##,
                    r##"style="fill:{}" />"##),
                    x, y, rect_width(&rect), config.stripHeight,
                    data.len() - 1, colors[state.unwrap()]);

                continue;
            }

            let max = rect.states.iter().enumerate()
                .max_by(|&(_, lhs), &(_, rhs)| lhs.cmp(rhs)).unwrap().0;

            let mut color = colors[max];
            let mut datum = format!("{{ t: {}, s: {{ ", rect.start);
            let mut comma = "";
            
            for j in 0..rect.states.len() {
                if rect.states[j] == 0 {
                    continue;
                }

                let ratio = rect.states[j] as f64 / rect.duration as f64;

                datum.push_str(&format!("{}'{}': {:.3}", comma, j, ratio));
                comma = ", ";

                if j != max {
                    color = color.mix_nonlinear(&colors[j], ratio as f32);
                }
            }

            datum.push_str("}");

            output_tags(&rect, &mut datum);
            datum.push_str("}");
            data.push(datum);

            println!(concat!(r##"<rect x="{}" y="{}" width="{}" "##,
                r##"height="{}" onclick="mapclick(evt, {})" "##,
                r##"style="fill:{}" />"##), x, y, rect_width(&rect),
                config.stripHeight, data.len() - 1, color);
        }
        
        println!("</g>");
        data
    }

    #[cfg(test)]
    fn print(&self, header: &str) {
        let mut v: Vec<u64>;
        let l: usize;
        
        v = self.rects.values().map(|r| r.borrow().start).collect();
        v.sort();
        l = v.len();

        for i in 0..l {
            let me = self.rects.get(&v[i]).unwrap().borrow();
            println!("{}: entity={}: [{}] {:?}: {:?}",
                header, self.id, i, v[i], me);
        }

        println!("{}: entity={}: last is {:?}", header, self.id, self.last);
    }

    #[cfg(test)]
    fn verify(&self) {
        let mut v: Vec<u64>;
        let l: usize;
        
        v = self.rects.values().map(|r| r.borrow().start).collect();
        v.sort();
        l = v.len();

        for i in 0..l {
            let me = self.rects.get(&v[i]).unwrap().borrow();
            let mut weight = me.duration;

            if i < l - 1 {
                let next = self.rects.get(&v[i + 1]).unwrap().borrow();
                assert_eq!(me.next, Some(next.start));
                assert!(me.start < next.start);
                weight += next.duration;
            } else {
                assert_eq!(me.next, None);
                assert_eq!(self.last, Some(me.start));
            }

            if i > 0 {
                let prev = self.rects.get(&v[i - 1]).unwrap().borrow();
                assert_eq!(me.prev, Some(prev.start));
                assert!(me.start > prev.start);
                weight += prev.duration;
            } else {
                assert_eq!(me.prev, None);
            }

            assert_eq!(me.weight, weight);

            if let Some(ref tags) = me.tags {
                let duration = tags.iter().fold(0,
                    |i, (_id, duration)| { i + duration });

                /*
                 * This is technically a more vigorous assertion than we can
                 * make:  we actually allow for partial tagging in that not
                 * all states must by tagged all of the time.  For the moment,
                 * though, we assert that if any states have been tagged, all
                 * have been.
                 */
                assert_eq!(duration, me.duration);
            }
        }
    }

    #[cfg(test)]
    fn subsume_apply_and_verify(&mut self, victim: u64) ->
        Vec<(u64, u64, Option<u64>)>
    {
        println!("=== Subsuming {}", victim);
        self.print(&format!("Before subsuming {}", victim));
        self.verify();

        let tup = self.subsume(victim);
        println!("Weight delta from subsuming {}: {:?}", victim, tup);

        self.print(&format!("After subsuming {}, before applying", victim));
        let updates = self.apply(tup);

        self.print(&format!("After subsuming {}, after applying", victim));
        self.verify();
        updates
    }
}

enum Ingest {
    Success,
    EndOfFile,
}

impl Statemap {
    pub fn new(config: &Config) -> Self {
        Statemap {
            config: *config,
            nrecs: 0,
            nevents: 0,
            entities: HashMap::new(),
            states: Vec::new(),
            byid: Vec::new(),
            byweight: BTreeSet::new(),
            metadata: None,
            tags: HashMap::new(),
        }
    }

    fn err<T>(&self, msg: &str) -> Result<T, Box<Error>>  {
        Err(Box::new(StatemapError::new(msg)))
    }

    fn entity_lookup(&mut self, name: &str) -> &mut StatemapEntity {
        /*
         * The lack of non-lexical lifetimes causes this code to be a bit
         * gnarlier than it should really have to be.
         */
        if self.entities.contains_key(name) {
            return match self.entities.get_mut(name) {
                Some(entity) => { entity },
                None => unreachable!()
            };
        }

        let entity = StatemapEntity::new(name, self.byid.len());
        self.byid.push(name.to_string());

        self.entities.insert(name.to_string(), entity);
        self.entities.get_mut(name).unwrap()
    }

    fn tag_lookup(&mut self, state: u32, tagr: &Option<String>)
        -> Option<usize>
    {
        if self.config.notags {
            return None;
        }

        match *tagr {
            Some(ref tag) => {
                let id;

                match self.tags.get(&(state, tag.to_string())) {
                    Some(( _value, idr)) => { return Some(*idr); },
                    None => { id = self.tags.len(); }
                }

                let value = json!({ "state": state, "tag": tag.to_string() });

                self.tags.insert((state, tag.to_string()), (value, id));
                Some(id)
            },
            None => None
        }
    }

    /*
     * Takes a vector of updates to apply to our byweight tree as well as a
     * template rectangle weight and applies the updates.
     */
    fn apply(&mut self, updates: Vec<(u64, u64, Option<u64>)>,
        rweight: &mut StatemapRectWeight)
    {
        for i in 0..updates.len() {
            rweight.start = updates[i].0;
            rweight.weight = updates[i].1;

            self.byweight.remove(rweight);

            match updates[i].2 {
                Some(delta) => {
                    rweight.weight += delta;
                    self.byweight.insert(*rweight);
                }
                None => {}
            }
        }
    }

    /*
     * Subsumes the rectangle of least weight, applies the deltas to the
     * entity corresponding to that rectangle, and then applies the
     * resulting rectangle weight updates.
     */
    fn trim(&mut self) {
        let mut remove: StatemapRectWeight;
        let updates;

        remove = *self.byweight.iter().next().unwrap();
        self.byweight.remove(&remove);
        
        /*
         * We need a scope here to help the compiler out with respect to
         * our use of entity.
         */
        {
            let name = &self.byid[remove.entity];
            let entity = self.entities.get_mut(name).unwrap();

            if entity.rects.len() == 1 {
                /*
                 * If this entity only has one rectangle, than there is
                 * nothing to subsume; we simply return.  (This weight has
                 * already been removed, so we won't find it again until
                 * another rectangle is added for this entity.)
                 */
                return;
            }

            let deltas = entity.subsume(remove.start);
            updates = entity.apply(deltas);
        }

        self.apply(updates, &mut remove);
    }

    #[must_use]
    fn sort(&self, sortby: Option<usize>) -> Vec<usize>
    {
        let mut v: Vec<(u64, &String, usize)>;

        let values = self.entities.values();

        match sortby {
            None => { v = values.map(|e| (0, &e.name, e.id)).collect(); },
            Some(state) => {
                v = values.map(|e| {
                    let ttl = e.rects.values().fold(0, |i, r| {
                        i + r.borrow().states[state]
                    });

                    (ttl, &e.name, e.id)
                }).collect();
            }
        }

        v.sort_by(|&a, &b| {
            let result = b.0.cmp(&a.0);

            if result == cmp::Ordering::Equal {
                natord::compare(a.1, b.1)
            } else {
                result
            }
        });

        v.iter().map(|e| e.2).collect()
    }

    #[cfg(test)]
    fn verify(&self) {
        /*
         * First, verify each of the entities.
         */
        for entity in self.entities.values() {
            entity.verify();
        }

        /*
         * Verify that each rectangle in each entity can be found in our
         * byweight set -- and that the weights match.
         */
        for entity in self.entities.values() {
            for cell in entity.rects.values() {
                let rect = cell.borrow();

                let rweight = StatemapRectWeight {
                    entity: entity.id,
                    weight: rect.weight,
                    start: rect.start
                };

                assert!(self.byweight.contains(&rweight) ||
                    entity.rects.len() == 1 ||
                    Some(rect.start) == entity.last ||
                    rect.next == entity.last);
            }
        }

        let mut present = HashSet::new();

        /*
         * Verify that each entity is valid that each entity/start tuple is
         * present exactly once.
         */
        for rweight in self.byweight.iter() {
            let name = &self.byid[rweight.entity];
            let tup = (rweight.entity, rweight.start);

            assert!(self.entities.get(name).is_some());

            let entity = self.entities.get(name).unwrap();

            assert!(entity.rects.get(&rweight.start).is_some());
            assert!(!present.contains(&tup));
            present.insert(tup);
        }
    }

    #[cfg(test)]
    fn subsume_apply_and_verify(&mut self, what: &str, victim: u64) {
        let id: usize;
        let mut weight: Option<u64> = None;
        let updates;

    {
        let entity = self.entity_lookup(what);
            updates = entity.subsume_apply_and_verify(victim);
            id = entity.id;
        }

        /*
         * Before we verify, remove the victim -- if it wasn't actually
         * to be removed, we'll add it back when we apply the updates.
         */
        for rweight in self.byweight.iter() {
            if rweight.entity == id && rweight.start == victim {
                assert!(weight.is_none());
                weight = Some(rweight.weight);
            }
        }

        assert!(weight.is_some());

        let mut rweight = StatemapRectWeight {
            entity: id, weight: 0, start: 0
        };

        self.apply(updates, &mut rweight);
        self.print(&format!("After subsuming {} from {}", victim, what)); 
        self.verify();
    }

    #[cfg(test)]
    fn print(&self, header: &str) {
        println!("{}: by weight: {:?}", header, self.byweight);

        for entity in self.entities.values() {
            entity.print(header);
        }

        println!("");
    }

    #[cfg(test)]
    fn get_rects(&self, entity: &str) -> Vec<(u64, u64, Vec<u64>)> {
        let mut rval: Vec<(u64, u64, Vec<u64>)>;

        let e = self.entities.get(entity);

        match e {
            Some(entity) => {
                rval = entity.rects.values().map(|r| {
                    let rect = r.borrow();

                    (rect.start, rect.duration, rect.states.clone())
                }).collect();

                rval.sort();
            },
            None => { rval = vec![]; }
        }

        rval
    }

    /*
     * Ingest and advance `payload` past the metadata JSON object.
     */
    fn ingest_metadata(&mut self, payload: &mut &str)
        -> Result<(), Box<Error>>
    {
        let metadata: StatemapInputMetadata = match try_parse(payload)? {
            None => return self.err("missing metadata payload"),
            Some(metadata) => metadata,
        };

        let nstates = metadata.states.len();
        let mut states: Vec<Option<StatemapState>> = vec![None; nstates];

        if metadata.start.len() != 2 {
            return self.err(concat!("\"start\" property must be a ",
                "two element array"));
        }

        for (key, value) in &metadata.states {
            let ndx = value.value;

            if ndx >= nstates {
                let errmsg = format!(concat!("state \"{}\" has value ({}) ",
                    "that exceeds maximum allowed value ({})"),
                    key, ndx, nstates - 1);
                return self.err(&errmsg);
            }

            if ndx < states.len() && states[ndx].is_some() {
                let errmsg = format!(concat!("state \"{}\" has value ",
                    "({}) that conflicts with state \"{}\""), key,
                    ndx, states[ndx].as_ref().unwrap().name);

                return self.err(&errmsg);
            }

            states[ndx] = Some(StatemapState {
                name: key.to_string(),
                value: ndx,
                color: match value.color {
                    Some(ref str) => { Some(str.to_string()) },
                    None => { None }
                }
            });
        }

        assert_eq!(self.states.len(), 0);

        /*
         * We have verified our states; now pull them into our array.
         */
        for _i in 0..nstates {
            self.states.push(states.remove(0).unwrap());
        }

        self.metadata = Some(metadata);

        Ok(())
    }

    fn ingest_end(&mut self) {
        let mut end = self.config.end;

        if end == 0 {
            /*
             * If we weren't given an ending time, take a lap through all
             * of our entities to find the one with the latest time.
             */
            end = self.entities.values().fold(0, |latest, e| {
                match e.start {
                    Some(start) => cmp::max(latest, start),
                    None => latest
                }
            });
        }

        let nstates = self.states.len() as u32;

        for entity in self.entities.values_mut() {
            match entity.start {
                Some(start) if start < end => {
                    /*
                     * We are adding a rectangle, but because we are now done
                     * with ingestion, we are not updating the rectangle weight
                     * tree and we are not going to subsume any rectangles; we
                     * can safely ignore the return value.
                     */
                    entity.newrect(end, nstates);

                    /*
                     * Even though we expect no other ingestion, we set our
                     * last to allow for state to be verified.
                     */
                    entity.last = entity.start;
                },
                _ => {}
            }
        }
    }

    /*
     * Ingest and advance `payload` past one JSON object datum.
     */
    fn ingest_datum(&mut self, payload: &mut &str)
        -> Result<Ingest, Box<Error>>
    {
        match try_parse::<StatemapInputDatum>(payload) {
            Ok(None) => return Ok(Ingest::EndOfFile),
            Ok(Some(datum)) => {
                let time: u64 = datum.time;
                let nstates: u32 = self.states.len() as u32;

                /*
                 * If the time of this datum is after our specified end time,
                 * we have nothing further to do to process it.
                 */
                if self.config.end > 0 && time > self.config.end {
                    return Ok(Ingest::Success);
                }

                if datum.state >= nstates {
                    return self.err("illegal state value");
                }

                let begin = self.config.begin;
                let mut errmsg: Option<String> = None;
                let mut insert: Option<StatemapRectWeight> = None;
                let mut update: Option<(StatemapRectWeight, u64)> = None;
                let tag = self.tag_lookup(datum.state, &datum.tag);

                /*
                 * We are going to do a lookup of our entity, but this will
                 * cause us to lose our reference on self (mutable or
                 * otherwise) -- which we need to fully record any error.  To
                 * implement this absent non-lexical lifetimes, we put the
                 * entity in a lexical scope implemented with "loop" so we
                 * can break out of it on an error condition.
                 */
                loop {
                    let name = &datum.entity;
                    let entity = self.entity_lookup(name);

                    match entity.start {
                        Some(start) => {
                            if time < start {
                                errmsg = Some(format!(concat!("time {} is out",
                                    " of order with respect to prior time {}"),
                                    time, start));
                                break;
                            }

                            if time > begin {
                                /*
                                 * We can now create a new rectangle for this
                                 * entity's past state.
                                 */
                                if start < begin {
                                    entity.start = Some(begin);
                                }

                                let rval = entity.newrect(time, nstates);
                                entity.last = entity.start;

                                match rval.0 {
                                    Some(rect) => {
                                        update = Some((StatemapRectWeight {
                                            weight: rect.1,
                                            start: rect.0,
                                            entity: entity.id
                                        }, rect.2));
                                    }
                                    None => {}
                                }

                                insert = Some(StatemapRectWeight {
                                    weight: (rval.1).1,
                                    start: (rval.1).0,
                                    entity: entity.id
                                });
                            }
                        }
                        None => {}
                    }

                    entity.start = Some(time);
                    entity.state = Some(datum.state);
                    entity.tag = tag;
                    break;
                }

                if errmsg.is_some() {
                    return self.err(&errmsg.unwrap());
                }

                if update.is_some() {
                    let mut rweight = update.unwrap().0;
                    self.byweight.remove(&rweight);
                    rweight.weight = update.unwrap().1;
                    self.byweight.insert(rweight);
                }

                if insert.is_some() {
                    self.byweight.insert(insert.unwrap());
                }

                return Ok(Ingest::Success);
            }
            Err(_) => {}
        }

        match try_parse::<StatemapInputDescription>(payload) {
            Ok(None) => return Ok(Ingest::EndOfFile),
            Ok(Some(datum)) => {
                let entity = self.entity_lookup(&datum.entity);
                entity.description = Some(datum.description.to_string());

                return Ok(Ingest::Success);
            }
            Err(_) => {}
        }

        match try_parse::<StatemapInputEvent>(payload) {
            Ok(None) => return Ok(Ingest::EndOfFile),
            Ok(Some(_datum)) => {
                /*
                 * Right now, we don't do anything with events -- but the
                 * intent is to be able to render these in the statemap, so
                 * we also don't reject them.
                 */
                self.nevents += 1;

                return Ok(Ingest::Success);
            }
            Err(_) => {}
        }

        match try_parse_raw::<StatemapInputTag>(payload) {
            Ok(None) => return Ok(Ingest::EndOfFile),
            Ok(Some((datum, value))) => {
                if self.config.notags {
                    return Ok(Ingest::Success);
                }

                /*
                 * We allow tags to be redefined, so we need to first lookup
                 * our tag to see if it exists -- and if it does, we need
                 * to use the existing ID.
                 */
                let id;

                match self.tags.get(&(datum.state, datum.tag.to_string())) {
                    Some((_value, idr)) => { id = *idr }
                    None => { id = self.tags.len() }
                };

                self.tags.insert((datum.state, datum.tag), (value, id));

                return Ok(Ingest::Success);

            }
            Err(_) => {}
        }

        self.err("unrecognized payload")
    }

    pub fn ingest(&mut self, filename: &str) -> Result<(), Box<Error>> {
        let file = File::open(filename)?;
        let mut nrecs = 0;

        /*
         * Unsafe because Rust cannot enforce that the underlying data on
         * filesystem is not mutated while our program contains a &[u8]
         * reference to it. Mutating the file would result in undefined
         * behavior.
         */
        let mmap = unsafe { MmapOptions::new().map(&file)? };
        let mut contents = str::from_utf8(&mmap[..])?;
        let len = contents.len();

        self.ingest_metadata(&mut contents)?;

        /*
         * Now rip through our data pulling out concatenated JSON payloads.
         */
        loop {
            match self.ingest_datum(&mut contents) {
                Ok(Ingest::Success) => nrecs += 1,
                Ok(Ingest::EndOfFile) => break,
                Err(err) => {
                    /*
                     * Lazily compute the line number for our error message.
                     */
                    let remaining_len = contents.len();
                    let byte_offset = len - remaining_len;
                    let line = line_number(&mmap, byte_offset);
                    let message =
                        format!("illegal datum on line {}: {}", line, err);
                    return self.err(&message);
                }
            }

            while self.byweight.len() >= self.config.maxrect as usize {
                self.trim();
            }
        }
        
        self.ingest_end();

        eprintln!("{} records processed, {} rectangles",
            nrecs, self.byweight.len());
        Ok(())
    }

    fn output_defs(&self, config: &StatemapSVGConfig,
        globals: &StatemapSVGGlobals)
    {
        println!("<defs>");

        println!("<script type=\"application/ecmascript\"><![CDATA[");

        println!("var globals = {{");
        let str = serde_json::to_string_pretty(&config).unwrap();
        println!("{},", &str[2..str.len() - 2]);

        let str = serde_json::to_string_pretty(&globals).unwrap();
        println!("{},", &str[2..str.len() - 2]);

        /*
         * Provide an "entities" member that has the descriptions for each
         * entity, if they have one.  Yes, this is a little goofy -- it
         * would make much more sense to have a "descriptions" member that
         * consists of strings named by entity -- but we're doing this for
         * the sake of compatibility with the legacy implementation, however
         * dubious..
         */
        println!("  entities: {{");

        let mut comma = "";

        for entity in self.entities.values() {
            let val = match entity.description {
                Some(ref description) => {
                    format!("description: \"{}\"", description)
                }
                _ => { "".to_string() }
            };

            println!("    {} \"{}\": {{ {} }}", comma, entity.name, val);
            comma = ",";
        }

        println!("  }}");
        println!("}};");

        if self.tags.len() > 0 {
            /*
             * Pull our tags into a Vec so we can sort them and emit them in
             * array order.
             */
            let mut tags: Vec<(usize, u32, &str)> = vec![];

            for ((state, tag), (_value, id)) in self.tags.iter() {
                tags.push((*id, *state, tag));
            }

            tags.sort_unstable();

            println!("globals.tags = [");

            for i in 0..tags.len() {
                let (value, id) =
                    self.tags.get(&(tags[i].1, tags[i].2.to_string())).unwrap();

                assert_eq!(i, *id);
                println!("{}{}", serde_json::to_string_pretty(value).unwrap(),
                    if i < tags.len() - 1 { "," } else { "" });
            }

            println!("];");
            println!("globals.notags = false;");
        } else {
            println!("globals.tags = [];");
            println!("globals.notags = true;");
        }

        /*
         * Now drop in our in-SVG code.
         */
        let lib = include_str!("statemap-svg.js");

        println!("{}\n]]></script>", lib);

        /*
         * Next up: CSS.
         */
        let css = include_str!("statemap-svg.css");

        println!("<style type=\"text/css\"><![CDATA[\n{}\n]]></style>", css);

        /*
         * And now other definitions.
         */
        let defs = include_str!("statemap-svg.defs");
        println!("{}", defs);

        println!("</defs>");
    }

    pub fn output_svg(&self, config: &StatemapSVGConfig) ->
        Result<(), Box<Error>>
    {
        struct Props {
            x: u32,
            y: u32,
            height: u32,
            width: u32,
            lheight: u32,
            spacing: u32
        };

        let output_data = |data: &HashMap<&String, Vec<String>>| {
            println!("<defs>");
            println!(r##"<script type="application/ecmascript"><![CDATA["##);

            println!("g_data = {{ ");
            let mut comma = "";

            for entity in data.keys() {
                println!("{}{}: [", comma, entity);

                let datum = data.get(entity).unwrap();

                for i in 0..datum.len() - 1 {
                    println!("{},", datum[i]);
                }

                println!("{}", datum[datum.len() - 1]);
                println!("]");
                comma = ",";
            }

            println!(r##"}} ]]></script></defs>"##);
        };

        let output_controls = |props: &Props| {
            let width = props.width / 4;
            let mut x = 0;
            let y = 0;

            let icons = vec![
                (include_str!("./icons/arrow-left-l.svg"), "panclick(50, 0)"),
                (include_str!("./icons/zoom-in.svg"), "zoomclick(1.25)"),
                (include_str!("./icons/zoom-out.svg"), "zoomclick(0.8)"),
                (include_str!("./icons/arrow-right-l.svg"), "panclick(-50, 0)")
            ];

            println!(r##"<svg x="{}px" y="{}px" width="{}px" height="{}px">"##,
                props.x, props.y, props.width, props.height);

            for i in 0..icons.len() {
                println!(r##"<svg x="{}px" y="{}px" width="{}px" height="{}px"
                    onclick="{}"><rect x="0px" y="0px" width="{}px" 
                    height="{}px" onclick="{}" class="button" />{}</svg>"##,
                    x, y, width, width, icons[i].1,
                    width, width, icons[i].1, icons[i].0);
                x += width;
            }

            println!("</svg>");
        };

        let output_legend = |props: &Props, colors: &Vec<StatemapColor>| {
            let x = props.x;
            let mut y = props.y;
            let height = props.lheight;
            let width = props.width;

            for state in 0..self.states.len() {
                println!(r##"<rect x="{}" y="{}" width="{}" height="{}"
                    id="statemap-legend-{}" onclick="legendclick(evt, {})"
                    class="statemap-legend" style="fill:{}" />"##,
                    x, y, width, height, state, state, colors[state]);
                y += height + props.spacing;

                println!(concat!(r##"<text x="{}" y="{}" "##,
                    r##"class="statemap-legendlabel sansserif">{}</text>"##),
                    x + (width / 2), y, self.states[state].name);
                y += props.spacing;
            }
        };

        let output_tagbox = || {
            if !self.config.notags {
                println!(r##"<g id="statemap-tagbox"></g>"##);
                println!(r##"<g id="statemap-tagbox-select"></g>"##);
            }
        };

        let metadata = match self.metadata {
            Some(ref metadata) => { metadata }
            _ => { return self.err("metadata not found in data stream"); }
        };

        #[allow(non_snake_case)]
        let timeWidth = self.entities.values().fold(self.config.end,
            |latest, e| {
                match e.start {
                    Some(start) => cmp::max(latest, start),
                    None => latest
               }
            }) - self.config.begin;

        let lmargin = config.legendWidth;
        let tmargin = 60;
        let rmargin = config.tagWidth;

        let height = (self.entities.len() as u32 *
            config.stripHeight) + tmargin;
        let width = config.stripWidth + lmargin + rmargin;

        let mut props = Props { x: 20, y: tmargin, height: 45,
            width: lmargin, lheight: 15, spacing: 10 };

        let lheight = tmargin + props.height + (self.states.len() as u32 *
            (props.lheight + (props.spacing * 2)));

        let globals = StatemapSVGGlobals {
            begin: self.config.begin,
            end: self.config.end,
            pixelWidth: config.stripWidth,
            pixelHeight: height - tmargin,
            totalHeight: cmp::max(height, lheight),
            timeWidth: timeWidth,
            lmargin: lmargin,
            tmargin: tmargin,
            entityPrefix: "statemap-entity-".to_string(),
            states: &self.states,
            start: &metadata.start,
            entityKind: match metadata.entityKind {
                Some(ref kind) => { kind }
                None => { "Entity" }
            }
        };

        /*
         * Sort our entities, by whatever criteria has been specified.
         */
        let sort = match config.sortby {
            None => None,
            Some(ref sortby) => {
                if metadata.states.contains_key(sortby) {
                    Some(metadata.states.get(sortby).unwrap().value)
                } else {
                    if sortby == "entity" {
                        /*
                         * A state of "entity" denotes that we should sort
                         * by entity name.
                         */
                        None
                    } else {
                        return self.err(&format!(concat!("cannot sort by ",
                            "state \"{}\": no such state"), sortby));
                    }
                }
            }
        };

        let entities = self.sort(sort);

        /*
         * Make sure that all of our colors are valid.
         */
        let mut colors: Vec<StatemapColor> = vec![];

        for i in 0..self.states.len() {
            match self.states[i].color {
                Some(ref name) => {
                    match StatemapColor::from_str(name) {
                        Ok(color) => colors.push(color),
                        Err(_err) => {
                            return self.err(&format!(concat!("illegal color",
                                "\"{}\" for state \"{}\""), name,
                                self.states[i].name));
                        }
                    }
                }
                None => colors.push(StatemapColor::random())
            }
        }

        println!(r##"<?xml version="1.0"?>
            <!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN"
                "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
            <svg width="{}" height="{}"
                xmlns="http://www.w3.org/2000/svg"
                version="1.1"
                onload="init(evt)">"##, width, globals.totalHeight);

        self.output_defs(config, &globals);

        println!(r##"<svg x="{}px" y="{}px" width="{}px" height="{}px">"##,
            lmargin, tmargin, globals.pixelWidth, height - tmargin);

        /*
         * First, we drop down a background rectangle as big as our SVG. This
         * color will be changed dynamically to be a highlight color, and
         * then rectangles can be made transparent to become highlighted.
         */
        println!(concat!(r##"<rect x="0px" y="0px" width="{}px" "##,
            r##"height="{}px" fill="{}" id="statemap-highlight" />"##),
            globals.pixelWidth, height - tmargin, config.background);

        println!(r##"<g id="statemap" transform="matrix(1 0 0 1 0 0)">"##);

        let mut y = 0;
        let mut data = HashMap::new();

        for e in entities {
            let entity = self.entities.get(self.byid.get(e).unwrap()).unwrap();

            println!("{}", e);
            data.insert(&entity.name,
                entity.output_svg(config, &globals, &colors, y));
            y += config.stripHeight;
        }

        println!("</g>");
        println!("</svg>");

        output_data(&data);

        /*
         * The border around our statemap.
         */
        println!(r##"<polygon class="statemap-border""##);
        println!(r##"  points="{} {}, {} {}, {} {}, {} {}"/>"##,
            lmargin, tmargin, lmargin + globals.pixelWidth, tmargin,
            lmargin + globals.pixelWidth, height, lmargin, height);

        println!(concat!(r##"<text x="{}" y="{}" "##,
            r##"class="statemap-title sansserif">{}</text>"##),
            lmargin + (globals.pixelWidth / 2), 16, metadata.title);

        println!(concat!(r##"<text x="{}" y="{}" class="statemap-timelabel"##,
            r##" sansserif" id="statemap-timelabel"></text>"##),
            lmargin + (globals.pixelWidth / 2), 34);

        println!(r##"<line x1="{}" y1="{}" x2="{}" y2="{}""##,
            lmargin + 10, 40, lmargin + globals.pixelWidth - 10, 40);
        println!(r##"class="statemap-timeline" />"##);

        props.width -= (2 * props.x) + 10;

        output_controls(&props);

        props.y += props.height;
        output_legend(&props, &colors);

        output_tagbox();

        println!("</svg>");

        Ok(())
    }
}

fn try_parse<'de, T>(content: &mut &'de str)
    -> Result<Option<T>, serde_json::Error>
where
    T: serde::Deserialize<'de>
{
    let mut de = serde_json::Deserializer::from_str(*content).into_iter();
    match de.next() {
        Some(Ok(value)) => {
            *content = &content[de.byte_offset()..];
            Ok(Some(value))
        }
        Some(Err(err)) => Err(err),
        None => Ok(None),
    }
}

fn try_parse_raw<'de, T>(content: &mut &'de str)
    -> Result<Option<(T, serde_json::Value)>, serde_json::Error>
where
    T: serde::Deserialize<'de>
{
    let mut de = serde_json::Deserializer::from_str(*content).into_iter();
    let offset = de.byte_offset();

    match de.next() {
        Some(Ok(value)) => {
            let v: serde_json::Value =
                serde_json::from_str(&content[offset..de.byte_offset()])?;

            *content = &content[de.byte_offset()..];

            Ok(Some((value, v)))
        }
        Some(Err(err)) => Err(err),
        None => Ok(None),
    }
}

fn line_number(mmap: &[u8], byte_offset: usize) -> usize {
    let mut nls = mmap[..byte_offset].iter().filter(|&&b| b == b'\n').count();

    /*
     * We report the line number of the first non-whitespace character after
     * byte_offset.
     */
    for b in mmap[byte_offset..].iter() {
        if *b == b'\n' {
            nls += 1;
        }

        if !((*b as char).is_whitespace()) {
            break;
        }
    }

    nls + 1
}

/*
 * The time value is written in the input as a JSON string containing a number.
 * Deserialize just the number here without allocating memory for a String.
 */
fn datum_time_from_string<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: &str = serde::Deserialize::deserialize(deserializer)?;
    match u64::from_str(s) {
        Ok(time) => Ok(time),
        Err(_) => Err(serde::de::Error::custom("illegal time value")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::process;
    use std::fs;
    use std::io::Write;

    fn metadata(config: Option<&Config>, mut metadata: &str) -> Statemap {
        let mut statemap;

        match config {
            Some(config) => { statemap = Statemap::new(&config); },
            None => {
                let config: Config = Default::default();
                statemap = Statemap::new(&config);
            }
        }

        match statemap.ingest_metadata(&mut metadata) {
            Err(err) => { panic!("metadata incorrectly failed: {:?}", err); }
            Ok(_) => { statemap }
        }
    }

    fn minimal(config: Option<&Config>) -> Statemap {
        metadata(config, r##"{
            "start": [ 0, 0 ],
            "title": "Foo",
            "states": {
                "zero": {"value": 0 },
                "one": {"value": 1 }
            }
        }"##)
    }

    fn data(config: Option<&Config>, data: Vec<&str>) -> Statemap {
        let mut statemap = minimal(config);

        for mut datum in data {
            match statemap.ingest_datum(&mut datum) {
                Err(err) => { panic!("data incorrectly failed: {:?}", err); }
                Ok(_) => {}
            }
        }

        statemap
    }

    fn bad_metadata(mut metadata: &str, expected: &str) {
        let config: Config = Default::default();
        let mut statemap = Statemap::new(&config);

        match statemap.ingest_metadata(&mut metadata) {
            Err(err) => {
                let errmsg = format!("{}", err);

                if errmsg.find(expected).is_none() {
                    panic!("error ('{}') did not contain '{}' as expected",
                        errmsg, expected);
                }
            },
            Ok(_) => { panic!("bad metadata succeeded!"); }
        }
    }

    fn bad_datum(operand: Option<Statemap>, mut datum: &str, expected: &str) {
        let mut statemap = match operand {
            Some(statemap) => statemap,
            None => {
                metadata(None, r##"{
                    "start": [ 0, 0 ],
                    "title": "Foo",
                    "states": {
                        "zero": {"value": 0 },
                        "one": {"value": 1 }
                    }
                }"##)
            }
        };

        match statemap.ingest_datum(&mut datum) {
            Err(err) => {
                let errmsg = format!("{}", err);

                if errmsg.find(expected).is_none() {
                    panic!("error ('{}') did not contain '{}' as expected",
                        errmsg, expected);
                }
            },
            Ok(_) => { panic!("bad datum succeeded!"); }
        }
    }

    fn statemap_ingest(statemap: &mut Statemap, raw: &str)
        -> Result<(), Box<Error>>
    {
        let mut path = env::temp_dir();
        path.push(format!("statemap.test.{}.{:p}", process::id(), statemap));

        let filename = path.to_str().unwrap();
        let mut file = File::create(filename)?;
        file.write_all(raw.as_bytes())?;

        let result = statemap.ingest(filename);

        fs::remove_file(filename)?;

        result
    }

    fn bad_statemap(raw: &str, expected: &str) {
        let config: Config = Default::default();
        let mut statemap = Statemap::new(&config);

        match statemap_ingest(&mut statemap, raw) {
            Err(err) => {
                let errmsg = format!("{}\n", err);

                if errmsg.find(expected).is_none() {
                    panic!("error ('{}') did not contain '{}' as expected",
                        errmsg, expected);
                }
            },
            Ok(_) => { panic!("bad statemap succeeded!"); }
        }
    }

    macro_rules! bad_statemap {
        ($what:expr) => ({
            bad_statemap(include_str!(concat!("../tst/tst.", $what, ".in")),
                include_str!(concat!("../tst/tst.", $what, ".err")));
        });
    }

    fn good_statemap(raw: &str) -> Statemap {
        let mut config: Config = Default::default();
        config.notags = false;

        let mut statemap = Statemap::new(&config);

        match statemap_ingest(&mut statemap, raw) {
            Err(err) => {
                panic!("statemap failed: {}", err);
            },
            Ok(_) => { statemap }
        }
    }

    macro_rules! good_statemap {
        ($what:expr) => ({
            good_statemap(include_str!(concat!("../tst/tst.", $what, ".in")))
        })
    }

    #[test]
    fn good_minimal() {
        metadata(None, r##"{
            "start": [ 0, 0 ],
            "title": "Foo",
            "states": {
                "zero": {"value": 0 }
            }
        }"##);
    }

    #[test]
    fn bad_title_missing() {
        bad_metadata(r##"{
            "start": [ 0, 0 ],
            "states": {
                "zero": {"value": 0 }
            }
        }"##, "missing field `title`");
    }

    #[test]
    fn bad_start_missing() {
        bad_metadata(r##"{
            "title": "Foo",
            "states": {
                "zero": {"value": 0 }
            }
        }"##, "missing field `start`");
    }

    #[test]
    fn bad_start_badval() {
        bad_metadata(r##"{
            "start": [ -1, 0 ],
            "title": "Foo",
            "states": {
                "zero": {"value": 0 }
            }
        }"##, "invalid value: integer `-1`");
    }

    #[test]
    fn bad_start_tooshort() {
        bad_metadata(r##"{
            "start": [ 0 ],
            "title": "Foo",
            "states": {
                "zero": {"value": 0 }
            }
        }"##, "\"start\" property must be a two element array");
    }

    #[test]
    fn bad_start_toolong() {
        bad_metadata(r##"{
            "start": [ 0, 0, 3 ],
            "title": "Foo",
            "states": {
                "zero": {"value": 0 }
            }
        }"##, "\"start\" property must be a two element array");
    }

    #[test]
    fn bad_states_missing() {
        bad_metadata(r##"{
            "start": [ 0, 0 ],
            "title": "Foo"
        }"##, "missing field `states`");
    }

    #[test]
    fn bad_states_badmap() {
        bad_metadata(r##"{
            "start": [ 0, 0 ],
            "title": "Foo",
            "states": 123
        }"##, "expected a map");
    }

    #[test]
    fn bad_states_value_missing() {
        bad_metadata(r##"{
            "start": [ 0, 0 ],
            "title": "Foo",
            "states": {
                "zero": {}
            }
        }"##, "missing field `value`");
    }

    #[test]
    fn bad_states_value_bad() {
        bad_metadata(r##"{
            "start": [ 0, 0 ],
            "title": "Foo",
            "states": {
                "zero": {"value": -1 }
            }
        }"##, "invalid value: integer `-1`");
    }

    #[test]
    fn bad_states_value_skipped1() {
        bad_metadata(r##"{
            "start": [ 0, 0 ],
            "title": "Foo",
            "states": {
                "zero": {"value": 0 },
                "one": {"value": 2 }
            }
        }"##, "state \"one\" has value (2) that exceeds maximum");
    }

    #[test]
    fn bad_states_value_toohigh() {
        bad_metadata(r##"{
            "start": [ 0, 0 ],
            "title": "Foo",
            "states": {
                "zero": {"value": 1 },
                "one": {"value": 2 }
            }
        }"##, "state \"one\" has value (2) that exceeds maximum");
    }

    #[test]
    fn bad_states_value_duplicate() {
        bad_metadata(r##"{
            "start": [ 0, 0 ],
            "title": "Foo",
            "states": {
                "zero": {"value": 1 },
                "one": {"value": 1 }
            }
        }"##, "has value (1) that conflicts");
    }

    #[test]
    fn bad_line_basic() {
        bad_statemap!("bad_line_basic");
    }

    #[test]
    fn bad_line_whitespace() {
        bad_statemap!("bad_line_whitespace");
    }

    #[test]
    fn bad_line_newline() {
        bad_statemap!("bad_line_newline");
    }

    #[test]
    fn basic() {
        let statemap = metadata(None, r##"{
            "start": [ 1528417173, 255882937 ],
            "title": "Foo",
            "host": "HA8S7MRD2",
            "entityKind": "Process",
            "states": {
                "on-cpu": {"value": 0, "color": "#2e9107" },
                "off-cpu-waiting": {"value": 1, "color": "#f9f9f9" },
                "off-cpu-semop": {"value": 2, "color": "#FF5733" },
                "off-cpu-blocked": {"value": 3, "color": "#C70039" },
                "off-cpu-zfs-read": {"value": 4, "color": "#FFC300" },
                "off-cpu-zfs-write": {"value": 5, "color": "#338AFF" },
                "off-cpu-zil-commit": {"value": 6, "color": "#66FFCC" },
                "off-cpu-tx-delay": {"value": 7, "color": "#e1ff00" },
                "off-cpu-dead": {"value": 8, "color": "#E0E0E0" }
            }
        }"##);
        assert_eq!(statemap.states.len(), 9);
        assert_eq!(statemap.states[0].name, "on-cpu");
        assert_eq!(statemap.states[0].color, Some("#2e9107".to_string()));
        assert_eq!(statemap.states[1].name, "off-cpu-waiting");
        assert_eq!(statemap.states[1].color, Some("#f9f9f9".to_string()));
        assert_eq!(statemap.states[8].name, "off-cpu-dead");
    }

    #[test]
    fn basic_datum() {
        let mut _statemap = data(None, vec![
            r##"{ "time": "156683", "entity": "foo", "state": 0 }"##
        ]);
    }

    #[test]
    fn basic_description() {
        let mut statemap = data(None, vec![
            r##"{ "time": "156683", "entity": "foo", "state": 0 }"##,
            r##"{ "entity": "foo", "description": "This is a foo!" }"##
        ]);

        assert_eq!(statemap.entity_lookup("foo").description,
            Some("This is a foo!".to_string()));
    }

    #[test]
    fn bad_datum_badtime() {
        bad_datum(None, r##"
            { "time": 156683, "entity": "foo", "state": 0 }
        "##, "unrecognized payload");
    }

    #[test]
    fn bad_datum_badtime_float() {
        bad_datum(None, r##"
            { "time": "156683.12", "entity": "foo", "state": 0 }
        "##, "unrecognized payload");
    }

    #[test]
    fn bad_datum_nostate() {
        bad_datum(None, r##"
            { "time": "156683", "entity": "foo" }
        "##, "unrecognized payload");
    }

    #[test]
    fn bad_datum_badstate() {
        bad_datum(None, r##"
            { "time": "156683", "entity": "foo", "state": 200 }
        "##, "illegal state value");
    }

    #[test]
    fn bad_datum_backwards() {
        let statemap = data(None, vec![
            r##"{ "time": "156683", "entity": "foo", "state": 0 }"##
        ]);

        bad_datum(Some(statemap), r##"
            { "time": "156682", "entity": "foo", "state": 1 }
        "##, "out of order with respect to prior time");
    }

    #[test]
    fn basic_data() {
        let statemap = data(None, vec![
            r##"{ "time": "100000", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "200000", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "300000", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "400000", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "500000", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "600000", "entity": "foo", "state": 1 }"##
        ]);

        statemap.verify();
    }

    #[test]
    fn subsume() {
        let mut statemap = data(None, vec![
            r##"{ "time": "100000", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "200000", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "300000", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "400000", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "500000", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "600000", "entity": "foo", "state": 1 }"##
        ]);

        statemap.print("Initial load");
        statemap.verify();
        statemap.subsume_apply_and_verify("foo", 100000);
        statemap.subsume_apply_and_verify("foo", 300000);
        statemap.subsume_apply_and_verify("foo", 100000);
    }

    #[test]
    fn subsume_right() {
        let mut statemap = data(None, vec![
            r##"{ "time": "100000", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "200000", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "300000", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "400000", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "500000", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "600000", "entity": "foo", "state": 1 }"##
        ]);

        statemap.print("Initial load");
        statemap.verify();

        statemap.subsume_apply_and_verify("foo", 500000);
        statemap.subsume_apply_and_verify("foo", 400000);
        statemap.subsume_apply_and_verify("foo", 300000);
        statemap.subsume_apply_and_verify("foo", 200000);
    }

    #[test]
    fn subsume_middle() {
        let mut statemap = data(None, vec![
            r##"{ "time": "100000", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "200000", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "300000", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "400000", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "500000", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "600000", "entity": "foo", "state": 1 }"##
        ]);

        statemap.print("Initial load");
        statemap.verify();
        statemap.subsume_apply_and_verify("foo", 300000);
        statemap.subsume_apply_and_verify("foo", 300000);
        statemap.subsume_apply_and_verify("foo", 200000);
    }

    #[test]
    fn subsume_tagged() {
        let mut statemap = data(None, vec![
            r##"{ "time": "100", "entity": "foo", "state": 0, "tag": "a" }"##,
            r##"{ "time": "200", "entity": "foo", "state": 1, "tag": "b" }"##,
            r##"{ "time": "300", "entity": "foo", "state": 0, "tag": "c" }"##,
            r##"{ "time": "400", "entity": "foo", "state": 1, "tag": "b" }"##,
            r##"{ "time": "500", "entity": "foo", "state": 0, "tag": "a" }"##,
            r##"{ "time": "600", "entity": "foo", "state": 1, "tag": "b" }"##
        ]);

        statemap.print("Initial load");
        statemap.verify();
        statemap.subsume_apply_and_verify("foo", 100);
        statemap.subsume_apply_and_verify("foo", 300);
        statemap.subsume_apply_and_verify("foo", 100);
    }

    #[test]
    fn trim() {
        let mut statemap = data(None, vec![
            r##"{ "time": "0", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "100", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "101", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "104", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "106", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "206", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "207", "entity": "foo", "state": 0 }"##
        ]);

        statemap.print("Initial");

        statemap.trim();
        statemap.verify();
        statemap.print("After first trim");

        statemap.trim();
        statemap.verify();
        statemap.print("After second trim");

        statemap.trim();
        statemap.verify();
        statemap.print("After third trim");
    }

    #[test]
    fn trim_insert() {
        let mut statemap = data(None, vec![
            r##"{ "time": "0", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "100", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "101", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "104", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "106", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "206", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "207", "entity": "foo", "state": 0 }"##
        ]);

        statemap.print("Initial");

        statemap.trim();
        statemap.verify();
        statemap.print("After first trim");

        let mut datum = r##"{ "time": "210", "entity": "foo", "state": 1 }"##;

        assert!(statemap.ingest_datum(&mut datum).is_ok());
        statemap.verify();
        statemap.print("After insert");

        statemap.trim();
        statemap.verify();
        statemap.print("After second trim");
    }

    #[test]
    fn trim_multient() {
        let mut statemap = data(None, vec![
            r##"{ "time": "0", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "1000", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "1010", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "1040", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "1060", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "2060", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "2070", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "0", "entity": "bar", "state": 0 }"##,
            r##"{ "time": "10", "entity": "bar", "state": 1 }"##,
        ]);

        statemap.print("Initial");

        statemap.trim();
        statemap.verify();
        statemap.print("After trim");
    }

    #[test]
    fn data_begin_time() {
        let mut config: Config = Default::default();
        config.begin = 200000;

        let statemap = data(Some(&config), vec![
            r##"{ "time": "100000", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "200000", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "300000", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "400000", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "500000", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "600000", "entity": "foo", "state": 1 }"##
        ]);

        statemap.verify();
        statemap.print("Begin at 200000");

        let rects = statemap.get_rects("foo");
        assert_eq!(rects.len(), 4);
        assert_eq!(rects[0].0, 200000);
        assert_eq!(rects[0].1, 300000 - 200000);
    }

    #[test]
    fn data_begin_time_later() {
        let mut config: Config = Default::default();
        config.begin = 200001;

        let statemap = data(Some(&config), vec![
            r##"{ "time": "100000", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "200000", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "300000", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "400000", "entity": "foo", "state": 1 }"##,
            r##"{ "time": "500000", "entity": "foo", "state": 0 }"##,
            r##"{ "time": "600000", "entity": "foo", "state": 1 }"##
        ]);

        statemap.verify();
        statemap.print("Begin at 200001");

        let rects = statemap.get_rects("foo");
        assert_eq!(rects.len(), 4);
        assert_eq!(rects[0].0, 200001);
        assert_eq!(rects[0].1, 300000 - 200001);
        assert_eq!((rects[0].2)[0], 0);
        assert_eq!((rects[0].2)[1], 300000 - 200001);
    }

    #[test]
    fn color_named() {
        let colors = vec![
            ("aliceblue", (240, 248, 255)),
            ("antiquewhite", (250, 235, 215)),
            ("aqua", (0, 255, 255)),
            ("aquamarine", (127, 255, 212)),
            ("azure", (240, 255, 255)),
            ("beige", (245, 245, 220)),
            ("bisque", (255, 228, 196)),
            ("black", (0, 0, 0)),
            ("blanchedalmond", (255, 235, 205)),
            ("blue", (0, 0, 255)),
            ("blueviolet", (138, 43, 226)),
            ("brown", (165, 42, 42)),
            ("burlywood", (222, 184, 135)),
            ("cadetblue", (95, 158, 160)),
            ("chartreuse", (127, 255, 0)),
            ("chocolate", (210, 105, 30)),
            ("coral", (255, 127, 80)),
            ("cornflowerblue", (100, 149, 237)),
            ("cornsilk", (255, 248, 220)),
            ("crimson", (220, 20, 60)),
            ("cyan", (0, 255, 255)),
            ("darkblue", (0, 0, 139)),
            ("darkcyan", (0, 139, 139)),
            ("darkgoldenrod", (184, 134, 11)),
            ("darkgray", (169, 169, 169)),
            ("darkgreen", (0, 100, 0)),
            ("darkgrey", (169, 169, 169)),
            ("darkkhaki", (189, 183, 107)),
            ("darkmagenta", (139, 0, 139)),
            ("darkolivegreen", (85, 107, 47)),
            ("darkorange", (255, 140, 0)),
            ("darkorchid", (153, 50, 204)),
            ("darkred", (139, 0, 0)),
            ("darksalmon", (233, 150, 122)),
            ("darkseagreen", (143, 188, 143)),
            ("darkslateblue", (72, 61, 139)),
            ("darkslategray", (47, 79, 79)),
            ("darkslategrey", (47, 79, 79)),
            ("darkturquoise", (0, 206, 209)),
            ("darkviolet", (148, 0, 211)),
            ("deeppink", (255, 20, 147)),
            ("deepskyblue", (0, 191, 255)),
            ("dimgray", (105, 105, 105)),
            ("dimgrey", (105, 105, 105)),
            ("dodgerblue", (30, 144, 255)),
            ("firebrick", (178, 34, 34)),
            ("floralwhite", (255, 250, 240)),
            ("forestgreen", (34, 139, 34)),
            ("fuchsia", (255, 0, 255)),
            ("gainsboro", (220, 220, 220)),
            ("ghostwhite", (248, 248, 255)),
            ("gold", (255, 215, 0)),
            ("goldenrod", (218, 165, 32)),
            ("gray", (128, 128, 128)),
            ("green", (0, 128, 0)),
            ("greenyellow", (173, 255, 47)),
            ("grey", (128, 128, 128)),
            ("honeydew", (240, 255, 240)),
            ("hotpink", (255, 105, 180)),
            ("indianred", (205, 92, 92)),
            ("indigo", (75, 0, 130)),
            ("ivory", (255, 255, 240)),
            ("khaki", (240, 230, 140)),
            ("lavender", (230, 230, 250)),
            ("lavenderblush", (255, 240, 245)),
            ("lawngreen", (124, 252, 0)),
            ("lemonchiffon", (255, 250, 205)),
            ("lightblue", (173, 216, 230)),
            ("lightcoral", (240, 128, 128)),
            ("lightcyan", (224, 255, 255)),
            ("lightgoldenrodyellow", (250, 250, 210)),
            ("lightgray", (211, 211, 211)),
            ("lightgreen", (144, 238, 144)),
            ("lightgrey", (211, 211, 211)),
            ("lightpink", (255, 182, 193)),
            ("lightsalmon", (255, 160, 122)),
            ("lightseagreen", (32, 178, 170)),
            ("lightskyblue", (135, 206, 250)),
            ("lightslategray", (119, 136, 153)),
            ("lightslategrey", (119, 136, 153)),
            ("lightsteelblue", (176, 196, 222)),
            ("lightyellow", (255, 255, 224)),
            ("lime", (0, 255, 0)),
            ("limegreen", (50, 205, 50)),
            ("linen", (250, 240, 230)),
            ("magenta", (255, 0, 255)),
            ("maroon", (128, 0, 0)),
            ("mediumaquamarine", (102, 205, 170)),
            ("mediumblue", (0, 0, 205)),
            ("mediumorchid", (186, 85, 211)),
            ("mediumpurple", (147, 112, 219)),
            ("mediumseagreen", (60, 179, 113)),
            ("mediumslateblue", (123, 104, 238)),
            ("mediumspringgreen", (0, 250, 154)),
            ("mediumturquoise", (72, 209, 204)),
            ("mediumvioletred", (199, 21, 133)),
            ("midnightblue", (25, 25, 112)),
            ("mintcream", (245, 255, 250)),
            ("mistyrose", (255, 228, 225)),
            ("moccasin", (255, 228, 181)),
            ("navajowhite", (255, 222, 173)),
            ("navy", (0, 0, 128)),
            ("oldlace", (253, 245, 230)),
            ("olive", (128, 128, 0)),
            ("olivedrab", (107, 142, 35)),
            ("orange", (255, 165, 0)),
            ("orangered", (255, 69, 0)),
            ("orchid", (218, 112, 214)),
            ("palegoldenrod", (238, 232, 170)),
            ("palegreen", (152, 251, 152)),
            ("paleturquoise", (175, 238, 238)),
            ("palevioletred", (219, 112, 147)),
            ("papayawhip", (255, 239, 213)),
            ("peachpuff", (255, 218, 185)),
            ("peru", (205, 133, 63)),
            ("pink", (255, 192, 203)),
            ("plum", (221, 160, 221)),
            ("powderblue", (176, 224, 230)),
            ("purple", (128, 0, 128)),
            ("rebeccapurple", (102, 51, 153)),
            ("red", (255, 0, 0)),
            ("rosybrown", (188, 143, 143)),
            ("royalblue", (65, 105, 225)),
            ("saddlebrown", (139, 69, 19)),
            ("salmon", (250, 128, 114)),
            ("sandybrown", (244, 164, 96)),
            ("seagreen", (46, 139, 87)),
            ("seashell", (255, 245, 238)),
            ("sienna", (160, 82, 45)),
            ("silver", (192, 192, 192)),
            ("skyblue", (135, 206, 235)),
            ("slateblue", (106, 90, 205)),
            ("slategray", (112, 128, 144)),
            ("slategrey", (112, 128, 144)),
            ("snow", (255, 250, 250)),
            ("springgreen", (0, 255, 127)),
            ("steelblue", (70, 130, 180)),
            ("tan", (210, 180, 140)),
            ("teal", (0, 128, 128)),
            ("thistle", (216, 191, 216)),
            ("tomato", (255, 99, 71)),
            ("turquoise", (64, 224, 208)),
            ("violet", (238, 130, 238)),
            ("wheat", (245, 222, 179)),
            ("white", (255, 255, 255)),
            ("whitesmoke", (245, 245, 245)),
            ("yellow", (255, 255, 0)),
            ("yellowgreen", (154, 205, 50)),
        ];

        let notcolors = vec!["nixon", "yellowgreenybeeny", "#1234567",
            "1234567", "$123456", "#123456#" ];

        for i in 0..notcolors.len() {
            match StatemapColor::from_str(notcolors[i]) {
                Ok(color) => {
                    panic!("lookup of {} succeeded with {:?}!",
                        notcolors[i], color);
                },
                Err(err) => {
                    println!("lookup of {} failed with {}", notcolors[i], err);
                }
            }
        }

        for i in 0..colors.len() {
            match StatemapColor::from_str(colors[i].0) {
                Ok(color) => {
                    let out = format!("rgb({}, {}, {})",
                        (colors[i].1).0, (colors[i].1).1, (colors[i].1).2);
                    assert_eq!(out, color.to_string());
                },
                Err(err) => {
                    panic!("lookup of {} failed with {}!", colors[i].0, err);
                }
            }
        }
    }

    #[test]
    fn color_mix() {
        let red = StatemapColor::from_str("red").unwrap();
        let white = StatemapColor::from_str("white").unwrap();

        let tests = vec![
            (0.0, "rgb(255, 0, 0)"),
            (0.25, "rgb(255, 137, 137)"),
            (0.5, "rgb(255, 188, 188)"),
            (0.75, "rgb(255, 225, 225)"),
            (1.0, "rgb(255, 255, 255)"),
        ];

        for i in 0..tests.len() {
            assert_eq!(red._mix(&white, tests[i].0).to_string(), tests[i].1);
            assert_eq!(red._mix(&white, tests[i].0).to_string(),
                white._mix(&red, 1.0 - tests[i].0).to_string());
        }
    }

    #[test]
    fn color_mix_linear() {
        let color = StatemapColor::from_str("#2e9107").unwrap();
        let other = StatemapColor::from_str("#f9f9f9").unwrap();
        let ratio = 351500 as f64 / 840108 as f64;
        let mix = color._mix(&other, ratio as f32);

        println!("color={}, other={}, mix={}", color, other, mix);
    }

    #[test]
    fn color_mix_nonlinear() {
        let color = StatemapColor::from_str("#2e9107").unwrap();
        let other = StatemapColor::from_str("#f9f9f9").unwrap();
        let ratio = 351500 as f64 / 840108 as f64;
        let mix = color.mix_nonlinear(&other, ratio as f32);

        println!("color={}, other={}, mix={}", color, other, mix);
    }

    #[test]
    fn tag_basic() {
        let statemap = good_statemap!("tag_basic");
        println!("{:?}", statemap.tags);
        statemap.verify();
    }

    #[test]
    fn tag_redefined() {
        let statemap = good_statemap!("tag_redefined");
        println!("{:?}", statemap.tags);
        statemap.verify();
    }
}
