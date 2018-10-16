# Statemap

This repository contains the software for rendering _statemaps_, a
software visualization in which time is on the X axis and timelines
for discrete entities are stacked on the Y axis, with different states
for the discrete entities rendered in different colors.

Generating a statemap consists of two steps: *instrumentation* and
*rendering*.  The result is a SVG that can be visualized with a SVG 
viewer (e.g., a web browser), allowing *interaction*.

## Installation

To compile the command to render a statemap from instrumentation data:

    cargo build --release

Note that statemap requires Rust.

## Instrumentation

Statemaps themselves are methodology- and OS-agnostic, but instrumentation
is usually more system-specific.
The `contrib` directory contains instrumentation for specific methodologies
and systems that will generate data that can be used 
as input to the `statemap` command:

<table>
<tr>
<th>Name</th>
<th>Method</th>
<th>OS</th>
<th>Statemap description</th>
</tr>
<tr>
<td><a href="./contrib/cpu-statemap.d">cpu-statemap.d</a></td>
<td>DTrace</td>
<td>SmartOS</td>
<td>CPU activity by CPU</td>
</tr>
<tr>
<td><a href="./contrib/cpu-statemap-tagged.d">cpu-statemap-tagged.d</a></td>
<td>DTrace</td>
<td>SmartOS</td>
<td>CPU activity by CPU, tagged by origin of activity</td>
</tr>
<tr>
<td><a href="./contrib/io-statemap.d">io-statemap.d</a></td>
<td>DTrace</td>
<td>SmartOS</td>
<td>SCSI devices in terms of number of outstanding I/O operations</td>
</tr>
<tr>
<td><a href="./contrib/lx-cmd-statemap.d">lx-cmd-statemap.d</a></td>
<td>DTrace</td>
<td>SmartOS</td>
<td>Processes and threads of a specified command in an LX zone</td>
</tr>
<tr>
<td><a href="./contrib/lx-statemap.d">lx-statemap.d</a></td>
<td>DTrace</td>
<td>SmartOS</td>
<td>Threads in a specified process in an LX zone</td>
</tr>
<tr>
<td><a href="./contrib/postgres-statemap.d">postgres-statemap.d</a></td>
<td>DTrace</td>
<td>SmartOS</td>
<td>PostgreSQL processes</td>
</tr>
<tr>
<td><a href="./contrib/postgres-zfs-statemap.d">postgres-zfs-statemap.d</a></td>
<td>DTrace</td>
<td>SmartOS</td>
<td>PostgreSQL processes, with ZFS-specific states</td>
</tr>
</table>

### Data format

To generate data for statemap generation,
instrumentation should create a file that consists of a stream of
concatenated JSON.
The expectation is that one JSON payload will consist of
metadata, with many JSON payloads containing data, but the metadata may
be split across multiple JSON payloads.  (No field can appear more than
once, however.)

#### Metadata

The following metadata fields are required: 

- `start`: A two-element array of integers consisting of the start time of
the data in seconds (the 0th element) and nanoseconds within the 
second (the 1st element).  The start time should be expressed in UTC.

- `states`: An object in which each member is the name of a valid
  entity state.  Each member object can contain the following :

  - `value`: The value by which this state will be referred to in the 
    data stream.

  - `color`: The color that should be used to render the state. If the
    color is not specified, a color will be selected at random.

  For example, here is a valid `states` object:

        "states": {
                "on-cpu": {"value": 0, "color": "#DAF7A6" },
                "off-cpu-waiting": {"value": 1, "color": "#f9f9f9" },
                "off-cpu-futex": {"value": 2, "color": "#f0f0f0" },
                "off-cpu-io": {"value": 3, "color": "#FFC300" },
                "off-cpu-blocked": {"value": 4, "color": "#C70039" },
                "off-cpu-dead": {"value": 5, "color": "#581845" }
        }
  
In addition, the metadata can contain the following optional fields are
optional:

- `title`: The title of the statemap.

- `host`: The host on which the data was gathered.

#### Data

The data for a statemap is provided following the metadata as
concatenated JSON (that is, each JSON payload is a datum).  Each
datum is a JSON object that must contain the following members:

- `entity`: The name of the entity.

- `time`: The time of the datum, expressed as a nanosecond offset from
  the `start` member present in the metadata.

- `state`: The value of the state that begins at the time of the datum.

Each datum may also contain an additional member:

- `tag`: The tag for the state.  See State tagging, below.

#### State tagging

It is often helpful to examine additional dimensionality within a particular
state or states.  For example, in understanding CPU activity, it may be
helpful to understand not just that a CPU was in a state in which it was
executing a user thread, but the nature of the thread itself:  the thread
identifier, process identifier, process name, and so on.  To facilitate this,
statemaps support *state tagging* whereby an immutable tag is associated with a
particular transition to a particular state.  There can be an arbitrary
number of such tags, but the expectation is that there are many more state
transitions than there are tags.  Tags are indicated by the `tag` member of
the state datum payload.  Elsewhere in the stream of data (though not
necessarily before the tag is used), the tag should be defined with
a tag-defining JSON payload that contains the following two members:

- `tag`: A string that is the tag that is being defined.

- `state`: The state that corresponds to this tag.  Each `state`/`tag` tuple
  must have its own tag definition.

Beyond these two members, the tag definition can have any number of scalar
members.  Tags are immutable; if a tag is redefined, the last tag definition
will apply to all uses of that tag.  The tag should not contain member
definitions that would cause it to be ambiguous with respect to data (namely,
`entity` and `time` members).

As an example, here is a tag definition for a state that is associated with
interrupt activity that indicates the source device:

```
{ "state": 6, "tag": "ffffd0c4f8f52000", "driver": "mpt_sas", "instance": 1 }
```

And here is an example of a tagged state datum:

```
{ "time": "1579579142", "entity": "55", "state": 6, "tag": "ffffd0c4f8f52000" }
```

This would indicate that at time 1579579142, entity 55 went into state 6 --
and the tag for this state (in this case, the interrupting device) was
instance 1 of the `mpt_sas` driver.

## Rendering

To render a statemap, run the `statemap` command, providing an instrumentation
data file.  The resulting statemap will be written as a SVG on standard
output:

    statemap my-instrumentation-output.out > statemap.svg

Statemaps are interactive; the resulting SVG will contain controls that
enable it to be zoomed, panned, states selected, etc. (See Interaction,
below.)

By default, statemaps consist of all states for the entire time duration
represented in the input data.  Because there can be many, many states
represented in the input, states will (by default) be _coalesced_ when
the time spent in a state is deemed a sufficiently small fraction of
the overall time.  For a coalesced state, the statemap will track the
overall fraction of states present (and will use a color that represents
a proportional blend of those states' colors).  When a statemap contains
coalesced states, some information will be lost (namely, the exact
time delineations of state transitions within the coalesced state).
Coalesced states can be eliminated in one of two ways:  either the
state coalescence target can be increased via the `-c` option, or the
statemap can be regenerated to cover a smaller range of time with some
combination of the `-b` option (to denote a beginning time) and the `-d`
option (to denote a duration).  The number of coalesced states can be
determined by looking at the metadata placed at the end of of the output
SVG.

### Options

The `statemap` command has the following options:

- `-b` (`--begin`): Takes a time offset at which the statemap should begin.
The time offset may be expressed in floating point with an optional
suffix (e.g., `-b 12.719s`).

- `-c` (`--coalesce`): Specifies the coalescing factor. Higher numbers will
result in less coalescence.

- `-d` (`--duration`): Takes a duration time for the statemap.  The time
may be expressed in floating point with an optional suffix (e.g.,
`-d 491.2ms`).

- `-h` (`--state-height`): The height (in pixels) of each state in the
statemap.

- `-i` (`--ignore-tags`): Ignore tags in the input, acting as if each state
is untagged. (This will result in shorter run-time and a smaller resulting
SVG.)

- `-s` (`--sortby`): The state by which to sort (default is to sort by
entity).

## Interaction

A statemap has icons for zooming and panning.  As the statemap is zoomed,
the time labels on top of the X axis will be updatd to reflect the current
duration.

Clicking on a statemap will highlight both the time at the point of the
click as well as the state.  Zooming when a time is selected will center
the zoomed statemap at the specified time.  To clear the time, click on
the time label above the statemap; to select another time, simply click
on the statemap.

