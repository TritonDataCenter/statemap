#!/usr/sbin/dtrace -Cs

/*
 * Copyright 2018, Joyent, Inc.
 */

#pragma D option quiet
#pragma D option destructive
#pragma D option switchrate=100hz

#define STATE_IDLE	0
#define STATE_UTHREAD	16
#define STATE_KTHREAD	17

#define T_INTR	1
inline int STATE_MAX = 17;

#define STATE_METADATA(_state, _str, _color) \
	printf("\t\t\"%s\": {\"value\": %d, \"color\": \"%s\" }%s\n", \
	    _str, _state, _color, _state < STATE_MAX ? "," : "");

BEGIN
{
	wall = walltimestamp;
	printf("{\n\t\"start\": [ %d, %d ],\n",
	    wall / 1000000000, wall % 1000000000);
	printf("\t\"title\": \"Statemap for CPU activity on %s\",\n",
	    `utsname.nodename);
	printf("\t\"host\": \"%s\",\n", `utsname.nodename);
	printf("\t\"states\": {\n");

	STATE_METADATA(STATE_IDLE, "idle", "#e0e0e0")

	/*
	 * Low level interrupts: shades of aqua and then blue
	 */
	STATE_METADATA(1, "level-1", "#689D99")
	STATE_METADATA(2, "level-2", "#41837E")
	STATE_METADATA(3, "level-3", "#236863")
	STATE_METADATA(4, "level-4", "#0D4E4A")
	STATE_METADATA(5, "level-5", "#003430")
	STATE_METADATA(6, "level-6", "#817FB2")
	STATE_METADATA(7, "level-7", "#575594")
	STATE_METADATA(8, "level-8", "#363377")
	STATE_METADATA(9, "level-9", "#1C1A59")
	STATE_METADATA(10, "level-10", "#0A093B")

	/*
	 * High level interrupts: shades of red
	 */
	STATE_METADATA(11, "level-11", "#FFAAAA")
	STATE_METADATA(12, "level-12", "#D46A6A")
	STATE_METADATA(13, "level-13", "#AA3939")
	STATE_METADATA(14, "level-14", "#801515")
	STATE_METADATA(15, "level-15", "#550000")

	/*
	 * Execution: shades of green
	 */
	STATE_METADATA(STATE_UTHREAD, "uthread", "#9BC362")
	STATE_METADATA(STATE_KTHREAD, "kthread", "#2E4E00")

	printf("\t}\n}\n");
	start = timestamp;
}

interrupt-start
{
	this->pri = curthread->t_cpu->cpu_m.mcpu_pri;

	printf("{ \"time\": \"%d\", \"entity\": \"%d\", \"state\": %d }\n",
	    timestamp - start,
	    curthread->t_cpu->cpu_id, this->pri);
}

interrupt-complete
{
	this->intr = curthread->t_intr;
	this->pri = curthread->t_cpu->cpu_m.mcpu_pri;

	/*
	 * This is a bit gnarly: we need to set the state to be back to
	 * what it was before the interrupt took place.  This is slightly
	 * imperfect in that it doesn't quite reflect high-level interrupts
	 * interrupting high-level interrupts, but that should be an unusual
	 * enough condition that this should be good enough for most purposes.
	 */
	printf("{ \"time\": \"%d\", \"entity\": \"%d\", \"state\": %d }\n",
	    timestamp - start,
	    curthread->t_cpu->cpu_id,
	    this->pri > 10 ? 
	    (curthread == curthread->t_cpu->cpu_idle_thread ? STATE_IDLE :
	    curthread->t_pil > 0 ? curthread->t_pil :
	    curthread->t_procp == &`p0 ? STATE_KTHREAD : STATE_UTHREAD) :
	    this->intr != NULL ?
	    (this->intr == curthread->t_cpu->cpu_idle_thread ? STATE_IDLE :
	    this->intr->t_pil > 0 ? this->intr->t_pil :
	    this->intr->t_procp == &`p0 ? STATE_KTHREAD : STATE_UTHREAD) :
	    STATE_IDLE);
}

sched:::on-cpu
{
	printf("{ \"time\": \"%d\", \"entity\": \"%d\", \"state\": %d }\n",
	    timestamp - start,
	    curthread->t_cpu->cpu_id,
	    curthread == curthread->t_cpu->cpu_idle_thread ? STATE_IDLE :
	    (curthread->t_flag & T_INTR) ? curthread->t_pil :
	    curthread->t_procp == &`p0 ? STATE_KTHREAD :
	    STATE_UTHREAD);
}

tick-1sec
/timestamp - start > 10 * 1000000000/
{
	exit(0);
}

