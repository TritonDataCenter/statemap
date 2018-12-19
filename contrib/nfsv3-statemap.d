#!/usr/sbin/dtrace -Cs

/*
 * Copyright 2018, Joyent, Inc.
 * Copyright (c) 2018 by Delphix. All rights reserved.
 */

#pragma D option quiet
#pragma D option destructive

typedef enum {
	STATE_NONE = 0,
	STATE_READ,
	STATE_WRITE,
	STATE_RW,
	STATE_MAX
} state_t;

state_t state;

#define STATE_METADATA(_state, _str, _color) \
	printf("\t\t\"%s\": {\"value\": %d, \"color\": \"%s\" }%s\n", \
	    _str, _state, _color, _state < STATE_MAX - 1 ? "," : "");

BEGIN
{
	reads = 0;
	writes = 0;

	wall = walltimestamp;
	printf("{\n\t\"start\": [ %d, %d ],\n",
	    wall / 1000000000, wall % 1000000000);

	printf("\t\"title\": \"nfsv3 I/O\",\n");
	printf("\t\"host\": \"%s\",\n", `utsname.nodename);
	printf("\t\"states\": {\n");

	STATE_METADATA(STATE_NONE, "nfsv3 idle", "#e0e0e0");
	STATE_METADATA(STATE_READ, "nfsv3 reading", "#FFC300");
	STATE_METADATA(STATE_WRITE, "nfsv3 writing", "#FF5733");
	STATE_METADATA(STATE_RW, "nfsv3 reading+writing", "#C70039");

	printf("\t}\n}\n");
	start = timestamp;
}

nfsv3:::op-read-start
{
	reads++;
	this->prio = STATE_READ;
}

nfsv3:::op-read-done
{
	reads--;
	this->prio = STATE_READ;
}

nfsv3:::op-write-start
{
	writes++;
	this->prio = STATE_WRITE;
}

nfsv3:::op-write-done
{
	writes--;
	this->prio = STATE_WRITE;
}

nfsv3:::op-read-start,
nfsv3:::op-write-start
{
	this->state = state;
	this->next = this->state != STATE_NONE ? this->state : this->prio;
}

nfsv3:::op-read-start,
nfsv3:::op-write-start
/(this->state == STATE_READ && this->prio == STATE_WRITE) ||
    (this->state == STATE_WRITE && this->prio == STATE_READ)/
{
	this->next = STATE_RW;
}

nfsv3:::op-read-done,
nfsv3:::op-write-done
{
	this->next = reads > 0 ?
	    (writes > 0 ? STATE_RW : STATE_READ) :
	    (writes > 0 ? STATE_WRITE : STATE_NONE);
}

nfsv3:::op-read-start,
nfsv3:::op-read-done,
nfsv3:::op-write-start,
nfsv3:::op-write-done
/this->state != this->next/
{
	printf("{ \"time\": \"%d\", \"entity\": \"%s\", \"state\": %d }\n",
	    timestamp - start, `utsname.nodename, this->next);
	state = this->next;
}

tick-1sec
/timestamp - start > 300 * 1000000000/
{
	exit(0);
}
