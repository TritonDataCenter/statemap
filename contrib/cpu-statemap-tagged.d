#!/usr/sbin/dtrace -Cs

/*
 * Copyright 2018, Joyent, Inc.
 */

#pragma D option quiet
#pragma D option destructive
#pragma D option switchrate=500hz

#define STATE_UTHREAD	0
#define STATE_KTHREAD	1
#define	STATE_INTR	1
#define STATE_IDLE	17

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
	printf("\t\"entityKind\": \"CPU\",\n");
	printf("\t\"states\": {\n");

	/*
	 * Execution: shades of green
	 */
	STATE_METADATA(STATE_UTHREAD, "uthread", "#9BC362")
	STATE_METADATA(STATE_KTHREAD, "kthread", "#2E4E00")

	/*
	 * Low level interrupts: shades of aqua and then blue
	 */
	STATE_METADATA(STATE_INTR + 1, "level-1", "#689D99")
	STATE_METADATA(STATE_INTR + 2, "level-2", "#41837E")
	STATE_METADATA(STATE_INTR + 3, "level-3", "#236863")
	STATE_METADATA(STATE_INTR + 4, "level-4", "#0D4E4A")
	STATE_METADATA(STATE_INTR + 5, "level-5", "#003430")
	STATE_METADATA(STATE_INTR + 6, "level-6", "#817FB2")
	STATE_METADATA(STATE_INTR + 7, "level-7", "#575594")
	STATE_METADATA(STATE_INTR + 8, "level-8", "#363377")
	STATE_METADATA(STATE_INTR + 9, "level-9", "#1C1A59")
	STATE_METADATA(STATE_INTR + 10, "level-10", "#0A093B")

	/*
	 * High level interrupts: shades of red
	 */
	STATE_METADATA(STATE_INTR + 11, "level-11", "#FFAAAA")
	STATE_METADATA(STATE_INTR + 12, "level-12", "#D46A6A")
	STATE_METADATA(STATE_INTR + 13, "level-13", "#AA3939")
	STATE_METADATA(STATE_INTR + 14, "level-14", "#801515")
	STATE_METADATA(STATE_INTR + 15, "level-15", "#550000")

	STATE_METADATA(STATE_IDLE, "idle", "#e0e0e0")

	printf("\t}\n}\n");
	start = timestamp;
}

interrupt-start
/arg0 != NULL && !itagged[arg0]/
{
	itagged[arg0] = 1;

	this->pri = curthread->t_cpu->cpu_m.mcpu_pri;
	this->devi = (struct dev_info *)arg0;

	printf("{ \"state\": %d, \"tag\": \"%p\", ",
	    this->pri + STATE_INTR, arg0);
	printf("\"driver\": \"%s\", \"instance\": %d }\n",
	    stringof(`devnamesp[this->devi->devi_major].dn_name),
	    this->devi->devi_instance);
}

interrupt-start
/arg0 == NULL/
{
	this->pri = curthread->t_cpu->cpu_m.mcpu_pri;

	printf("{ \"time\": \"%d\", \"entity\": \"%d\", \"state\": %d }\n",
	    timestamp - start,
	    curthread->t_cpu->cpu_id, this->pri + STATE_INTR);
}

interrupt-start
/arg0 != NULL/
{
	this->pri = curthread->t_cpu->cpu_m.mcpu_pri;

	printf("{ \"time\": \"%d\", \"entity\": \"%d\", \"state\": %d, \"tag\": \"%p\" }\n",
	    timestamp - start,
	    curthread->t_cpu->cpu_id, this->pri + STATE_INTR, arg0);

	/*
	 * We set this, but we don't bother to ever clear it:  the number of
	 * CPUs and number of interrupt levels are both finite and small.
	 */
	itag[curthread->t_cpu, this->pri] = arg0;
}

av_dispatch_softvect:entry
/!itagged[curthread->t_pil]/
{
	itagged[curthread->t_pil] = 1;

	printf("{ \"state\": %d, \"tag\": \"%p\", ", curthread->t_pil,
	    curthread->t_pil + STATE_INTR);
	printf("\"driver\": \"softint\", \"instance\": 0 }\n");
}

av_dispatch_softvect:entry
{
	printf("{ \"time\": \"%d\", \"entity\": \"%d\", \"state\": %d, \"tag\": \"%p\" }\n",
	    timestamp - start,
	    curthread->t_cpu->cpu_id, curthread->t_pil + STATE_INTR,
	    curthread->t_pil);

	itag[curthread->t_cpu, curthread->t_pil] = curthread->t_pil;
}

interrupt-complete,
av_dispatch_softvect:return
{
	this->intr = curthread->t_intr;
	this->pri = curthread->t_cpu->cpu_m.mcpu_pri;
	this->idle = 0;
}

interrupt-complete,
av_dispatch_softvect:return
/(this->pri > 10 && curthread == curthread->t_cpu->cpu_idle_thread) ||
    (this->pri <= 10 && (this->intr == NULL ||
    this->intr == curthread->t_cpu->cpu_idle_thread))/
{
	printf("{ \"time\": \"%d\", \"entity\": \"%d\", \"state\": %d }\n",
	    timestamp - start,
	    curthread->t_cpu->cpu_id, STATE_IDLE);
	this->idle = 1;
}

interrupt-complete,
av_dispatch_softvect:return
/!this->idle/
{
	/*
	 * We want to set the state (and tag) back to the thread that we're
	 * going to return to, which we do imperfectly in that we don't
	 * reflect high-level interrupts interrupting high-level interrupts
	 * (we will show this as the underlying thread executing).
	 */
	printf("{ \"time\": \"%d\", \"entity\": \"%d\", \"state\": %d, \"tag\": \"%p\" }\n",
	    timestamp - start, curthread->t_cpu->cpu_id, 
	    this->pri > 10 ?
	    (curthread->t_pil > 0 ? curthread->t_pil + STATE_INTR :
	    curthread->t_procp == &`p0 ? STATE_KTHREAD : STATE_UTHREAD) :
	    (this->intr->t_pil > 0 ? this->intr->t_pil + STATE_INTR :
	    this->intr->t_procp == &`p0 ? STATE_KTHREAD : STATE_UTHREAD),
	    this->pri > 10 ? 
	    (curthread->t_pil > 0 ? itag[curthread->t_cpu, curthread->t_pil] :
	    curthread->t_did) : 
	    (this->intr->t_pil > 0 ? itag[curthread->t_cpu, this->intr->t_pil] :
	    this->intr->t_did));
}

sched:::on-cpu
/curthread != curthread->t_cpu->cpu_idle_thread &&
    pid == 0 && !tagged[curthread->t_did]/
{
	tagged[curthread->t_did] = 1;

	printf("{ \"state\": %d, \"tag\": \"%p\", \"thread\": \"%a\", \"taskq\": \"%s\" }\n",
	    STATE_KTHREAD,
	    curthread->t_did, curthread->t_startpc,
	    curthread->t_taskq != NULL ?
	    stringof(((taskq_t *)curthread->t_taskq)->tq_name) : "<none>");
}

sched:::on-cpu
/curthread != curthread->t_cpu->cpu_idle_thread && pid != 0 &&
    !tagged[curthread->t_did]/
{
	tagged[curthread->t_did] = 1;

	/*
	 * The godforsaken strtok() mess is to deal with pr_psargs that
	 * contain an embedded quote, backslashing that quote to assure that
	 * we generate valid JSON.  Yes, it would be (MUCH!) easier if DTrace
	 * provided a subroutine to do this, and this is imperfect in that (1)
	 * it will eat backslashes and turn them into backslashed quotes, even
	 * if that's not correct and (2) it will elide the string with an
	 * ellipsis after the third quote or backslash -- but at least it will
	 * always yield parseable JSON!
	 */
	printf("{ \"state\": %d, \"tag\": \"%p\", \"pid\": \"%d\", \"tid\": \"%d\", \"execname\": \"%s\", \"psargs\": \"%s\" }\n",
	    STATE_UTHREAD, curthread->t_did, pid, tid,
	    execname, 
            strchr(curpsinfo->pr_psargs, '"') == NULL ? curpsinfo->pr_psargs :
            strjoin(strtok(curpsinfo->pr_psargs, "\"\\"),
            (this->s = strtok(NULL, "\"\\")) == NULL ? "" :
            strjoin(strjoin("\\\"", this->s),
            (this->s = strtok(NULL, "\"\\")) == NULL ? "" :
            strjoin(strjoin("\\\"", this->s),
            (this->s = strtok(NULL, "\"\\")) == NULL ? "" :
            strjoin(strjoin("\\\"", this->s),
            (this->s = strtok(NULL, "\"\\")) == NULL ? "" :
            strjoin("\\\"", this->s))))));
}

sched:::on-cpu
/curthread != curthread->t_cpu->cpu_idle_thread/
{
	printf("{ \"time\": \"%d\", \"entity\": \"%d\", \"state\": %d, \"tag\": \"%p\" }\n",
	    timestamp - start,
	    curthread->t_cpu->cpu_id,
	    (curthread->t_flag & T_INTR) ? curthread->t_pil + STATE_INTR :
	    (curthread->t_procp == &`p0 ? STATE_KTHREAD : STATE_UTHREAD),
	    (curthread->t_flag & T_INTR) ?
	    itag[curthread->t_cpu, curthread->t_pil] :
	    curthread->t_did);
}

sched:::on-cpu
/curthread == curthread->t_cpu->cpu_idle_thread/
{
	printf("{ \"time\": \"%d\", \"entity\": \"%d\", \"state\": %d }\n",
	    timestamp - start,
	    curthread->t_cpu->cpu_id, STATE_IDLE);
}

tick-1sec
/timestamp - start > 10 * 1000000000/
{
	exit(0);
}

