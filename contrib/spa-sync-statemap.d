#!/usr/sbin/dtrace -Cs 

/*
 * Copyright 2018, Joyent, Inc.
 */

#pragma D option quiet

typedef enum {
	STATE_ON_CPU = 0,
	STATE_OFF_CPU_WAITING,
	STATE_OFF_CPU_BLOCKED,
	STATE_OFF_CPU_ZIO_WAIT,
	STATE_OFF_CPU_ZIO_WAIT_MOS,
	STATE_OFF_CPU_ZIO_WAIT_SYNC,
	STATE_OFF_CPU_OBJSET_SYNC,
	STATE_OFF_CPU_CV,
	STATE_OFF_CPU_PREEMPTED,
	STATE_MAX
} state_t;

#define STATE_METADATA(_state, _str, _color) \
	printf("\t\t\"%s\": {\"value\": %d, \"color\": \"%s\" }%s\n", \
	    _str, _state, _color, _state < STATE_MAX - 1 ? "," : "");

BEGIN
{
	wall = walltimestamp;
	printf("{\n\t\"start\": [ %d, %d ],\n",
	    wall / 1000000000, wall % 1000000000);
	printf("\t\"title\": \"SPA sync\",\n");
	printf("\t\"host\": \"%s\",\n", `utsname.nodename);
	printf("\t\"entityKind\": \"SPA sync thread for pool\",\n");
	printf("\t\"states\": {\n");

	STATE_METADATA(STATE_ON_CPU, "on-cpu", "#DAF7A6")
	STATE_METADATA(STATE_OFF_CPU_WAITING, "off-cpu-waiting", "#f9f9f9")
	STATE_METADATA(STATE_OFF_CPU_BLOCKED, "off-cpu-blocked", "#C70039")
	STATE_METADATA(STATE_OFF_CPU_ZIO_WAIT, "off-cpu-zio-wait", "#FFC300")
	STATE_METADATA(STATE_OFF_CPU_ZIO_WAIT_MOS,
	    "off-cpu-zio-wait-mos", "#FF5733")
	STATE_METADATA(STATE_OFF_CPU_ZIO_WAIT_SYNC,
	    "off-cpu-zio-wait-sync", "#BB8FCE")
	STATE_METADATA(STATE_OFF_CPU_OBJSET_SYNC,
	    "off-cpu-objset-sync", "#338AFF")
	STATE_METADATA(STATE_OFF_CPU_CV, "off-cpu-cv", "#66FFCC")
	STATE_METADATA(STATE_OFF_CPU_PREEMPTED, "off-cpu-preempted", "#CCFF00")

	printf("\t}\n}\n");
	start = timestamp;
}

fbt::spa_sync:return
{
	self->state = STATE_OFF_CPU_WAITING;
}

fbt::spa_sync:entry
{
	self->spa = args[0];
	self->state = STATE_ON_CPU;
}

fbt::zio_wait:entry
/self->spa != NULL && self->state == STATE_ON_CPU/
{
	self->state = STATE_OFF_CPU_ZIO_WAIT;
	
}

fbt::zio_wait:return
/self->state == STATE_OFF_CPU_ZIO_WAIT/
{
	self->state = STATE_ON_CPU;
}

fbt::vdev_config_sync:entry
/self->state == STATE_ON_CPU/
{
	self->state = STATE_OFF_CPU_ZIO_WAIT_SYNC;
}

fbt::vdev_config_sync:return
/self->state == STATE_OFF_CPU_ZIO_WAIT_SYNC/
{
	self->state = STATE_ON_CPU;
}

fbt::dsl_pool_sync_mos:entry
/self->state == STATE_ON_CPU/
{
	self->state = STATE_OFF_CPU_ZIO_WAIT_MOS;
}

fbt::dsl_pool_sync_mos:return
/self->state == STATE_OFF_CPU_ZIO_WAIT_MOS/
{
	self->state = STATE_ON_CPU;
}

fbt::dmu_objset_sync:entry
/self->state == STATE_ON_CPU/
{
	self->state = STATE_OFF_CPU_OBJSET_SYNC;
}

fbt::dmu_objset_sync:return
/self->state == STATE_OFF_CPU_OBJSET_SYNC/
{
	self->state = STATE_ON_CPU;
}

sched:::off-cpu
/self->spa != NULL/
{
	printf("{ \"time\": \"%d\", \"entity\": \"%s\", ",
	    timestamp - start, self->spa->spa_name);

	printf("\"state\": %d }\n",
	    self->state != STATE_ON_CPU ? self->state : 
	    curthread->t_sobj_ops == NULL ? STATE_OFF_CPU_PREEMPTED :
	    curthread->t_sobj_ops == &`cv_sobj_ops ? STATE_OFF_CPU_CV :
	    STATE_OFF_CPU_BLOCKED);
}

sched:::on-cpu
/self->spa != NULL/
{
	printf("{ \"time\": \"%d\", \"entity\": \"%s\", ",
	    timestamp - start, self->spa->spa_name);
	printf("\"state\": %d }\n", STATE_ON_CPU);
}

tick-1sec
/timestamp - start > 300 * 1000000000/
{
	exit(0);
}

