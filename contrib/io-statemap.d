#!/usr/sbin/dtrace -Cs

/*
 * Copyright 2018, Joyent, Inc.
 */

#pragma D option quiet
#pragma D option destructive

inline int STATE_MAXIO = 20;

#define STATE_METADATA(_state, _str, _color) \
	printf("\t\t\"%s\": {\"value\": %d, \"color\": \"%s\" }%s\n", \
	    _str, _state, _color, _state < STATE_MAXIO ? "," : "");

BEGIN
{
	wall = walltimestamp;
	printf("{\n\t\"start\": [ %d, %d ],\n",
	    wall / 1000000000, wall % 1000000000);
	printf("\t\"title\": \"disk I/O\",\n");
	printf("\t\"host\": \"%s\",\n", `utsname.nodename);
	printf("\t\"states\": {\n");

	STATE_METADATA(0, "no I/O", "#e0e0e0")
	STATE_METADATA(1, "1 I/O", "#DFE500");
	STATE_METADATA(2, "2 I/Os", "#DDD800");
	STATE_METADATA(3, "3 I/Os", "#DBCC01");
	STATE_METADATA(4, "4 I/Os", "#D9C002");
	STATE_METADATA(5, "5 I/Os", "#D8B403");
	STATE_METADATA(6, "6 I/Os", "#D6A804");
	STATE_METADATA(7, "7 I/Os", "#D49C05");
	STATE_METADATA(8, "8 I/Os", "#D39006");
	STATE_METADATA(9, "9 I/Os", "#D18407");
	STATE_METADATA(10, "10 I/Os", "#CF7808");
	STATE_METADATA(11, "11 I/Os", "#CE6C09");
	STATE_METADATA(12, "12 I/Os", "#CC600A");
	STATE_METADATA(13, "13 I/Os", "#CA540B");
	STATE_METADATA(14, "14 I/Os", "#C9480C");
	STATE_METADATA(15, "15 I/Os", "#C73C0D");
	STATE_METADATA(16, "16 I/Os", "#C5300E");
	STATE_METADATA(17, "17 I/Os", "#C4240F");
	STATE_METADATA(18, "18 I/Os", "#C21810");
	STATE_METADATA(19, "19 I/Os", "#C00C11");
	STATE_METADATA(STATE_MAXIO, ">=20 I/Os", "#BF0012");

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
