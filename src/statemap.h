/*
 * Copyright 2018 Joyent, Inc.
 */

#ifndef _SYS_STATEMAP_H
#define	_SYS_STATEMAP_H

#ifdef  __cplusplus
extern "C" {
#endif

#include "./avl/avl.h"

#define	STATEMAP_METADATA_MAX	(16 * 1024)

#define	STATEMAP_METADATA_STATES	"states"
#define	STATEMAP_METADATA_STATESVALUE	"value"

#define	STATEMAP_DATA_ENTITY		"entity"
#define	STATEMAP_DATA_TIME		"time"
#define	STATEMAP_DATA_STATE		"state"
#define	STATEMAP_DATA_TAG		"tag"
#define	STATEMAP_DATA_EVENT		"event"
#define	STATEMAP_DATA_DESCRIPTION	"description"

#define	STATEMAP_ENTITY_HASHSIZE	8192
#define	STATEMAP_TAGDEF_HASHSIZE	8192

#define	STATEMAP_CONFIG_MAXRECT		25000

typedef struct statemap_config {
	int64_t smc_maxrect;			/* maximum # of rects */
	int64_t smc_begin;			/* offset to begin, if any */
	int64_t smc_end;			/* offset to end, if any */
	int smc_notags;				/* boolean: ignore tags */
	int smc_dryrun;				/* boolean: dry run */
} statemap_config_t;

struct statemap_entity;

typedef struct statemap_rect {
	long long smr_start;			/* nanosecond offset */
	long long smr_duration;			/* nanosecond duration */
	long long smr_weight;			/* my weight + neighbors */
	struct statemap_rect *smr_next;		/* next for entity */
	struct statemap_rect *smr_prev;		/* previous for entity */
	struct statemap_entity *smr_entity;	/* pointer back to entity */
	struct statemap_tag *smr_tags;		/* tags for this state */
	avl_node_t smr_node;			/* AVL node */
	long long smr_states[1];		/* time in each state */
} statemap_rect_t;

typedef struct statemap_tagdef {
	char *smtd_name;			/* name of this tag */
	int smtd_index;				/* index for this tag */
	int smtd_state;				/* state for this tag */
	char *smtd_json;			/* JSON description of tag */
	struct statemap_tagdef *smtd_next;	/* next def on global list */
	struct statemap_tagdef *smtd_hashnext;	/* next def on hash chain */
} statemap_tagdef_t;

typedef struct statemap_entity {
	char *sme_name;				/* name of this entity */
	char *sme_description;			/* description, if any */
	statemap_rect_t *sme_first;		/* first rect for this entity */
	statemap_rect_t *sme_last;		/* last rect for this entity */
	struct statemap_entity *sme_next;	/* next on global list */
	struct statemap_entity *sme_hashnext;	/* next on hash chain */
	long long sme_start;			/* start of current state */
	int sme_state;				/* current state */
	statemap_tagdef_t *sme_tagdef;		/* current tag def, if any */
} statemap_entity_t;

typedef struct statemap_tag {
	statemap_tagdef_t *smt_def;		/* tag definition */
	long long smt_duration;			/* duration for this tag */
	struct statemap_tag *smt_next;		/* next tag for this state */
} statemap_tag_t;

typedef struct statemap {
	statemap_config_t sm_config;		/* configuration options */
	long sm_line;				/* current line */
	char sm_errmsg[256];			/* error message */
	int sm_nstates;				/* number of possible states */
	long sm_ncoalesced;			/* number of coalesced rects */
	long sm_nevents;			/* number of events */
	long sm_nelisions;			/* number of elisions */
	statemap_entity_t *sm_hash[STATEMAP_ENTITY_HASHSIZE]; /* hash */
	statemap_entity_t *sm_entities;		/* list of entities */
	statemap_tagdef_t *sm_taghash[STATEMAP_TAGDEF_HASHSIZE]; /* tag hash */
	statemap_tagdef_t *sm_tagdefs;		/* list of tag defs */
	statemap_tagdef_t *sm_taglast;		/* last tag def */
	long sm_ntagdefs;			/* number of tag definitions */
	avl_tree_t sm_rects;			/* tree of rectangles */
	int sm_rectsize;			/* size of rect structure */
	statemap_rect_t *sm_freerect;		/* freelist of rectangles */
	statemap_tag_t *sm_freetag;		/* freelist of tags */
} statemap_t;

extern statemap_t *statemap_create(statemap_config_t *);
extern int statemap_ingest(statemap_t *, const char *filename);
extern const char *statemap_errmsg(statemap_t *);
extern void statemap_destroy(statemap_t *);

#ifdef  __cplusplus
}
#endif

#endif  /* _SYS_STATEMAP_H */
