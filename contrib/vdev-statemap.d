#!/usr/sbin/dtrace -Cs

/*
 * Copyright 2018, Joyent, Inc.
 */

#pragma D option quiet
#pragma D option destructive

typedef enum zio_priority {
	ZIO_PRIORITY_SYNC_READ,
	ZIO_PRIORITY_SYNC_WRITE,        /* ZIL */
	ZIO_PRIORITY_ASYNC_READ,        /* prefetch */
	ZIO_PRIORITY_ASYNC_WRITE,       /* spa_sync() */
	ZIO_PRIORITY_SCRUB,             /* asynchronous scrub/resilver reads */
	ZIO_PRIORITY_REMOVAL,           /* reads/writes for vdev removal */
	ZIO_PRIORITY_INITIALIZING,      /* initializing I/O */
	ZIO_PRIORITY_NUM_QUEUEABLE
} zio_priority_t;

typedef enum {
	STATE_NONE = 0,
	STATE_READ,
	STATE_WRITE,
	STATE_RW,
	STATE_MAX
} state_t;

state_t state[vdev_queue_t *];

#define STATE_METADATA(_state, _str, _color) \
	printf("\t\t\"%s\": {\"value\": %d, \"color\": \"%s\" }%s\n", \
	    _str, _state, _color, _state < STATE_MAX - 1 ? "," : "");

BEGIN
{
	wall = walltimestamp;
	printf("{\n\t\"start\": [ %d, %d ],\n",
	    wall / 1000000000, wall % 1000000000);
	
	printf("\t\"title\": \"vdev I/O\",\n");
	printf("\t\"host\": \"%s\",\n", `utsname.nodename);
	printf("\t\"states\": {\n");

	STATE_METADATA(STATE_NONE, "idle", "#e0e0e0");
	STATE_METADATA(STATE_READ, "reading", "#FFC300");
	STATE_METADATA(STATE_WRITE, "writing", "#FF5733");
	STATE_METADATA(STATE_RW, "reading+writing", "#C70039");

	printf("\t}\n}\n");
	start = timestamp;
}

vdev_queue_pending_add:entry
{
	this->prio = (args[1]->io_priority == ZIO_PRIORITY_SYNC_WRITE ||
	    args[1]->io_priority == ZIO_PRIORITY_ASYNC_WRITE) ?
	    STATE_WRITE : STATE_READ;

	this->state = state[args[0]];
	this->next = this->state != STATE_NONE ? this->state : this->prio;
}

vdev_queue_pending_add:entry
/(this->state == STATE_READ && this->prio == STATE_WRITE) ||
    (this->state == STATE_WRITE && this->prio == STATE_READ)/
{
	this->next = STATE_RW;
}

vdev_queue_pending_remove:entry
{
	this->prio = (args[1]->io_priority == ZIO_PRIORITY_SYNC_WRITE ||
	    args[1]->io_priority == ZIO_PRIORITY_ASYNC_WRITE) ?
	    STATE_WRITE : STATE_READ;

	this->reads = args[0]->vq_class[ZIO_PRIORITY_ASYNC_READ].vqc_active +
	    args[0]->vq_class[ZIO_PRIORITY_SYNC_READ].vqc_active -
	    (this->prio == STATE_READ ? 1 : 0);

	this->writes = args[0]->vq_class[ZIO_PRIORITY_ASYNC_WRITE].vqc_active +
	    args[0]->vq_class[ZIO_PRIORITY_SYNC_WRITE].vqc_active -
	    (this->prio == STATE_WRITE ? 1 : 0);

	this->state = state[args[0]];
	this->next = this->reads > 0 ?
	    (this->writes > 0 ? STATE_RW : STATE_READ) :
	    (this->writes > 0 ? STATE_WRITE : STATE_NONE);
}

vdev_queue_pending_add:entry,
vdev_queue_pending_remove:entry
/this->state != this->next/
{
	this->q = (vdev_queue_t *)arg0;
	printf("{ \"time\": \"%d\", \"entity\": \"%s\", \"state\": %d }\n",
	    timestamp - start,
	    basename(this->q->vq_vdev->vdev_path), this->next);

	state[this->q] = this->next;
}

tick-1sec
/timestamp - start > 300 * 1000000000/
{
	exit(0);
}
