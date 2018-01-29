#!/usr/sbin/dtrace -Cs

/*
 * Copyright 2018, Joyent, Inc.
 */

#pragma D option quiet
#pragma D option destructive

inline int STATE_MAXIO = 9;

#define STATE_METADATA(_state, _str, _color) \
	printf("\t\t\"%s\": {\"value\": %d, \"color\": \"%s\" }%s\n", \
	    _str, _state, _color, _state < STATE_MAXIO ? "," : "");

BEGIN
{
	wall = walltimestamp;
	printf("{\n\t\"start\": [ %d, %d ],\n",
	    wall / 1000000000, wall % 1000000000);
	printf("\t\"title\": \"Statemap for device I/O on %s\",\n",
	    `utsname.nodename);
	printf("\t\"host\": \"%s\",\n", `utsname.nodename);
	printf("\t\"states\": {\n");

	STATE_METADATA(0, "no I/O", "#e0e0e0")
	STATE_METADATA(1, "1 I/O", "#ffffcc");
	STATE_METADATA(2, "2 I/Os", "#ffeda0");
	STATE_METADATA(3, "3 I/Os", "#fed976");
	STATE_METADATA(4, "4 I/Os", "#feb24c");
	STATE_METADATA(5, "5 I/Os", "#fd8d3c");
	STATE_METADATA(6, "6 I/Os", "#fc4e2a");
	STATE_METADATA(7, "7 I/Os", "#e31a1c");
	STATE_METADATA(8, "8 I/Os", "#bd0026");
	STATE_METADATA(STATE_MAXIO, ">8 I/Os", "#800026");

	printf("\t}\n}\n");
	start = timestamp;
}

scsi-transport-dispatch
{
	this->b = (struct buf *)arg0;
	this->u = ((struct sd_xbuf *)this->b->b_private)->xb_un;

	printf("{ \"time\": \"%d\", \"entity\": \"sd%d\", \"state\": %d }\n",
	    timestamp - start,
	    ((struct dev_info *)this->u->un_sd->sd_dev)->devi_instance,
	    this->u->un_ncmds_in_transport < STATE_MAXIO ?
	    this->u->un_ncmds_in_transport : STATE_MAXIO);
}

sdintr:entry
{
	this->b = (struct buf *)args[0]->pkt_private;
	self->un = ((struct sd_xbuf *)this->b->b_private)->xb_un;
}

sdintr:return
/(this->u = self->un) != NULL/
{
	printf("{ \"time\": \"%d\", \"entity\": \"sd%d\", \"state\": %d }\n",
	    timestamp - start,
	    ((struct dev_info *)this->u->un_sd->sd_dev)->devi_instance,
	    this->u->un_ncmds_in_transport < STATE_MAXIO ?
	    this->u->un_ncmds_in_transport : STATE_MAXIO);

	self->un = NULL;
}

tick-1sec
/timestamp - start > 10 * 1000000000/
{
	exit(0);
}
