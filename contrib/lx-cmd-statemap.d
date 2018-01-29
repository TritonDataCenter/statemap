#!/usr/sbin/dtrace -Cs 

/*
 * Copyright 2017, Joyent, Inc.
 */

#pragma D option quiet
#pragma D option destructive

#define T_WAKEABLE	0x0002

typedef enum {
	STATE_ON_CPU = 0,
	STATE_OFF_CPU_WAITING = 1,
	STATE_OFF_CPU_FUTEX = 2,
	STATE_OFF_CPU_IO = 3,
	STATE_OFF_CPU_BLOCKED = 4,
	STATE_OFF_CPU_DEAD = 5,
	STATE_MAX = 6
} state_t;

#define STATE_METADATA(_state, _str, _color) \
	printf("\t\t\"%s\": {\"value\": %d, \"color\": \"%s\" }%s\n", \
	    _str, _state, _color, _state < STATE_MAX - 1 ? "," : "");

BEGIN
{
	wall = walltimestamp;
	printf("{\n\t\"start\": [ %d, %d ],\n",
	    wall / 1000000000, wall % 1000000000);
	printf("\t\"title\": \"Statemap for all %s LX processes on %s\",\n",
	    $$1, `utsname.nodename);
	printf("\t\"host\": \"%s\",\n", `utsname.nodename);
	printf("\t\"states\": {\n");

	STATE_METADATA(STATE_ON_CPU, "on-cpu", "#DAF7A6")
	STATE_METADATA(STATE_OFF_CPU_WAITING, "off-cpu-waiting", "#f9f9f9")
	STATE_METADATA(STATE_OFF_CPU_FUTEX, "off-cpu-futex", "#f0f0f0")
	STATE_METADATA(STATE_OFF_CPU_IO, "off-cpu-io", "#FFC300")
	STATE_METADATA(STATE_OFF_CPU_BLOCKED, "off-cpu-blocked", "#C70039")
	STATE_METADATA(STATE_OFF_CPU_DEAD, "off-cpu-dead", "#581845")

	printf("\t}\n}\n");
	start = timestamp;
}

zfs_fillpage:entry
/execname == $$1/
{
	self->state = STATE_OFF_CPU_IO;
}

zfs_fillpage:return
/execname == $$1/
{
	self->state = STATE_ON_CPU;
}

lx_futex:entry
/execname == $$1/
{
	self->state = STATE_OFF_CPU_FUTEX;
}

lx_futex:return
/execname == $$1/
{
	self->state = STATE_ON_CPU;
}

sched:::off-cpu
/execname == $$1/
{
	printf("{ \"time\": \"%d\", \"entity\": \"%d/%d\", ",
	    timestamp - start, pid, tid);

	printf("\"state\": %d }\n", self->state != STATE_ON_CPU ?
	    self->state : curthread->t_flag & T_WAKEABLE ?
	    STATE_OFF_CPU_WAITING : STATE_OFF_CPU_BLOCKED);
}

sched:::on-cpu
/execname == $$1/
{
	self->state = STATE_ON_CPU;
	printf("{ \"time\": \"%d\", \"entity\": \"%d/%d\", ",
	    timestamp - start, pid, tid);
	printf("\"state\": %d }\n", self->state);
}

proc:::lwp-exit
/execname == $$1/
{
	printf("{ \"time\": \"%d\", \"entity\": \"%d/%d\", ",
	    timestamp - start, pid, tid);
	printf("\"state\": %d }\n", STATE_OFF_CPU_DEAD);
}

tick-1sec
/timestamp - start > 60 * 1000000000/
{
	exit(0);
}
