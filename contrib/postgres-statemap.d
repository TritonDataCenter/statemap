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
	STATE_OFF_CPU_IO = 2,
	STATE_OFF_CPU_SEMOP = 3,
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
	printf("\t\"title\": \"PostgreSQL statemap on %s, by process ID\",\n",
	    `utsname.nodename);
	printf("\t\"host\": \"%s\",\n", `utsname.nodename);
	printf("\t\"states\": {\n");

	STATE_METADATA(STATE_ON_CPU, "on-cpu", "#DAF7A6")
	STATE_METADATA(STATE_OFF_CPU_WAITING, "off-cpu-waiting", "#f9f9f9")
	STATE_METADATA(STATE_OFF_CPU_BLOCKED, "off-cpu-blocked", "#C70039")
	STATE_METADATA(STATE_OFF_CPU_SEMOP, "off-cpu-semop", "#FF5733")
	STATE_METADATA(STATE_OFF_CPU_IO, "off-cpu-io", "#FFC300")
	STATE_METADATA(STATE_OFF_CPU_DEAD, "off-cpu-dead", "#581845")

	printf("\"data\": [\n");
	start = timestamp;
	exit(0);
}

sched:::wakeup
/execname == "postgres" && args[1]->pr_fname == "postgres"/
{
	printf("{ \"time\": \"%d\", \"entity\": \"%d\", ",
	    timestamp - start, pid);
	printf("\"event\": \"wakeup\", \"target\": \"%d\" },\n",
	    args[1]->pr_pid);
}

zio_wait:entry
/execname == "postgres"/
{
	self->state = STATE_OFF_CPU_IO;
}

zio_wait:return
/execname == "postgres"/
{
	self->state = STATE_ON_CPU;
}

syscall::semop:entry
/execname == "postgres"/
{
	self->state = STATE_OFF_CPU_SEMOP;
}

syscall::semop:return
/execname == "postgres"/
{
	self->state = STATE_ON_CPU;
}

sched:::off-cpu
/execname == "postgres"/
{
	printf("{ \"time\": \"%d\", \"entity\": \"%d\", ",
	    timestamp - start, tid);

	printf("\"state\": %d },\n", self->state != STATE_ON_CPU ?
	    self->state : curthread->t_flag & T_WAKEABLE ?
	    STATE_OFF_CPU_WAITING : STATE_OFF_CPU_BLOCKED);
}

sched:::on-cpu
/execname == "postgres"/
{
	self->state = STATE_ON_CPU;
	printf("{ \"time\": \"%d\", \"entity\": \"%d\", ",
	    timestamp - start, tid);
	printf("\"state\": %d },\n", self->state);
}

tick-1sec
/timestamp - start > 60 * 1000000000/
{
	exit(0);
}

END
{
	printf("{} ] }\n");
}
